//! Live check of prompt caching: a chat turn shaped the way cnctd.world sends
//! one (a cached system prompt and tool list, a breakpoint on the previous
//! message, a moving tail through the tool loop), then a second turn that
//! must read what the first wrote. Reports cache reads and writes per call on
//! Anthropic (explicit breakpoints), OpenAI and Gemini (automatic). Spends a
//! few cents.
//!
//! ```bash
//! ANTHROPIC_API_KEY=... OPENAI_API_KEY=... GEMINI_API_KEY=... \
//!   cargo run --example live_cache_check
//! ```
//! Set `ONLY=anthropic|openai|gemini` to run one group.

use async_trait::async_trait;
use cnctd_ai::agent_loop::{LoopHandler, ToolExecResult, ToolExecutor};
use cnctd_ai::{
    create_tool, run_agent_loop, AnthropicConfig, CacheControl, Client, CompletionRequest, GeminiConfig, LoopConfig,
    Message, OpenAiConfig, PromptCache, RequestOptions, Usage,
};
use serde_json::json;

struct Weather;

#[async_trait]
impl ToolExecutor for Weather {
    async fn execute(&self, tool_use: &cnctd_ai::ToolUse) -> ToolExecResult {
        let city = tool_use.input["city"].as_str().unwrap_or("?").to_lowercase();
        ToolExecResult { output: format!("{{\"city\":\"{city}\",\"celsius\":{}}}", 10 + city.len()), success: true, duration_ms: 1 }
    }
}

struct Quiet;
#[async_trait]
impl LoopHandler for Quiet {}

fn tools() -> Vec<cnctd_ai::Tool> {
    vec![create_tool(
        "get_weather",
        "Current temperature for one city.",
        json!({ "type": "object", "properties": { "city": { "type": "string" } }, "required": ["city"] }),
    )
    .unwrap()]
}

/// A system prompt well past every provider's minimum cacheable length
/// (4,096 tokens on the strictest Anthropic models), unique per run so the
/// first call is a real write.
fn system_prompt(nonce: &str) -> String {
    let mut s = format!("Run {nonce}. You are a concise weather assistant. Use one tool call per step.\n\n");
    for i in 0..400 {
        s.push_str(&format!(
            "Guideline {i}: when a user asks about the weather in a city, report the temperature in Celsius, \
             keep the answer short, and never invent a reading you did not fetch.\n"
        ));
    }
    s
}

fn show(label: &str, u: &Usage) {
    println!(
        "  {label}: input {} (uncached {}), cache read {}, cache write {} (1h {}), output {}",
        u.prompt_tokens,
        u.effective_prompt_tokens(),
        u.cache_read_tokens.unwrap_or(0),
        u.cache_creation_tokens.unwrap_or(0),
        u.cache_creation_1h_tokens.unwrap_or(0),
        u.completion_tokens
    );
}

fn check(label: &str, ok: bool, failures: &mut Vec<String>) {
    if !ok {
        println!("  FAIL {label}");
        failures.push(label.to_string());
    }
}

/// Two chat turns through the agent loop (streamed), then the second turn
/// again through a plain non-streamed call.
async fn anthropic_case(model: &str, client: &Client, failures: &mut Vec<String>) {
    println!("== {model}");
    let options = RequestOptions {
        max_tokens: Some(4000),
        prompt_cache: Some(PromptCache {
            tools: Some(CacheControl::Extended),
            tail: Some(CacheControl::Ephemeral),
            key: None,
        }),
        ..Default::default()
    };
    let nonce = format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    let system = Message::system(system_prompt(&nonce)).with_extended_cache();
    let request = |messages: Vec<Message>| CompletionRequest {
        messages,
        tools: Some(tools()),
        built_in_tools: None,
        tool_config: None,
        options: Some(options.clone()),
    };

    let q1 = Message::user("Call get_weather for Paris, then for Tokyo, then say which is warmer in one sentence.");
    let turn1 = match run_agent_loop(client, request(vec![system.clone(), q1.clone()]), &Weather, &Quiet, LoopConfig::default()).await {
        Ok(r) => r,
        Err(e) => return check(&format!("{model} turn 1: {e}"), false, failures),
    };
    show(&format!("turn 1 ({} rounds)", turn1.iterations), &turn1.usage);
    check(&format!("{model} turn 1 wrote the cache"), turn1.usage.cache_creation_tokens.unwrap_or(0) > 4000, failures);
    check(&format!("{model} turn 1 wrote 1h entries"), turn1.usage.cache_creation_1h_tokens.unwrap_or(0) > 4000, failures);
    check(&format!("{model} turn 1 later rounds read the earlier ones"), turn1.usage.cache_read_tokens.unwrap_or(0) > 4000, failures);

    let mut history = vec![system.clone(), q1];
    history.extend(turn1.messages.clone());
    if let Some(last) = history.pop() {
        history.push(last.with_cache());
    }
    history.push(Message::user("And Berlin? Check it and compare with both."));
    let turn2 = match run_agent_loop(client, request(history.clone()), &Weather, &Quiet, LoopConfig::default()).await {
        Ok(r) => r,
        Err(e) => return check(&format!("{model} turn 2: {e}"), false, failures),
    };
    show(&format!("turn 2 ({} rounds)", turn2.iterations), &turn2.usage);
    let per_round_floor = 4000 * turn2.iterations.max(1);
    check(&format!("{model} turn 2 read every round"), turn2.usage.cache_read_tokens.unwrap_or(0) >= per_round_floor, failures);

    match client.complete(request(history)).await {
        Ok(r) => {
            show("turn 2 again, non-streamed", &r.usage);
            check(&format!("{model} non-streamed read"), r.usage.cache_read_tokens.unwrap_or(0) > 4000, failures);
        }
        Err(e) => check(&format!("{model} non-streamed: {e}"), false, failures),
    }
}

