use serde::Deserialize;
use std::collections::HashMap;

/// Response schema for the obfuscation source URL.
/// The calling application hosts an HTTP endpoint that returns this payload.
#[derive(Debug, Deserialize)]
pub struct ObfuscationSourceResponse {
    /// The full entity dictionary.
    pub entities: Vec<EntityPayload>,

    /// Optional extra key-inference patterns beyond the auto-derived ones.
    /// Keyed by entity type name (lowercase).
    #[serde(default)]
    pub key_inference_overrides: HashMap<String, KeyInferenceOverride>,

    /// Optional dynamic numeric scaling rules.
    /// If omitted, the server uses built-in defaults.
    #[serde(default)]
    pub numeric_rules: Option<Vec<NumericRule>>,
}

#[derive(Debug, Deserialize)]
pub struct EntityPayload {
    #[serde(rename = "type")]
    pub entity_type: String,
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct KeyInferenceOverride {
    #[serde(default)]
    pub id_patterns: Vec<String>,
    #[serde(default)]
    pub name_patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct NumericRule {
    pub key: String,
    pub min_scale: f64,
    pub max_scale: f64,
}

/// Fetch the obfuscation dictionary from the source URL.
pub async fn fetch_from_source(
    url: &str,
    token: &str,
) -> Result<ObfuscationSourceResponse, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!(
            "Obfuscation source returned HTTP {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        )
        .into());
    }

    let body = response.json::<ObfuscationSourceResponse>().await?;
    Ok(body)
}
