use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestToolMessageArgs,
    ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
    CreateChatCompletionRequest, // <-- add this import
    FinishReason,
    ResponseFormat,
};
use async_openai::Client;
use futures_util::StreamExt;
use futures_core::Stream;
use async_stream::try_stream;

use crate::ask::config::AskConfig;
use crate::ask::msg::{Msg, Role};
use crate::ask::request::{AskChunk, AskRequest};
use crate::ask::response::{AskResponse, Usage};
use crate::error::AiError;
use crate::util::get_http_client;

pub struct OpenAiApi;

impl OpenAiApi {
    pub async fn get_client(config: &AskConfig) -> Result<Client<OpenAIConfig>, AiError> {
        let http_client = get_http_client(&config)?;
        let api_key = config.api_key.clone();
        let url = config.url.clone();
        let oai_cfg = OpenAIConfig::new().with_api_base(url).with_api_key(api_key);
        Ok(Client::with_config(oai_cfg).with_http_client(http_client))
    }

    pub async fn ask(
        config: AskConfig,
        request: &AskRequest,
    ) -> Result<AskResponse, AiError> {
        let client = Self::get_client(&config).await?;
        let req = build_openai_request(&config, request)?; // <-- build once here

        let resp = client.chat().create(req).await.map_err(map_oai_err)?;

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
            latency_ms: 0,
            provider_meta: serde_json::to_value(&resp).unwrap_or(serde_json::Value::Null),
        })
    }

    pub async fn ask_stream(
        config: AskConfig,
        request: &AskRequest,
    ) -> Result<impl Stream<Item = Result<AskChunk, AiError>> + Send, AiError> {
        

        let client = Self::get_client(&config).await?;
        let req = build_openai_request(&config, request)?; // <-- reuse same builder logic

        let mut stream = client.chat().create_stream(req).await.map_err(map_oai_err)?;

        let mut full_text = String::new();
        let mut finish_reason: Option<String> = None;
        let mut provider_meta = serde_json::json!({});
        let mut usage: Option<Usage> = None;

        let s = try_stream! {
            while let Some(event) = stream.next().await {
                let chunk = event.map_err(map_oai_err)?;
                provider_meta = serde_json::to_value(&chunk).unwrap_or(serde_json::Value::Null);

                if let Some(choice) = chunk.choices.get(0) {
                    let delta = &choice.delta;

                    if let Some(ct) = &delta.content {
                        full_text.push_str(ct);
                        yield AskChunk::Delta { text: ct.clone() };
                    }

                    if let Some(tcs) = &delta.tool_calls {
                        for tc in tcs {
                            let id = tc.id.clone().unwrap_or_default();
                            let name = tc.function.as_ref().and_then(|f| f.name.clone());
                            let args_delta = tc.function.as_ref().and_then(|f| f.arguments.clone());
                            yield AskChunk::ToolCallDelta { tool_call_id: id, name, args_delta };
                        }
                    }

                    if let Some(role) = &delta.role {
                        yield AskChunk::Role(role.to_string());
                    }

                    if let Some(fr) = &choice.finish_reason {
                        finish_reason = Some(finish_reason_str(fr).to_string());
                    }
                }


                if let Some(u) = &chunk.usage {
                    usage = Some(Usage {
                        prompt_tokens: Some(u.prompt_tokens as u32),
                        completion_tokens: Some(u.completion_tokens as u32),
                        total_tokens: Some(u.total_tokens as u32),
                    });
                }
            }

            let resp = AskResponse {
                text: full_text,
                finish_reason: finish_reason.unwrap_or_else(|| "stop".to_string()),
                usage,
                latency_ms: 0,
                provider_meta,
            };
            yield AskChunk::Complete(resp);
        };

        Ok(s)
    }

    pub async fn get_models(config: &AskConfig) -> Result<serde_json::Value, AiError> {
        let client = Self::get_client(config).await?;
        let models = client.models().list().await.map_err(|e| AiError::Provider(e.to_string()))?.data;
        Ok(models.into_iter().map(|m| m.id).collect::<Vec<String>>().into())
    }

    pub async fn get_embedding(
        text: &str,
        config: &AskConfig,
        model: Option<&str>,
    ) -> Result<Vec<f32>, AiError> {
        let client = Self::get_client(config).await?;
        let model = model.unwrap_or("text-embedding-ada-002");

        let req = async_openai::types::CreateEmbeddingRequestArgs::default()
            .model(model)
            .input(vec![text.to_string()])
            .build()
            .map_err(|e| AiError::Provider(e.to_string()))?;

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

fn build_openai_request(
    config: &AskConfig,
    request: &AskRequest,
) -> Result<CreateChatCompletionRequest, AiError> {
    let mut oa_msgs: Vec<ChatCompletionRequestMessage> =
        Vec::with_capacity(request.messages.len() + 1);

    if let Some(sys) = &request.system {
        oa_msgs.push(
            ChatCompletionRequestSystemMessageArgs::default()
                .content(sys.as_str())
                .build()
                .map_err(|e| AiError::Provider(e.to_string()))?
                .into(),
        );
    }

    for m in &request.messages {
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
                        .content(content.clone())
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

    let mut builder = CreateChatCompletionRequestArgs::default();
    builder.model(config.model.as_str()).messages(oa_msgs);

    if let Some(t) = request.options.temperature {
        builder.temperature(t);
    }
    if let Some(mx) = request.options.max_output_tokens {
        builder.max_tokens(mx);
    }
    if request.options.json_mode.unwrap_or(false) {
        builder.response_format(ResponseFormat::JsonObject);
    }

    builder.build().map_err(|e| AiError::Provider(e.to_string()))
}
