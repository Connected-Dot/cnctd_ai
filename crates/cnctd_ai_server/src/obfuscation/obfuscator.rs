use std::sync::Arc;

use super::entity_dictionary::EntityType;
use super::session::SessionState;

pub struct Obfuscator {
    session: Arc<SessionState>,
}

impl Obfuscator {
    pub fn new(session: Arc<SessionState>) -> Self {
        Self { session }
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
    /// Numeric deobfuscation is deferred to Phase 5.
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
                    // Check if the entire string is a single token
                    if let Some((entity_type, id)) = self.session.tokenizer.deobfuscate_token(s) {
                        // If used in a context that expects an ID (numeric), return the ID
                        // Otherwise return the name
                        if let Some(record) = self.session.dictionary.lookup_by_id(entity_type, id)
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
                    let new_v = if self.is_id_field(k) {
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

    /// Check if a JSON key name suggests it holds an entity ID.
    fn is_id_field(&self, key: &str) -> bool {
        let k = key.to_lowercase();
        k.ends_with("_id")
            || k.ends_with("_ids")
            || k == "id"
            || k == "channelids"
            || k == "bidderids"
    }

    /// If the value is a token string, replace with the numeric ID.
    fn deobfuscate_to_id(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => {
                if let Some((_entity_type, id)) = self.session.tokenizer.deobfuscate_token(s) {
                    serde_json::Value::Number(serde_json::Number::from(id))
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
                    if self.is_id_field(key) {
                        if let Some(f) = n.as_i64() {
                            // Try to find entity and replace with token
                            return self.try_obfuscate_id_value(key, f as i32);
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

    /// Try to replace a numeric ID with an entity token.
    /// Checks all entity types for this key prefix.
    fn try_obfuscate_id_value(&self, key: &str, id: i32) -> serde_json::Value {
        // Infer entity type from key name
        let entity_type = self.infer_entity_type_from_key(key);

        if let Some(et) = entity_type {
            if let Some(token) = self.session.tokenizer.obfuscate_id(et, id) {
                return serde_json::Value::String(token.to_string());
            }
        }

        // If we can't infer the type, try all types
        for et in EntityType::all() {
            if let Some(token) = self.session.tokenizer.obfuscate_id(*et, id) {
                return serde_json::Value::String(token.to_string());
            }
        }

        // Not a known entity ID, pass through
        serde_json::Value::Number(serde_json::Number::from(id))
    }

    fn infer_entity_type_from_key(&self, key: &str) -> Option<EntityType> {
        let k = key.to_lowercase();
        if k.contains("channel") {
            Some(EntityType::Channel)
        } else if k.contains("bidder") {
            Some(EntityType::Bidder)
        } else if k.contains("advertiser") {
            Some(EntityType::Advertiser)
        } else if k.contains("agency") {
            Some(EntityType::Agency)
        } else if k.contains("order") {
            Some(EntityType::Order)
        } else if k.contains("line_item") || k.contains("lineitem") {
            Some(EntityType::LineItem)
        } else if k.contains("trafficker") {
            Some(EntityType::Trafficker)
        } else {
            None
        }
    }
}
