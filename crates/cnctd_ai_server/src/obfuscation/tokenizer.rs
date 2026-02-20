use aho_corasick::AhoCorasick;
use hmac::{Hmac, Mac};
use regex::Regex;
use sha2::Sha256;
use std::collections::{HashMap, HashSet};

use super::entity_dictionary::EntityDictionary;

type HmacSha256 = Hmac<Sha256>;

pub struct HmacTokenizer {
    key: String,
    salt: String,
    suffix_length: usize,
    /// (entity_type, id) -> token
    id_to_token: HashMap<(String, i32), String>,
    /// token -> (entity_type, id)
    token_to_id: HashMap<String, (String, i32)>,
    /// lowercase name -> token (first match wins for ambiguous names)
    name_to_token: HashMap<String, String>,
    used_tokens: HashSet<String>,
    token_pattern: Regex,
    /// Pre-compiled Aho-Corasick automaton for name obfuscation.
    /// Patterns are lowercase entity names sorted longest-first.
    ac_automaton: Option<AhoCorasick>,
    /// Replacement tokens indexed by AC pattern index.
    ac_replacements: Vec<String>,
    /// Original pattern lengths indexed by AC pattern index (for word boundary checks).
    ac_pattern_lengths: Vec<usize>,
}

impl HmacTokenizer {
    pub fn new(key: &str, salt: &str, suffix_length: usize, entity_type_names: &[String]) -> Self {
        let type_alternation = entity_type_names
            .iter()
            .map(|t| regex::escape(t))
            .collect::<Vec<_>>()
            .join("|");
        let pattern = format!(r"\b({})_[0-9a-f]{{{},}}\b", type_alternation, suffix_length);
        let token_pattern = Regex::new(&pattern).expect("Invalid token regex");

        Self {
            key: key.to_string(),
            salt: salt.to_string(),
            suffix_length,
            id_to_token: HashMap::new(),
            token_to_id: HashMap::new(),
            name_to_token: HashMap::new(),
            used_tokens: HashSet::new(),
            token_pattern,
            ac_automaton: None,
            ac_replacements: Vec::new(),
            ac_pattern_lengths: Vec::new(),
        }
    }

    /// Build all token mappings from the entity dictionary.
    pub fn build(&mut self, dictionary: &EntityDictionary) {
        self.id_to_token.clear();
        self.token_to_id.clear();
        self.name_to_token.clear();
        self.used_tokens.clear();

        for record in dictionary.all_records() {
            let token = self.generate_unique_token(&record.entity_type, record.id);
            self.id_to_token
                .insert((record.entity_type.clone(), record.id), token.clone());
            self.token_to_id
                .insert(token.clone(), (record.entity_type.clone(), record.id));

            // Map name -> token (first record wins for duplicate names)
            let lower_name = record.name.to_lowercase();
            self.name_to_token.entry(lower_name).or_insert(token);
        }

        // Pre-compile Aho-Corasick automaton for name obfuscation
        self.build_ac_automaton(dictionary);
    }

    /// Build a single Aho-Corasick automaton from all entity names.
    /// Sorted longest-first so longer matches take priority.
    fn build_ac_automaton(&mut self, dictionary: &EntityDictionary) {
        let mut names: Vec<&str> = dictionary.all_names().into_iter().collect();
        names.sort_by(|a, b| b.len().cmp(&a.len()));

        let mut patterns: Vec<String> = Vec::with_capacity(names.len());
        let mut replacements: Vec<String> = Vec::with_capacity(names.len());
        let mut pattern_lengths: Vec<usize> = Vec::with_capacity(names.len());

        for name in &names {
            if let Some(token) = self.name_to_token.get(*name) {
                // Patterns are lowercase for case-insensitive matching
                patterns.push(name.to_string());
                replacements.push(token.clone());
                pattern_lengths.push(name.len());
            }
        }

        if patterns.is_empty() {
            self.ac_automaton = None;
            self.ac_replacements.clear();
            self.ac_pattern_lengths.clear();
            return;
        }

        // Build case-insensitive automaton with leftmost-longest matching
        let ac = aho_corasick::AhoCorasickBuilder::new()
            .ascii_case_insensitive(true)
            .match_kind(aho_corasick::MatchKind::LeftmostLongest)
            .build(&patterns)
            .expect("Failed to build Aho-Corasick automaton");

        self.ac_automaton = Some(ac);
        self.ac_replacements = replacements;
        self.ac_pattern_lengths = pattern_lengths;
    }

