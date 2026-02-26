use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::session::SessionState;
use super::source::KeyInferenceOverride;

/// Dynamically-built engine that maps JSON key names to entity types.
/// Auto-derives patterns from entity type names and merges optional overrides.
pub struct KeyInferenceEngine {
    /// key_pattern (lowercase) -> entity_type_name
    id_patterns: HashMap<String, String>,
    /// All known patterns that indicate an ID field
    id_field_set: HashSet<String>,
}

impl KeyInferenceEngine {
    /// Build from entity type names + optional overrides from the source URL.
    pub fn build(
        entity_type_names: &[String],
        overrides: &HashMap<String, KeyInferenceOverride>,
    ) -> Self {
        let mut id_patterns: HashMap<String, String> = HashMap::new();
        let mut id_field_set: HashSet<String> = HashSet::new();

        // Always include generic ID patterns
        id_field_set.insert("id".to_string());
        id_field_set.insert("_id".to_string());
        id_field_set.insert("_ids".to_string());

        for type_name in entity_type_names {
            let t = type_name.to_lowercase();

            // Auto-derive patterns: {type}_id, {type}id, {type}ids, {type}_ids
            let auto_patterns = vec![
                format!("{}_id", t),
                format!("{}id", t),
                format!("{}ids", t),
                format!("{}_ids", t),
            ];

            for pattern in &auto_patterns {
                id_patterns.insert(pattern.clone(), type_name.clone());
                id_field_set.insert(pattern.clone());
            }

            // Merge overrides (additive, not replacing)
            if let Some(overr) = overrides.get(&t) {
                for pattern in &overr.id_patterns {
                    let p = pattern.to_lowercase();
                    id_patterns.insert(p.clone(), type_name.clone());
                    id_field_set.insert(p);
                }
            }
        }

        Self {
            id_patterns,
            id_field_set,
        }
    }

    /// Check if a JSON key name suggests it holds an entity ID.
    pub fn is_id_field(&self, key: &str) -> bool {
        let k = key.to_lowercase();
        k == "id" || k.ends_with("_id") || k.ends_with("_ids") || self.id_field_set.contains(&k)
    }

    /// Infer entity type from a key name. Returns None if no match.
    pub fn infer_entity_type(&self, key: &str) -> Option<&str> {
        let k = key.to_lowercase();
        self.id_patterns.get(&k).map(|s| s.as_str())
    }
}

pub struct Obfuscator {
    session: Arc<SessionState>,
}

impl Obfuscator {
    pub fn new(session: Arc<SessionState>) -> Self {
        Self { session }
    }

    /// Max token length for streaming deobfuscation buffering.
    pub fn max_token_length(&self) -> usize {
        self.session
            .tokenizer
            .max_token_length(self.session.dictionary.entity_types())
    }

    // ── Interception 1: User message -> LLM ────────────────────────────
    /// Replace real entity names with HMAC tokens in user-facing text.
    pub fn obfuscate_user_message(&self, text: &str) -> String {
        self.session
            .tokenizer
            .obfuscate_names_in_text(text, &self.session.dictionary)
    }

    // ── Interception 2: LLM tool args -> MCP ───────────────────────────
    /// Replace tokens in tool call arguments with real IDs/names.
    pub fn deobfuscate_tool_args(&self, args: &serde_json::Value) -> serde_json::Value {
        self.walk_json_deobfuscate(args)
    }

    // ── Interception 3: MCP tool results -> LLM ────────────────────────
    /// Replace real entity data with tokens and scale numeric values.
    pub fn obfuscate_tool_result(&self, result: &str) -> String {
        // Try to parse as JSON for structured obfuscation
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(result) {
            let obfuscated = self.walk_json_obfuscate(&json, None);
            serde_json::to_string(&obfuscated).unwrap_or_else(|_| result.to_string())
        } else {
            // Plain text: replace entity names
            self.session
                .tokenizer
                .obfuscate_names_in_text(result, &self.session.dictionary)
        }
    }

    // ── Interception 4: LLM response -> client ─────────────────────────
    /// Replace tokens in LLM-generated text with real entity names.
    pub fn deobfuscate_llm_response(&self, text: &str) -> String {
        self.session
            .tokenizer
            .deobfuscate_text(text, &self.session.dictionary)
    }

    // ── Token map export ──────────────────────────────────────────────
    /// Export the full entity-to-token mapping for the obfuscation inspector.
    pub fn export_token_map(&self) -> serde_json::Value {
        let entries: Vec<serde_json::Value> = self
            .session
            .tokenizer
            .export_token_map(&self.session.dictionary)
            .into_iter()
            .map(|(entity_type, id, name, token)| {
                serde_json::json!({
                    "type": entity_type,
                    "id": id,
                    "name": name,
                    "token": token,
                })
            })
            .collect();

        serde_json::json!({
            "entity_count": entries.len(),
            "entries": entries,
        })
    }

    // ── JSON walkers ───────────────────────────────────────────────────