/// Automatic caching: the same long prefix twice (with a routing key on
/// OpenAI). A hit is likely, not guaranteed, so a miss is reported but only
/// fails when `expect_hit`.
async fn automatic_case(model: &str, client: &Client, key: Option<String>, expect_hit: bool, failures: &mut Vec<String>) {
    println!("== {model}");
    let nonce = format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    let options = RequestOptions {
        max_tokens: Some(2000),
        prompt_cache: key.map(|k| PromptCache { key: Some(k), ..Default::default() }),
        ..Default::default()
    };
    let request = |q: &str| CompletionRequest {
        messages: vec![Message::system(system_prompt(&nonce)), Message::user(q)],
        tools: Some(tools()),
        built_in_tools: None,
        tool_config: None,
        options: Some(options.clone()),
    };
    let mut last = Usage::zero();
    for (i, q) in ["Reply with the word ready.", "Reply with the word again."].iter().enumerate() {
        match client.complete(request(q)).await {
            Ok(r) => {
                show(&format!("call {}", i + 1), &r.usage);
                last = r.usage;
            }
            Err(e) => return check(&format!("{model} call {}: {e}", i + 1), false, failures),
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    let mut stream = match client.complete_stream(request("Reply with the word streamed.")).await {
        Ok(s) => s,
        Err(e) => return check(&format!("{model} streamed: {e}"), false, failures),
    };
    while let Some(chunk) = stream.next().await {
        if let Err(e) = chunk {
            return check(&format!("{model} streamed: {e}"), false, failures);
        }
    }
    if let Some(r) = stream.final_response() {
        show("call 3, streamed", &r.usage);
        if r.usage.cache_read_tokens.unwrap_or(0) > last.cache_read_tokens.unwrap_or(0) {
            last = r.usage;
        }
    }
    let hit = last.cache_read_tokens.unwrap_or(0) > 1000;
    if !hit {
        println!("  (no automatic cache hit this run)");
    }
    if expect_hit {
        check(&format!("{model} automatic cache hit"), hit, failures);
    }
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    let only = std::env::var("ONLY").ok();
    let run = |group: &str| only.as_deref().map(|o| o == group).unwrap_or(true);
    let mut failures = Vec::new();

    let anthropic = |model: &str| {
        Client::anthropic(
            AnthropicConfig { api_key: std::env::var("ANTHROPIC_API_KEY").unwrap(), model: model.into(), version: None },
            None,
        )
        .unwrap()
    };

    if run("anthropic") {
        for model in ["claude-sonnet-5-5", "claude-haiku-4-5"] {
            anthropic_case(model, &anthropic(model), &mut failures).await;
        }
    }
    if run("openai") {
        let client = Client::openai(
            OpenAiConfig {
                api_key: std::env::var("OPENAI_API_KEY").unwrap(),
                model: "gpt-5.4-mini".into(),
                organization: None,
                transcription_model: None,
            },
            None,
        )
        .unwrap();
        automatic_case("gpt-5.4-mini", &client, Some("live-cache-check".into()), true, &mut failures).await;
    }
    if run("gemini") {
        let client = Client::gemini(
            GeminiConfig { api_key: std::env::var("GEMINI_API_KEY").unwrap(), model: "gemini-3.6-flash".into() },
            None,
        )
        .unwrap();
        automatic_case("gemini-3.6-flash", &client, None, false, &mut failures).await;
    }

    if failures.is_empty() {
        println!("\nall cache checks passed");
    } else {
        println!("\n{} failed:\n  {}", failures.len(), failures.join("\n  "));
        std::process::exit(1);
    }
}
