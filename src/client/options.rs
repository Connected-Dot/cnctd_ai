use crate::retry::RetryPolicy;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ClientOptions {
    pub timeout: Option<Duration>,
    pub retry_policy: RetryPolicy,
    pub base_url: Option<String>,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(60)),
            retry_policy: RetryPolicy::default(),
            base_url: None,
        }
    }
}
