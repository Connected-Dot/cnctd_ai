use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::entity_dictionary::{EntityDictionary, EntityRecord};
use super::numeric_scaler::NumericScaler;
use super::obfuscator::KeyInferenceEngine;
use super::source::fetch_from_source;
use super::tokenizer::HmacTokenizer;

const DEFAULT_SUFFIX_LENGTH: usize = 4;

pub struct SessionState {
    pub salt: String,
    pub dictionary: EntityDictionary,
    pub tokenizer: HmacTokenizer,
    pub scaler: NumericScaler,
    pub key_inference: KeyInferenceEngine,
    pub created_at: Instant,
}

pub struct SessionCache {
    key: String,
    source_url: String,
    source_token: String,
    sessions: RwLock<HashMap<String, Arc<SessionState>>>,
    ttl: Duration,
}

impl SessionCache {
    pub fn new(key: String, source_url: String, source_token: String, ttl: Duration) -> Self {
        Self {
            key,
            source_url,
            source_token,
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

        // Cache miss or expired: fetch from source URL
        let source = fetch_from_source(&self.source_url, &self.source_token).await?;

        // Convert EntityPayloads -> EntityRecords and load dictionary
        let records: Vec<EntityRecord> = source
            .entities
            .into_iter()
            .map(|e| EntityRecord {
                entity_type: e.entity_type,
                id: e.id,
                name: e.name,
            })
            .collect();

        let mut dictionary = EntityDictionary::new();
        dictionary.load(records);

        let stats = dictionary.stats();
        let summary: Vec<String> = stats.iter().map(|(k, v)| format!("{}:{}", k, v)).collect();
        tracing::info!(
            "Obfuscation session [salt={}...]: loaded entities [{}]",
            &salt[..salt.len().min(8)],
            summary.join(", ")
        );

        let entity_types = dictionary.entity_types().to_vec();

        // Build key inference engine from entity types + optional overrides
        let key_inference =
            KeyInferenceEngine::build(&entity_types, &source.key_inference_overrides);

        // Build tokenizer with dynamic entity type names
        let mut tokenizer =
            HmacTokenizer::new(&self.key, salt, DEFAULT_SUFFIX_LENGTH, &entity_types);
        tokenizer.build(&dictionary);

        // Build numeric scaler from dynamic rules or fall back to defaults
        let scaler = match source.numeric_rules {
            Some(ref rules) => NumericScaler::new_from_rules(rules),
            None => NumericScaler::new_random(),
        };

        let session = Arc::new(SessionState {
            salt: salt.to_string(),
            dictionary,
            tokenizer,
            scaler,
            key_inference,
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

    /// Remove a session from the cache (called by the invalidation endpoint).
    pub async fn invalidate(&self, salt: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        sessions.remove(salt).is_some()
    }

    /// Remove all sessions from the cache.
    pub async fn invalidate_all(&self) -> usize {
        let mut sessions = self.sessions.write().await;
        let count = sessions.len();
        sessions.clear();
        count
    }

    /// The source token, used to authenticate invalidation requests.
    pub fn source_token(&self) -> &str {
        &self.source_token
    }
}
