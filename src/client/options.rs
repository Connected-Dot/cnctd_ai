use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ClientOptions {
    pub timeout: Option<Duration>,
    pub max_retries: u32,
    pub base_url: Option<String>,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(60)),
            max_retries: 3,
            base_url: None,
        }
    }
}