use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestToolMessageArgs,
    ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
    FinishReason,
    ResponseFormat,
};
use async_openai::Client;

use crate::config::AiConfig;
use crate::error::AiError;
use crate::types::{AskRequest, Msg, Role, AskResponse, Usage};

pub async fn ask(
    http: &reqwest::Client,
    cfg: &AiConfig,
    req: &AskRequest,
) -> Result<AskResponse, AiError> {
    // --- client/config ---
    let key = cfg.openai_api_key.as_ref().ok_or(AiError::Auth)?;
    let mut oai_cfg = OpenAIConfig::new().with_api_base(cfg.openai_base_url.clone());
    oai_cfg = oai_cfg.with_api_key(key.clone());
    let client = Client::with_config(oai_cfg).with_http_client(http.clone());

    // --- messages ---
    let mut oa_msgs: Vec<ChatCompletionRequestMessage> =
        Vec::with_capacity(req.messages.len() + 1);

    if let Some(sys) = &req.system {
        oa_msgs.push(
            ChatCompletionRequestSystemMessageArgs::default()
                .content(sys.as_str()) // <- &str or String works; avoid &String
                .build()
                .map_err(|e| AiError::Provider(e.to_string()))?
                .into(),
        );
    }

    for m in &req.messages {
        match m {
            Msg { role: Role::User, content, .. } => {
                oa_msgs.push(
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(content.as_str())
                        .build()
                        .map_err(|e| AiError::Provider(e.to_string()))?
                        .into(),
                );
            }
            Msg { role: Role::Assistant, content, .. } => {
                oa_msgs.push(
                    ChatCompletionRequestAssistantMessageArgs::default()
                        .content(content.clone()) // pass String directly
                        .build()
                        .map_err(|e| AiError::Provider(e.to_string()))?
                        .into(),
                );
            }
            Msg { role: Role::Tool, content, name } => {
                oa_msgs.push(
                    ChatCompletionRequestToolMessageArgs::default()
                        .content(content.clone())
                        .tool_call_id(name.clone().unwrap_or_default())
                        .build()
                        .map_err(|e| AiError::Provider(e.to_string()))?
                        .into(),
                );
            }
            Msg { role: Role::System, content, .. } => {
                oa_msgs.push(
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(content.as_str())
                        .build()
                        .map_err(|e| AiError::Provider(e.to_string()))?
                        .into(),
                );
            }
        }
    }

    // --- request builder (methods return &mut self; don't reassign) ---
    let mut builder = CreateChatCompletionRequestArgs::default();
    builder
        .model(req.model.clone().unwrap_or_else(|| cfg.default_model.clone()))
        .messages(oa_msgs);

    if let Some(t) = req.options.temperature {
        builder.temperature(t);
    }
    if let Some(mx) = req.options.max_output_tokens {
        builder.max_tokens(mx); // <- u32 in 0.29.3
    }
    if req.options.json_mode.unwrap_or(false) {
        builder.response_format(ResponseFormat::JsonObject);
    }

    let request = builder
        .build()
        .map_err(|e| AiError::Provider(e.to_string()))?;

    // --- call ---
    let resp = client.chat().create(request).await.map_err(map_oai_err)?;

    // --- normalize ---
    let text = resp.choices
        .get(0)
        .and_then(|c| c.message.content.clone())
        .unwrap_or_default();

    let finish_reason = resp.choices
        .get(0)
        .and_then(|c| c.finish_reason.as_ref())
        .map(finish_reason_str)
        .unwrap_or("stop")
        .to_string();

    let usage = resp.usage.as_ref().map(|u| Usage {
        prompt_tokens: Some(u.prompt_tokens as u32),
        completion_tokens: Some(u.completion_tokens as u32),
        total_tokens: Some(u.total_tokens as u32),
    });

    Ok(AskResponse {
        text,
        finish_reason,
        usage,
        latency_ms: 0, // facade fills this
        provider_meta: serde_json::to_value(&resp).unwrap_or(serde_json::Value::Null),
    })
}

pub async fn get_embedding(
    http: &reqwest::Client,
    cfg: &AiConfig,
    input: &str,
) -> Result<Vec<f32>, AiError> {
    // --- client/config ---
    let key = cfg.openai_api_key.as_ref().ok_or(AiError::Auth)?;
    let mut oai_cfg = OpenAIConfig::new().with_api_base(cfg.openai_base_url.clone());
    oai_cfg = oai_cfg.with_api_key(key.clone());
    let client = Client::with_config(oai_cfg).with_http_client(http.clone());

    let model = "text-embedding-ada-002";

    let req = async_openai::types::CreateEmbeddingRequestArgs::default()
        .model(model)
        .input(vec![input.to_string()])
        .build()
        .map_err(|e| AiError::Provider(e.to_string()))?;

    println!("Embedding request: {:?}", req);

    let resp = client
        .embeddings()
        .create(req)
        .await
        .map_err(map_oai_err)?;

    let embedding = resp.data
        .get(0)
        .map(|e| e.embedding.clone())
        .unwrap_or_default();

    Ok(embedding)
}

fn finish_reason_str(fr: &FinishReason) -> &'static str {
    match fr {
        FinishReason::Stop => "stop",
        FinishReason::Length => "length",
        FinishReason::ToolCalls => "tool_call",
        FinishReason::ContentFilter => "content_filter",
        FinishReason::FunctionCall => "function_call",
    }
}

fn map_oai_err(e: async_openai::error::OpenAIError) -> AiError {
    use async_openai::error::OpenAIError as E;
    match e {
        E::ApiError(err) => {
            if let Some(code) = err.code {
                let s = code.to_string();
                if s.contains("401") { return AiError::Auth; }
                if s.contains("429") { return AiError::RateLimited; }
            }
            AiError::Provider(err.message)
        }
        E::StreamError(m) => AiError::Provider(m),
        E::Reqwest(e2) => {
            if e2.is_timeout() { AiError::Timeout } else { AiError::Provider(e2.to_string()) }
        }
        other => AiError::Provider(other.to_string()),
    }
}