    fn generate_unique_token(&mut self, entity_type: &str, id: i32) -> String {
        let base_input = format!("{}:{}", entity_type, id);

        for attempt in 0..100 {
            let input = if attempt == 0 {
                base_input.clone()
            } else {
                format!("{}:attempt_{}", base_input, attempt)
            };

            let hmac_key = format!("{}{}", self.key, self.salt);
            let mut mac =
                HmacSha256::new_from_slice(hmac_key.as_bytes()).expect("HMAC accepts any key size");
            mac.update(input.as_bytes());
            let result = mac.finalize().into_bytes();
            let hex_str = hex::encode(result);
            let suffix = &hex_str[..self.suffix_length];
            let token = format!("{}_{}", entity_type, suffix);

            if !self.used_tokens.contains(&token) {
                self.used_tokens.insert(token.clone());
                return token;
            }
        }

        // Fallback: use full hex (virtually impossible to collide)
        let hmac_key = format!("{}{}", self.key, self.salt);
        let mut mac =
            HmacSha256::new_from_slice(hmac_key.as_bytes()).expect("HMAC accepts any key size");
        mac.update(base_input.as_bytes());
        let result = mac.finalize().into_bytes();
        let token = format!("{}_{}", entity_type, hex::encode(result));
        self.used_tokens.insert(token.clone());
        token
    }

    pub fn obfuscate_id(&self, entity_type: &str, id: i32) -> Option<&str> {
        self.id_to_token
            .get(&(entity_type.to_string(), id))
            .map(|s| s.as_str())
    }

    pub fn deobfuscate_token(&self, token: &str) -> Option<(String, i32)> {
        self.token_to_id.get(token).cloned()
    }

    pub fn obfuscate_name(&self, name: &str) -> Option<&str> {
        self.name_to_token
            .get(&name.to_lowercase())
            .map(|s| s.as_str())
    }

    /// Resolve a token back to the entity's real name.
    pub fn deobfuscate_to_name(
        &self,
        token: &str,
        dictionary: &EntityDictionary,
    ) -> Option<String> {
        let (entity_type, id) = self.deobfuscate_token(token)?;
        dictionary
            .lookup_by_id(&entity_type, id)
            .map(|r| r.name.clone())
    }

    pub fn token_regex(&self) -> &Regex {
        &self.token_pattern
    }

    /// Replace all tokens in text with their real entity names.
    pub fn deobfuscate_text(&self, text: &str, dictionary: &EntityDictionary) -> String {
        self.token_pattern
            .replace_all(text, |caps: &regex::Captures| {
                let token = caps.get(0).unwrap().as_str();
                self.deobfuscate_to_name(token, dictionary)
                    .unwrap_or_else(|| token.to_string())
            })
            .into_owned()
    }

    /// Export the full token map as (entity_type, id, name, token) tuples.
    /// Used to emit the token_map SSE event for the obfuscation inspector.
    pub fn export_token_map(
        &self,
        dictionary: &EntityDictionary,
    ) -> Vec<(String, i32, String, String)> {
        self.id_to_token
            .iter()
            .map(|((entity_type, id), token)| {
                let name = dictionary
                    .lookup_by_id(entity_type, *id)
                    .map(|r| r.name.clone())
                    .unwrap_or_default();
                (entity_type.clone(), *id, name, token.clone())
            })
            .collect()
    }

    /// Replace all known entity names in text with their tokens.
    /// Uses pre-compiled Aho-Corasick automaton for O(text_length) matching.
    pub fn obfuscate_names_in_text(&self, text: &str, _dictionary: &EntityDictionary) -> String {
        let ac = match &self.ac_automaton {
            Some(ac) => ac,
            None => return text.to_string(),
        };

        let text_bytes = text.as_bytes();
        let text_len = text_bytes.len();
        let mut result = String::with_capacity(text.len());
        let mut last_end = 0;

        for mat in ac.find_iter(text) {
            let start = mat.start();
            let end = mat.end();

            // Word boundary check: char before match must be non-alphanumeric (or start of string)
            if start > 0 {
                let before = text_bytes[start - 1];
                if before.is_ascii_alphanumeric() || before == b'_' {
                    continue;
                }
            }
            // Char after match must be non-alphanumeric (or end of string)
            if end < text_len {
                let after = text_bytes[end];
                if after.is_ascii_alphanumeric() || after == b'_' {
                    continue;
                }
            }

            let pattern_idx = mat.pattern().as_usize();
            let replacement = &self.ac_replacements[pattern_idx];

            result.push_str(&text[last_end..start]);
            result.push_str(replacement);
            last_end = end;
        }

        result.push_str(&text[last_end..]);
        result
    }

    /// Compute the maximum possible token length for this tokenizer.
    /// Used by StreamingDeobfuscator to know how much to buffer.
    pub fn max_token_length(&self, entity_type_names: &[String]) -> usize {
        let longest_type = entity_type_names.iter().map(|t| t.len()).max().unwrap_or(0);
        // token format: {type}_{hex_suffix}
        longest_type + 1 + self.suffix_length
    }
}