    /// Recursively walk JSON, replacing tokens with real values.
    fn walk_json_deobfuscate(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => {
                let token_re = self.session.tokenizer.token_regex();
                if token_re.is_match(s) {
                    // Normalise LLM-reformatted tokens before lookup
                    let normalised = s.to_lowercase().replace(' ', "_");
                    // Check if the entire string is a single token
                    if let Some((entity_type, id)) =
                        self.session.tokenizer.deobfuscate_token(&normalised)
                    {
                        // If used in a context that expects an ID (numeric), return the ID
                        // Otherwise return the name
                        if let Some(record) =
                            self.session.dictionary.lookup_by_id(&entity_type, &id)
                        {
                            return serde_json::Value::String(record.name.clone());
                        }
                    }
                    // Multiple tokens in a string or partial match: replace all tokens with names
                    let deobfuscated = self
                        .session
                        .tokenizer
                        .deobfuscate_text(s, &self.session.dictionary);
                    serde_json::Value::String(deobfuscated)
                } else {
                    value.clone()
                }
            }
            serde_json::Value::Array(arr) => serde_json::Value::Array(
                arr.iter().map(|v| self.walk_json_deobfuscate(v)).collect(),
            ),
            serde_json::Value::Object(obj) => {
                let mut new_obj = serde_json::Map::new();
                for (k, v) in obj {
                    // Check if this key expects an ID value
                    let new_v = if self.session.key_inference.is_id_field(k) {
                        self.deobfuscate_to_id(v)
                    } else {
                        self.walk_json_deobfuscate(v)
                    };
                    new_obj.insert(k.clone(), new_v);
                }
                serde_json::Value::Object(new_obj)
            }
            _ => value.clone(),
        }
    }

    /// If the value is a token string, replace with the original ID.
    /// Returns a JSON number if the ID is numeric, otherwise a JSON string.
    fn deobfuscate_to_id(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => {
                if let Some((_entity_type, id)) = self.session.tokenizer.deobfuscate_token(s) {
                    // Try to return as a number (Publica integer IDs), fall back to string (MongoDB ObjectIds)
                    if let Ok(n) = id.parse::<i64>() {
                        serde_json::Value::Number(serde_json::Number::from(n))
                    } else {
                        serde_json::Value::String(id)
                    }
                } else {
                    self.walk_json_deobfuscate(value)
                }
            }
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(|v| self.deobfuscate_to_id(v)).collect())
            }
            _ => self.walk_json_deobfuscate(value),
        }
    }

    /// Recursively walk JSON, replacing real entity data with tokens and scaling numbers.
    fn walk_json_obfuscate(
        &self,
        value: &serde_json::Value,
        parent_key: Option<&str>,
    ) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => {
                // If parent key is an ID field, try to obfuscate as entity ID first
                if let Some(key) = parent_key {
                    if self.session.key_inference.is_id_field(key) {
                        let result = self.try_obfuscate_id_value(key, s);
                        if result != serde_json::Value::String(s.clone()) {
                            return result;
                        }
                    }
                }
                // Replace entity names with tokens
                let obfuscated = self
                    .session
                    .tokenizer
                    .obfuscate_names_in_text(s, &self.session.dictionary);
                serde_json::Value::String(obfuscated)
            }
            serde_json::Value::Number(n) => {
                if let Some(key) = parent_key {
                    // Check if this key is an entity ID field
                    if self.session.key_inference.is_id_field(key) {
                        if let Some(f) = n.as_i64() {
                            // Try to find entity and replace with token
                            return self.try_obfuscate_id_value(key, &f.to_string());
                        }
                    }
                    // Check if this key is a metric that should be scaled
                    if self.session.scaler.is_known_metric(key) {
                        if let Some(f) = n.as_f64() {
                            let scaled = self.session.scaler.scale(key, f);
                            return serde_json::json!(scaled);
                        }
                    }
                }
                value.clone()
            }
            serde_json::Value::Array(arr) => serde_json::Value::Array(
                arr.iter()
                    .map(|v| self.walk_json_obfuscate(v, parent_key))
                    .collect(),
            ),
            serde_json::Value::Object(obj) => {
                let mut new_obj = serde_json::Map::new();
                for (k, v) in obj {
                    let new_v = self.walk_json_obfuscate(v, Some(k));
                    new_obj.insert(k.clone(), new_v);
                }
                serde_json::Value::Object(new_obj)
            }
            _ => value.clone(),
        }
    }

    /// Try to replace an ID value with an entity token.
    /// Checks the inferred entity type first, then falls back to trying all types.
    /// Returns the original value (as number if parseable, otherwise string) if no match.
    fn try_obfuscate_id_value(&self, key: &str, id: &str) -> serde_json::Value {
        // Try inferred entity type first
        if let Some(et) = self.session.key_inference.infer_entity_type(key) {
            if let Some(token) = self.session.tokenizer.obfuscate_id(et, id) {
                return serde_json::Value::String(token.to_string());
            }
        }

        // Fallback: try all known entity types
        for et in self.session.dictionary.entity_types() {
            if let Some(token) = self.session.tokenizer.obfuscate_id(et, id) {
                return serde_json::Value::String(token.to_string());
            }
        }

        // Not a known entity ID, pass through in original form
        if let Ok(n) = id.parse::<i64>() {
            serde_json::Value::Number(serde_json::Number::from(n))
        } else {
            serde_json::Value::String(id.to_string())
        }
    }
}
