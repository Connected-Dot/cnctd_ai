use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::entity_dictionary::{self, EntityDictionary};
use super::numeric_scaler::NumericScaler;
use super::tokenizer::HmacTokenizer;

const DEFAULT_SUFFIX_LENGTH: usize = 4;

pub struct SessionState {
    pub salt: String,
    pub dictionary: EntityDictionary,
    pub tokenizer: HmacTokenizer,
    pub scaler: NumericScaler,
    pub created_at: Instant,
}

pub struct SessionCache {
    key: String,
    pg_conn_string: String,
    sessions: RwLock<HashMap<String, Arc<SessionState>>>,
    ttl: Duration,
}

impl SessionCache {
    pub fn new(key: String, pg_conn_string: String, ttl: Duration) -> Self {
        Self {
            key,
            pg_conn_string,
            sessions: RwLock::new(HashMap::new()),
            ttl,
        }
    }

    /// Get an existing session or create a new one for this salt.
    pub async fn get_or_create(
        &self,
        salt: &str,
    ) -> Result<Arc<SessionState>, Box<dyn std::error::Error + Send + Sync>> {
        // Check cache first (read lock)
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(salt) {
                if session.created_at.elapsed() < self.ttl {
                    return Ok(session.clone());
                }
            }
        }

        // Cache miss or expired: build new session
        let records = entity_dictionary::load_from_postgres(&self.pg_conn_string).await?;

        let mut dictionary = EntityDictionary::new();
        dictionary.load(records);

        let stats = dictionary.stats();
        let summary: Vec<String> = stats.iter().map(|(k, v)| format!("{}:{}", k, v)).collect();
        tracing::info!(
            "Obfuscation session [salt={}...]: loaded entities [{}]",
            &salt[..salt.len().min(8)],
            summary.join(", ")
        );

        let mut tokenizer = HmacTokenizer::new(&self.key, salt, DEFAULT_SUFFIX_LENGTH);
        tokenizer.build(&dictionary);

        let scaler = NumericScaler::new_random();

        let session = Arc::new(SessionState {
            salt: salt.to_string(),
            dictionary,
            tokenizer,
            scaler,
            created_at: Instant::now(),
        });

        // Store in cache (write lock)
        {
            let mut sessions = self.sessions.write().await;
            // Lazy cleanup of expired entries
            sessions.retain(|_, s| s.created_at.elapsed() < self.ttl);
            sessions.insert(salt.to_string(), session.clone());
        }

        Ok(session)
    }
}
