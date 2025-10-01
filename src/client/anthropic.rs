use anthropic_sdk::Client;
use futures_core::Stream;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use async_stream::try_stream;

use crate::{
    ask::{
        config::AskConfig,
        msg::{Msg, Role},
        request::{AskChunk, AskRequest},
        response::{AskResponse, Usage},
    },
    error::AiError,
};

pub struct AnthropicApi;

impl AnthropicApi {
    /// Non-streaming: builds once and collects full text.
    pub async fn ask(
        config: AskConfig,
        request: &AskRequest,
    ) -> Result<AskResponse, AiError> {
        let messages = build_anthropic_messages(request);

        let mut builder = Client::new()
            .auth(&config.api_key)
            .model(&config.model)
            .max_tokens(request.options.max_output_tokens.unwrap_or(1024) as i32)
            .messages(&json!(messages));

        if let Some(system) = &request.system {
            builder = builder.system(system);
        }
        if let Some(t) = request.options.temperature {
            builder = builder.temperature(t);
        }

        let req = builder.build().map_err(|e| AiError::Provider(e.to_string()))?;

        // Accumulate streamed content from the callback into a buffer
        let acc = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let acc2 = acc.clone();

        req.execute(move |content| {
            let acc = acc2.clone();
            async move {
                if let Ok(mut s) = acc.lock() {
                    s.push_str(&content);
                }
            }
        })
        .await
        .map_err(|e| AiError::Provider(e.to_string()))?;

        let final_text = acc.lock().unwrap().clone();

        Ok(AskResponse {
            text: final_text,
            finish_reason: "stop".to_string(), // SDK doesn't expose finish_reason here
            usage: None,                        // not available here; fill upstream if needed
            latency_ms: 0,
            provider_meta: Value::Null,
        })
    }

    /// Streaming: exposes a Stream of AskChunk::Delta then AskChunk::Complete.
    pub async fn ask_stream(
        config: AskConfig,
        request: &AskRequest,
    ) -> Result<impl Stream<Item = Result<AskChunk, AiError>> + Send, AiError> {
        

        let messages = build_anthropic_messages(request);

        let mut builder = Client::new()
            .auth(&config.api_key)
            .model(&config.model)
            .max_tokens(request.options.max_output_tokens.unwrap_or(1024) as i32)
            .messages(&json!(messages));

        if let Some(system) = &request.system {
            builder = builder.system(system);
        }
        if let Some(t) = request.options.temperature {
            builder = builder.temperature(t);
        }

        let req = builder.build().map_err(|e| AiError::Provider(e.to_string()))?;

        // Bridge SDK callback -> Stream via channel
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let (err_tx, mut err_rx) = mpsc::unbounded_channel::<AiError>();

        // Spawn producer
        let handle = tokio::spawn({
            let tx = tx.clone();
            async move {
                let res = req
                    .execute(move |content| {
                        let tx = tx.clone();
                        async move {
                            let _ = tx.send(content.to_string());
                        }
                    })
                    .await;
                if let Err(e) = res {
                    let _ = err_tx.send(AiError::Provider(e.to_string()));
                }
                // dropping tx closes channel
            }
        });

        let s = try_stream! {
            let mut full_text = String::new();
            let mut stream_err: Option<AiError> = None;

            loop {
                tokio::select! {
                    Some(err) = err_rx.recv() => {
                        let _ = handle.await;
                        stream_err = Some(err);
                        break;
                    }
                    Some(delta) = rx.recv() => {
                        full_text.push_str(&delta);
                        yield AskChunk::Delta { text: delta };
                    }
                    else => {
                        let _ = handle.await;
                        let resp = AskResponse {
                            text: full_text,
                            finish_reason: "stop".to_string(),
                            usage: None,
                            latency_ms: 0,
                            provider_meta: Value::Null,
                        };
                        yield AskChunk::Complete(resp);
                        break;
                    }
                }
            }

            if let Some(err) = stream_err {
                Err::<(), AiError>(err)?;
            }
        };

        Ok(s)
    }

    // keep this exactly as you had it
    pub async fn get_models(_config: &AskConfig) -> Result<Value, AiError> {
        let models = vec![
            "default",
            "sonnet",
            "opus",
            "haiku",
            "sonnet[1m]",
            "opusplan"
        ];
        Ok(json!(models))
    }
}

fn build_anthropic_messages(request: &AskRequest) -> Vec<Value> {
    // System goes in `.system(...)`; tool messages will map later to tool blocks/tool_result.
    let mut messages = Vec::with_capacity(request.messages.len());
    for m in &request.messages {
        match m {
            Msg { role: Role::User, content, .. } => {
                messages.push(json!({ "role": "user", "content": content }));
            }
            Msg { role: Role::Assistant, content, .. } => {
                messages.push(json!({ "role": "assistant", "content": content }));
            }
            Msg { role: Role::Tool, .. } | Msg { role: Role::System, .. } => {}
        }
    }
    messages
}
