use rand::Rng;
use std::collections::HashMap;

use super::source::NumericRule;

/// Per-metric-type scaling factors for numeric obfuscation.
/// Within a session, the same factor is applied to all values of a given metric,
/// preserving relative ordering and trends while hiding absolute values.
pub struct NumericScaler {
    factors: HashMap<String, f64>,
}

/// Built-in defaults used when the source URL doesn't provide numeric_rules.
const DEFAULT_METRIC_CONFIGS: &[(&str, f64, f64)] = &[
    ("revenue", 0.2, 0.8),
    ("gross_revenue", 0.2, 0.8),
    ("impressions", 1.5, 3.0),
    ("requests", 1.5, 3.0),
    ("bid_requests", 1.5, 3.0),
    ("bid_responses", 1.5, 3.0),
    ("ad_filled", 1.5, 3.0),
    ("ad_impression", 1.5, 3.0),
    ("complete", 1.5, 3.0),
    ("unique_users", 1.5, 3.0),
    ("cpm", 0.3, 0.9),
    ("avg_winning_bid", 0.3, 0.9),
    ("fillrate", 1.0, 1.0),
    ("fill_rate", 1.0, 1.0),
    ("completion_rate", 1.0, 1.0),
    ("render_rate", 1.0, 1.0),
    ("win_rate", 1.0, 1.0),
];

impl NumericScaler {
    /// Generate random scale factors from built-in defaults.
    pub fn new_random() -> Self {
        Self::build_factors(
            DEFAULT_METRIC_CONFIGS
                .iter()
                .map(|&(k, min, max)| (k, min, max)),
        )
    }

    /// Generate random scale factors from dynamic rules provided by the source URL.
    /// Falls back to built-in defaults if `rules` is empty.
    pub fn new_from_rules(rules: &[NumericRule]) -> Self {
        if rules.is_empty() {
            return Self::new_random();
        }
        Self::build_factors(
            rules
                .iter()
                .map(|r| (r.key.as_str(), r.min_scale, r.max_scale)),
        )
    }

    fn build_factors<'a>(configs: impl Iterator<Item = (&'a str, f64, f64)>) -> Self {
        let mut rng = rand::thread_rng();
        let mut factors = HashMap::new();

        for (metric, min, max) in configs {
            let factor = if (max - min).abs() < f64::EPSILON {
                min
            } else {
                rng.gen_range(min..=max)
            };
            factors.insert(metric.to_string(), factor);
        }

        Self { factors }
    }

    /// Scale a value for sending to the LLM. Unknown metrics pass through at 1.0.
    pub fn scale(&self, metric: &str, value: f64) -> f64 {
        let factor = self.factors.get(metric).copied().unwrap_or(1.0);
        value * factor
    }

    /// Reverse-scale a value from the LLM back to real. Unknown metrics pass through.
    pub fn unscale(&self, metric: &str, value: f64) -> f64 {
        let factor = self.factors.get(metric).copied().unwrap_or(1.0);
        if factor.abs() < f64::EPSILON {
            value
        } else {
            value / factor
        }
    }

    /// Check if a key name looks like a known metric.
    pub fn is_known_metric(&self, key: &str) -> bool {
        self.factors.contains_key(key)
    }
}
