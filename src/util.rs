use serde::de::DeserializeOwned;

pub fn parse_json<T: DeserializeOwned>(s: &str) -> Result<T, String> {
    if let Ok(v) = serde_json::from_str::<T>(s) {
        return Ok(v);
    }
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        if let Ok(v) = serde_json::from_str::<T>(&s[start..=end]) {
            return Ok(v);
        }
    }
    let cleaned = s
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str::<T>(cleaned).map_err(|e| e.to_string())
}