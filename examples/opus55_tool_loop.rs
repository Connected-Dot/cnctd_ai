//! Reproduction (2026-10-12): Dotty on claude-opus-5-5 runs her tool calls and the
//! loop's second round never finishes. One fake tool, the real agent loop, every
//! chunk timestamped, a hard cap so a hang shows as a timeout.
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use cnctd_ai::{
    AnthropicConfig, CacheControl, Client, CompletionRequest, LoopConfig, LoopHandler, Message, PromptCache,
    RequestOptions, StreamChunk, Thinking, Tool, ToolExecResult, ToolExecutor, ToolUse, run_agent_loop,
};
use serde_json::{Map, Value, json};

struct Printer(Instant);
impl Printer {
    fn t(&self) -> String { format!("{:>7.2}s", self.0.elapsed().as_secs_f64()) }
}

#[async_trait]
impl LoopHandler for Printer {
    async fn on_chunk(&self, chunk: &StreamChunk) {
        if let Some(d) = &chunk.delta { println!("{} delta: {:?}", self.t(), d); }
        if let Some(e) = &chunk.tool_use_event { println!("{} tool event: {:?}", self.t(), e); }
        if let Some(f) = &chunk.finish_reason { println!("{} finish: {:?}", self.t(), f); }
    }
    async fn on_tool_call_start(&self, tu: &ToolUse) { println!("{} tool start: {} {}", self.t(), tu.name, tu.input); }
    async fn on_tool_call_result(&self, tu: &ToolUse, r: &ToolExecResult) { println!("{} tool result: {} ok={} {}", self.t(), tu.name, r.success, r.output); }
    async fn on_turn_complete(&self, i: u32) { println!("{} turn {} complete", self.t(), i); }
    async fn on_error(&self, m: &str) { println!("{} ERROR: {}", self.t(), m); }
}

#[async_trait]
impl ToolExecutor for Printer {
    async fn execute(&self, tu: &ToolUse) -> ToolExecResult {
        let q = tu.input.get("query").and_then(Value::as_str).unwrap_or("").to_lowercase();
        let answer = if q.contains("france") { "Paris" } else if q.contains("spain") { "Madrid" } else if q.contains("italy") { "Rome" }
            else if q.contains("paris") { "2,102,650 (2023)" } else if q.contains("madrid") { "3,332,035 (2023)" } else if q.contains("rome") { "2,748,109 (2023)" }
            else { "no entry" };
        ToolExecResult { output: format!("lookup({q}) -> {answer}"), success: true, duration_ms: 3 }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let model = std::env::var("MODEL").unwrap_or_else(|_| "claude-opus-5-5".into());
    let client = Client::anthropic(
        AnthropicConfig { api_key: std::env::var("ANTHROPIC_API_KEY")?, model: model.clone(), version: None },
        None,
    )?;
    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), json!({ "query": { "type": "string" } }));
    schema.insert("required".into(), json!(["query"]));
    let tool = Tool::new("lookup".to_string(), "Look a fact up. Returns the answer as text.".to_string(), Arc::new(schema));
    let request = CompletionRequest {
        messages: vec![
            Message::system("You are a terse assistant. Use the lookup tool for every fact before answering; one lookup per call, and decide what to look up next from each answer.").with_extended_cache(),
            Message::user(std::env::var("PROMPT").unwrap_or_else(|_| "Rank the capitals of France, Spain and Italy by population. Look up each capital, then each population, one lookup at a time, reasoning between calls. Finish with one ranked line.".into())),
        ],
        tools: Some(vec![tool]),
        built_in_tools: None,
        tool_config: None,
        options: Some(RequestOptions {
            max_tokens: Some(32_000),
            thinking: if model.contains("opus-4-5") { None } else { Some(Thinking::Adaptive) },
            prompt_cache: Some(PromptCache { tools: Some(CacheControl::Extended), tail: Some(CacheControl::Ephemeral), key: Some("repro".into()) }),
            ..Default::default()
        }),
    };
    let printer = Printer(Instant::now());
    println!("model: {model}");
    let out = tokio::time::timeout(
        Duration::from_secs(120),
        run_agent_loop(&client, request, &printer, &printer, LoopConfig::default()),
    )
    .await;
    match out {
        Err(_) => println!("{} TIMED OUT: the loop never returned", printer.t()),
        Ok(Err(e)) => println!("{} loop Err: {e}", printer.t()),
        Ok(Ok(r)) => {
            println!("{} loop done: stop={:?} iterations={} text={:?}", printer.t(), r.stop_reason, r.iterations, r.accumulated_text);
            for (i, m) in r.messages.iter().enumerate() {
                let blocks: Vec<String> = m.provider_content.as_ref().map(|pc| pc.blocks.iter().map(|b| {
                    let ty = b.get("type").and_then(Value::as_str).unwrap_or("?");
                    match ty {
                        "thinking" => format!("thinking(len={}, sig={})", b.get("thinking").and_then(Value::as_str).map(str::len).unwrap_or(0), b.get("signature").and_then(Value::as_str).map(|s| !s.is_empty()).unwrap_or(false)),
                        "text" => format!("text(len={})", b.get("text").and_then(Value::as_str).map(str::len).unwrap_or(0)),
                        other => other.to_string(),
                    }
                }).collect()).unwrap_or_default();
                println!("  message {i}: role={:?} has_tool_results={} blocks={:?}", m.role, m.has_tool_results(), blocks);
            }
        }
    }
    Ok(())
}
