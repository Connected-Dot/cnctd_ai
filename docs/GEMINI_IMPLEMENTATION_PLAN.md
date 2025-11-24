# Google Gemini Integration Plan

This document outlines the implementation plan for adding Google Gemini support to cnctd_ai.

## Overview

Add Google Gemini as a third provider alongside Anthropic and OpenAI, following the existing architectural patterns.

## Implementation Approach

Use direct HTTP implementation (like the Anthropic streaming workaround) rather than a third-party SDK. This provides full control and consistency with existing patterns.

## Files to Modify

### 1. `src/client/config.rs`

Add `GeminiConfig` struct:

```rust
#[derive(Clone, Debug)]
pub struct GeminiConfig {
    pub api_key: String,
    pub model: String,
}
```

Update the `pub use` statement in `mod.rs` to export `GeminiConfig`.

### 2. `src/client/mod.rs`

#### Add to ProviderType enum:

```rust
#[derive(Clone)]
enum ProviderType {
    Anthropic { config: AnthropicConfig },
    OpenAi {
        sdk_client: async_openai::Client<async_openai::config::OpenAIConfig>,
        config: OpenAiConfig,
    },
    Gemini { config: GeminiConfig },
}
```

#### Add constructor:

```rust
pub fn gemini(
    config: GeminiConfig,
    options: Option<ClientOptions>,
) -> Result<Self, crate::error::Error> {
    let options = options.unwrap_or_default();
    
    Ok(Self {
        provider: ProviderType::Gemini { config },
        options,
    })
}
```

#### Update complete() method:

Add match arm for Gemini in the `complete()` method.

#### Implement complete_gemini():

```rust
async fn complete_gemini(
    &self,
    config: &GeminiConfig,
    request: &crate::request::CompletionRequest,
) -> crate::error::Result<crate::response::CompletionResponse>
```

#### Update complete_stream() method:

Add match arm for Gemini.

#### Implement stream_gemini():

```rust
async fn stream_gemini(
    &self,
    config: &GeminiConfig,
    request: &crate::request::CompletionRequest,
) -> crate::error::Result<crate::stream::CompletionStream>
```

### 3. `src/stream.rs`

#### Add to StreamType enum:

```rust
enum StreamType {
    AnthropicCustom(...),
    OpenAi(...),
    GeminiCustom(
        std::pin::Pin<
            Box<
                dyn futures::Stream<Item = Result<eventsource_stream::Event, eventsource_stream::EventStreamError<reqwest::Error>>>
                    + Send
            >
        >
    ),
}
```

#### Add constructor:

```rust
pub fn gemini_custom(
    stream: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
    model: String,
) -> Self
```

#### Update next() method:

Add handling for `StreamType::GeminiCustom` in the match statement.

#### Add SSE handler:

```rust
async fn handle_gemini_sse_event(
    &mut self,
    event: eventsource_stream::Event,
) -> Option<Option<StreamChunk>>
```

### 4. `src/error.rs`

Add Gemini-specific error variant:

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // ... existing variants ...
    
    #[error("Gemini API error: {0}")]
    GeminiError(String),
}
```

### 5. `src/lib.rs`

Update re-exports to include `GeminiConfig`.

## API Mapping Reference

### Endpoint

```
POST https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={API_KEY}
POST https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?alt=sse&key={API_KEY}
```

### Message Role Mapping

| cnctd_ai | Gemini |
|----------|--------|
| `Role::System` | `systemInstruction.parts[].text` |
| `Role::User` | `contents[].role = "user"` |
| `Role::Assistant` | `contents[].role = "model"` |

### Request Body Structure

```json
{
    "systemInstruction": {
        "parts": [{ "text": "system message here" }]
    },
    "contents": [
        {
            "role": "user",
            "parts": [{ "text": "user message" }]
        },
        {
            "role": "model", 
            "parts": [{ "text": "assistant response" }]
        }
    ],
    "tools": [{
        "functionDeclarations": [{
            "name": "tool_name",
            "description": "tool description",
            "parameters": { ... json schema ... }
        }]
    }],
    "generationConfig": {
        "temperature": 0.7,
        "maxOutputTokens": 4096,
        "topP": 0.9
    }
}
```

### Tool Call Handling

#### Assistant tool call (in response):

```json
{
    "candidates": [{
        "content": {
            "role": "model",
            "parts": [{
                "functionCall": {
                    "name": "tool_name",
                    "args": { ... }
                }
            }]
        }
    }]
}
```

#### Tool result (in next request):

```json
{
    "role": "user",
    "parts": [{
        "functionResponse": {
            "name": "tool_name",
            "response": { "result": "tool output here" }
        }
    }]
}
```

Note: Gemini doesn't use tool_call_id like OpenAI/Anthropic. It matches by function name.

### Streaming Response Format

Gemini streaming returns JSON chunks (not SSE data fields). Each chunk:

```json
{
    "candidates": [{
        "content": {
            "parts": [{ "text": "delta text" }],
            "role": "model"
        },
        "finishReason": "STOP"
    }],
    "usageMetadata": {
        "promptTokenCount": 10,
        "candidatesTokenCount": 20,
        "totalTokenCount": 30
    }
}
```

### Finish Reason Mapping

| Gemini | cnctd_ai |
|--------|----------|
| `STOP` | `FinishReason::Stop` |
| `MAX_TOKENS` | `FinishReason::Length` |
| `SAFETY` | `FinishReason::ContentFilter` |
| `RECITATION` | `FinishReason::ContentFilter` |
| Other | `FinishReason::Other` |

## Implementation Order

1. Add `GeminiConfig` to `config.rs`
2. Add `GeminiError` to `error.rs`
3. Extend `ProviderType` enum and add `gemini()` constructor
4. Implement `complete_gemini()` for non-streaming
5. Test non-streaming completion
6. Extend `StreamType` and add `gemini_custom()` constructor
7. Implement `stream_gemini()` and `handle_gemini_sse_event()`
8. Test streaming completion
9. Test tool calling (both streaming and non-streaming)
10. Update lib.rs exports
11. Add example in `examples/` directory
12. Update README.md

## Testing Checklist

- [ ] Basic completion (no tools)
- [ ] Completion with system message
- [ ] Multi-turn conversation
- [ ] Streaming completion
- [ ] Tool declaration
- [ ] Tool call detection
- [ ] Tool result handling
- [ ] Multi-tool scenarios
- [ ] Error handling (invalid API key, rate limits, etc.)

## Notes

- Gemini API key goes in query param, not header (different from Anthropic/OpenAI)
- No tool_call_id concept - tool results matched by function name
- Parts array can contain mixed content (text + function calls)
- Model names: `gemini-2.0-flash`, `gemini-1.5-pro`, `gemini-1.5-flash`, `gemini-1.5-pro-latest`
