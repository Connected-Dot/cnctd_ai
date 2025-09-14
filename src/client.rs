use crate::error::AiError;

pub struct AiClient {
    // keep this private so you can swap providers later
    base_url: String,
    api_key: String,
    default_model: String,
}

pub struct AskRequest<'a> {
    pub prompt: &'a str,
    pub system: Option<&'a str>,   // fine to pass None for now
    pub model: Option<&'a str>,    // overrides default_model
}

pub struct AskResponse {
    pub text: String,
    pub finish_reason: Option<String>, // future-friendly
    pub usage_tokens: Option<(u32, u32, u32)>, // prompt, completion, total
}

impl AiClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>, default_model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            default_model: default_model.into(),
        }
    }

    pub async fn ask(&self, req: AskRequest<'_>) -> Result<AskResponse, AiError> {
        // for v0: call the provider client directly here.
        // keep the provider-specific code *private* so you can change it later.
        // return AskResponse { ... }
        todo!()
    }
}
