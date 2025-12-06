use async_openai::types::{CreateEmbeddingRequestArgs, EmbeddingInput as OpenAiEmbeddingInput};
use crate::error::Result;
use crate::client::config::OpenAiConfig;
use super::{EmbeddingRequest, EmbeddingResponse, Embedding, EmbeddingUsage, EmbeddingInput};

pub(crate) async fn embed(
    sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
    _config: &OpenAiConfig,
    request: &EmbeddingRequest,
) -> Result<EmbeddingResponse> {
    // Convert our input type to OpenAI's
    let input: OpenAiEmbeddingInput = match &request.input {
        EmbeddingInput::Single(s) => OpenAiEmbeddingInput::String(s.clone()),
        EmbeddingInput::Multiple(v) => OpenAiEmbeddingInput::StringArray(v.clone()),
    };

    // Build the request
    let mut builder = CreateEmbeddingRequestArgs::default();
    builder.model(&request.model).input(input);

    // Add dimensions if specified
    if let Some(dims) = request.dimensions {
        builder.dimensions(dims);
    }

    let openai_request = builder.build()?;

    // Make the API call - uses #[from] impl for async_openai::error::OpenAIError
    let response = sdk_client
        .embeddings()
        .create(openai_request)
        .await?;

    // Convert response to our types
    let embeddings = response
        .data
        .into_iter()
        .map(|e| Embedding {
            vector: e.embedding,
            index: e.index as usize,
        })
        .collect();

    Ok(EmbeddingResponse {
        embeddings,
        model: response.model,
        usage: EmbeddingUsage {
            prompt_tokens: response.usage.prompt_tokens,
            total_tokens: response.usage.total_tokens,
        },
    })
}
