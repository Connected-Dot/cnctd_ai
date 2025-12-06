mod openai;


/// Request for generating embeddings
#[derive(Debug, Clone)]
pub struct EmbeddingRequest {
    /// Text(s) to embed - can be a single string or multiple
    pub input: EmbeddingInput,
    /// Model to use for embedding (e.g., "text-embedding-3-small")
    pub model: String,
    /// Optional dimensions for models that support it (e.g., text-embedding-3-*)
    pub dimensions: Option<u32>,
}

/// Input can be a single text or multiple texts
#[derive(Debug, Clone)]
pub enum EmbeddingInput {
    Single(String),
    Multiple(Vec<String>),
}

impl From<String> for EmbeddingInput {
    fn from(s: String) -> Self {
        EmbeddingInput::Single(s)
    }
}

impl From<&str> for EmbeddingInput {
    fn from(s: &str) -> Self {
        EmbeddingInput::Single(s.to_string())
    }
}

impl From<Vec<String>> for EmbeddingInput {
    fn from(v: Vec<String>) -> Self {
        EmbeddingInput::Multiple(v)
    }
}

impl From<Vec<&str>> for EmbeddingInput {
    fn from(v: Vec<&str>) -> Self {
        EmbeddingInput::Multiple(v.into_iter().map(|s| s.to_string()).collect())
    }
}

/// Response containing generated embeddings
#[derive(Debug, Clone)]
pub struct EmbeddingResponse {
    /// The generated embeddings, one per input text
    pub embeddings: Vec<Embedding>,
    /// Model used
    pub model: String,
    /// Token usage information
    pub usage: EmbeddingUsage,
}

/// A single embedding vector
#[derive(Debug, Clone)]
pub struct Embedding {
    /// The embedding vector
    pub vector: Vec<f32>,
    /// Index of the input text this embedding corresponds to
    pub index: usize,
}

/// Token usage for embedding request
#[derive(Debug, Clone)]
pub struct EmbeddingUsage {
    /// Number of tokens in the input
    pub prompt_tokens: u32,
    /// Total tokens used
    pub total_tokens: u32,
}

impl EmbeddingRequest {
    /// Create a new embedding request with a single text
    pub fn new(input: impl Into<EmbeddingInput>, model: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            model: model.into(),
            dimensions: None,
        }
    }

    /// Set custom dimensions (for models that support it)
    pub fn with_dimensions(mut self, dimensions: u32) -> Self {
        self.dimensions = Some(dimensions);
        self
    }
}

/// Convenience function to create embedding request for text-embedding-3-small
pub fn embed_small(input: impl Into<EmbeddingInput>) -> EmbeddingRequest {
    EmbeddingRequest::new(input, "text-embedding-3-small")
}

/// Convenience function to create embedding request for text-embedding-3-large  
pub fn embed_large(input: impl Into<EmbeddingInput>) -> EmbeddingRequest {
    EmbeddingRequest::new(input, "text-embedding-3-large")
}

// Re-export the OpenAI implementation for use by Client
pub(crate) use openai::embed as openai_embed;
