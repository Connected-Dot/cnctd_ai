use rand::Rng;
use std::collections::HashMap;

/// Per-metric-type scaling factors for numeric obfuscation.
/// Within a session, the same factor is applied to all values of a given metric,
/// preserving relative ordering and trends while hiding absolute values.
pub struct NumericScaler {
    factors: HashMap<String, f64>,
}

/// (metric_name, min_factor, max_factor)
const METRIC_CONFIGS: &[(&str, f64, f64)] = &[
    // Revenue/money: scale down
    ("revenue", 0.2, 0.8),
    ("gross_revenue", 0.2, 0.8),
    // Counts: scale up
    ("impressions", 1.5, 3.0),
    ("requests", 1.5, 3.0),
    ("bid_requests", 1.5, 3.0),
    ("bid_responses", 1.5, 3.0),
    ("ad_filled", 1.5, 3.0),
    ("ad_impression", 1.5, 3.0),
    ("complete", 1.5, 3.0),
    ("unique_users", 1.5, 3.0),
    // Per-unit prices: scale down
    ("cpm", 0.3, 0.9),
    ("avg_winning_bid", 0.3, 0.9),
    // Rates/percentages: pass through (1.0)
    ("fillrate", 1.0, 1.0),
    ("fill_rate", 1.0, 1.0),
    ("completion_rate", 1.0, 1.0),
    ("render_rate", 1.0, 1.0),
    ("win_rate", 1.0, 1.0),
];

impl NumericScaler {
    /// Generate random scale factors for all known metrics.
    pub fn new_random() -> Self {
        let mut rng = rand::thread_rng();
        let mut factors = HashMap::new();

        for &(metric, min, max) in METRIC_CONFIGS {
            let factor = if (max - min).abs() < f64::EPSILON {
                min // Fixed factor (e.g. 1.0 for percentages)
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
