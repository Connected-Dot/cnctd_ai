//! Live check of the provider paths a model refresh depends on: tool loops
//! with thinking replay (Anthropic thinking blocks, Gemini thought
//! signatures, OpenAI reasoning items), a second turn replayed from the
//! first turn's messages, single calls with thinking/effort/sampling
//! settings, and transcription. Spends a few cents.
//!
//! ```bash
//! ANTHROPIC_API_KEY=... OPENAI_API_KEY=... GEMINI_API_KEY=... \
//!   cargo run --example live_model_check -- path/to/two-voices.wav
//! ```
//! Set `ONLY=anthropic|openai|gemini|transcribe` to run one group.

use async_trait::async_trait;
use cnctd_ai::agent_loop::{LoopHandler, ToolExecResult, ToolExecutor};
use cnctd_ai::transcription::TranscriptionRequest;
use cnctd_ai::{
    create_tool, AgentStopReason, AnthropicConfig, Client, CompletionRequest, Effort, FinishReason, GeminiConfig,
    LoopConfig, MediaResolution, Message, OpenAiConfig, RequestOptions, Thinking, ThinkingLevel, run_agent_loop,
};
use serde_json::json;

struct Weather;

#[async_trait]
impl ToolExecutor for Weather {
    async fn execute(&self, tool_use: &cnctd_ai::ToolUse) -> ToolExecResult {
        let city = tool_use.input["city"].as_str().unwrap_or("?").to_lowercase();
        let temp = match city.as_str() {
            c if c.contains("paris") => 18,
            c if c.contains("tokyo") => 24,
            c if c.contains("berlin") => 12,
            _ => 20,
        };
        ToolExecResult { output: format!("{{\"city\":\"{}\",\"celsius\":{}}}", city, temp), success: true, duration_ms: 1 }
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

fn opts(f: impl FnOnce(&mut RequestOptions)) -> RequestOptions {
    let mut o = RequestOptions { max_tokens: Some(16000), ..Default::default() };
    f(&mut o);
    o
}

/// Count replayable reasoning carried by a turn's assistant messages.
fn replay_summary(messages: &[Message]) -> String {
    let (mut thinking, mut signatures, mut reasoning) = (0, 0, 0);
    for m in messages {
        if let Some(pc) = &m.provider_content {
            for b in &pc.blocks {
                if matches!(b["type"].as_str(), Some("thinking") | Some("redacted_thinking"))
                    && (b.get("signature").is_some() || b.get("data").is_some())
                {
                    thinking += 1;
                }
                if b.get("thoughtSignature").is_some() {
                    signatures += 1;
                }
            }
        }
        reasoning += m.reasoning_items.as_ref().map(|r| r.len()).unwrap_or(0);
    }
    format!("thinking={} sigs={} reasoning={}", thinking, signatures, reasoning)
}

async fn loop_case(label: &str, client: &Client, options: RequestOptions, failures: &mut Vec<String>) {
    let first_user = Message::user(
        "Call get_weather for Paris. After you see that result, call get_weather for Tokyo. \
         Then answer in one short sentence: which city is warmer?",
    );
    let request = CompletionRequest {
        messages: vec![Message::system("You are a concise weather assistant. Use one tool call per step."), first_user],
        tools: Some(tools()),
        built_in_tools: None,
        tool_config: None,
        options: Some(options.clone()),
    };
    let first = match run_agent_loop(client, request.clone(), &Weather, &Quiet, LoopConfig::default()).await {
        Ok(r) => r,
        Err(e) => {
            failures.push(format!("{label}: loop error {e}"));
            println!("FAIL {label}: {e}");
            return;
        }
    };
    let ok_first = matches!(first.stop_reason, AgentStopReason::ModelFinished(FinishReason::Stop)) && first.iterations >= 2;
    let answer = first.final_response.as_ref().map(|r| r.text().to_string()).unwrap_or_default();

    // Turn 2: replay turn 1 exactly as stored, then ask for another tool call.
    let mut messages = request.messages.clone();
    messages.extend(first.messages.clone());
    messages.push(Message::user("Now check Berlin the same way. Is Berlin warmer than Tokyo? One sentence."));
    let second = run_agent_loop(
        client,
        CompletionRequest { messages, ..request },
        &Weather,
        &Quiet,
        LoopConfig::default(),
    )
    .await;
    let (ok_second, second_note) = match &second {
        Ok(r) => (
            matches!(r.stop_reason, AgentStopReason::ModelFinished(FinishReason::Stop)) && r.iterations >= 2,
            format!("{:?}/{} rounds", r.stop_reason, r.iterations),
        ),
        Err(e) => (false, format!("error {e}")),
    };

    let status = if ok_first && ok_second { "ok  " } else { "FAIL" };
    println!(
        "{status} {label}: turn1 {:?}/{} rounds [{}] in={} out={} | turn2 {} | \"{}\"",
        first.stop_reason,
        first.iterations,
        replay_summary(&first.messages),
        first.usage.prompt_tokens,
        first.usage.completion_tokens,
        second_note,
        answer.replace('\n', " ").chars().take(80).collect::<String>()
    );
    if !(ok_first && ok_second) {
        failures.push(label.to_string());
    }
}

async fn complete_case(label: &str, client: &Client, options: RequestOptions, failures: &mut Vec<String>) {
    let request = CompletionRequest {
        messages: vec![Message::system("Answer with one word."), Message::user("What color is a clear daytime sky?")],
        tools: None,
        built_in_tools: None,
        tool_config: None,
        options: Some(options),
    };
    match client.complete(request).await {
        Ok(r) if r.finish_reason == FinishReason::Stop && !r.text().trim().is_empty() => {
            println!("ok   {label}: \"{}\" out={}", r.text().trim(), r.usage.completion_tokens)
        }
        Ok(r) => {
            println!("FAIL {label}: finish={:?} text=\"{}\"", r.finish_reason, r.text());
            failures.push(label.to_string());
        }
        Err(e) => {
            println!("FAIL {label}: {e}");
            failures.push(label.to_string());
        }
    }
}

async fn transcribe_case(label: &str, client: &Client, request: TranscriptionRequest, failures: &mut Vec<String>) {
    match client.transcribe(request).await {
        Ok(r) if !r.text.is_empty() => {
            let speakers = r.speakers.as_ref().map(|s| s.len()).unwrap_or(0);
            let segments = r.segments.as_ref().map(|s| s.len()).unwrap_or(0);
            println!("ok   {label}: speakers={speakers} segments={segments} lang={:?} \"{}\"", r.language, r.text);
        }
        Ok(_) => {
            println!("FAIL {label}: empty transcript");
            failures.push(label.to_string());
        }
        Err(e) => {
            println!("FAIL {label}: {e}");
            failures.push(label.to_string());
        }
    }
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    let only = std::env::var("ONLY").ok();
    let run = |group: &str| only.as_deref().map(|o| o == group).unwrap_or(true);
    let audio = std::env::args().nth(1);
    let mut failures = Vec::new();

    let anthropic = |model: &str| {
        Client::anthropic(
            AnthropicConfig { api_key: std::env::var("ANTHROPIC_API_KEY").unwrap(), model: model.into(), version: None },
            None,
        )
        .unwrap()
    };
    let openai = |model: &str| {
        Client::openai(
            OpenAiConfig {
                api_key: std::env::var("OPENAI_API_KEY").unwrap(),
                model: model.into(),
                organization: None,
                transcription_model: None,
            },
            None,
        )
        .unwrap()
    };
    let gemini = |model: &str| {
        Client::gemini(GeminiConfig { api_key: std::env::var("GEMINI_API_KEY").unwrap(), model: model.into() }, None).unwrap()
    };

    if run("anthropic") {
        println!("== Anthropic");
        loop_case("claude-opus-5-5 default thinking, effort low", &anthropic("claude-opus-5-5"), opts(|o| o.effort = Some(Effort::Low)), &mut failures).await;
        loop_case("claude-opus-5-5 effort max (forces thinking blocks)", &anthropic("claude-opus-5-5"), opts(|o| o.effort = Some(Effort::Max)), &mut failures).await;
        loop_case("claude-sonnet-5-5 adaptive, effort low", &anthropic("claude-sonnet-5-5"), opts(|o| { o.thinking = Some(Thinking::Adaptive); o.effort = Some(Effort::Low); }), &mut failures).await;
        loop_case("claude-fable-5-1 effort low", &anthropic("claude-fable-5-1"), opts(|o| o.effort = Some(Effort::Low)), &mut failures).await;
        loop_case("claude-opus-4-8 adaptive, effort medium", &anthropic("claude-opus-4-8"), opts(|o| { o.thinking = Some(Thinking::Adaptive); o.effort = Some(Effort::Medium); }), &mut failures).await;
        loop_case("claude-sonnet-4-6 adaptive", &anthropic("claude-sonnet-4-6"), opts(|o| o.thinking = Some(Thinking::Adaptive)), &mut failures).await;
        loop_case("claude-haiku-4-5 no thinking", &anthropic("claude-haiku-4-5-20251001"), opts(|_| {}), &mut failures).await;
        complete_case("claude-sonnet-4-6 thinking off + temperature 0.1", &anthropic("claude-sonnet-4-6"), opts(|o| { o.thinking = Some(Thinking::Disabled); o.temperature = Some(0.1); o.max_tokens = Some(1024); }), &mut failures).await;
        complete_case("claude-sonnet-5-5 between_tools, effort low", &anthropic("claude-sonnet-5-5"), opts(|o| { o.thinking = Some(Thinking::BetweenTools); o.effort = Some(Effort::Low); }), &mut failures).await;
        complete_case("claude-opus-5-5 effort low (non-streaming)", &anthropic("claude-opus-5-5"), opts(|o| o.effort = Some(Effort::Low)), &mut failures).await;
    }

    if run("openai") {
        println!("== OpenAI");
        loop_case("gpt-6.1-sol effort low", &openai("gpt-6.1-sol"), opts(|o| o.effort = Some(Effort::Low)), &mut failures).await;
        loop_case("gpt-5.6-terra effort low", &openai("gpt-5.6-terra"), opts(|o| o.effort = Some(Effort::Low)), &mut failures).await;
        loop_case("gpt-5.5 default", &openai("gpt-5.5"), opts(|_| {}), &mut failures).await;
        loop_case("gpt-4o-mini (non-reasoning)", &openai("gpt-4o-mini"), opts(|_| {}), &mut failures).await;
        complete_case("gpt-5.4 effort none + temperature 0.1", &openai("gpt-5.4"), opts(|o| { o.thinking = Some(Thinking::Disabled); o.temperature = Some(0.1); }), &mut failures).await;
        complete_case("gpt-6-luna effort max", &openai("gpt-6-luna"), opts(|o| o.effort = Some(Effort::Max)), &mut failures).await;
    }

    if run("gemini") {
        println!("== Gemini");
        loop_case("gemini-3.8-flash effort low", &gemini("gemini-3.8-flash"), opts(|o| o.effort = Some(Effort::Low)), &mut failures).await;
        loop_case("gemini-3.5-flash-lite default", &gemini("gemini-3.5-flash-lite"), opts(|_| {}), &mut failures).await;
        loop_case("gemini-3.1-pro-preview default", &gemini("gemini-3.1-pro-preview"), opts(|_| {}), &mut failures).await;
        loop_case("gemini-2.5-flash thinking off", &gemini("gemini-2.5-flash"), opts(|o| o.thinking = Some(Thinking::Disabled)), &mut failures).await;
        complete_case("gemini-3.6-flash minimal + media resolution low", &gemini("gemini-3.6-flash"), opts(|o| { o.thinking_level = Some(ThinkingLevel::Minimal); o.media_resolution = Some(MediaResolution::Low); }), &mut failures).await;
        complete_case("gemini-3.8-flash effort max -> high", &gemini("gemini-3.8-flash"), opts(|o| o.effort = Some(Effort::Max)), &mut failures).await;
    }

    if run("transcribe") {
        if let Some(path) = &audio {
            println!("== Transcription ({path})");
            transcribe_case("whisper-1", &openai("whisper-1"), TranscriptionRequest::new(path.as_str()).with_model("whisper-1").with_language("en"), &mut failures).await;
            transcribe_case("gpt-transcribe", &openai("gpt-transcribe"), TranscriptionRequest::new(path.as_str()).with_model("gpt-transcribe").with_language("en"), &mut failures).await;
            transcribe_case(
                "gemini-3.5-transcribe diarization+timestamps",
                &gemini("gemini-3.5-transcribe"),
                TranscriptionRequest::new(path.as_str()).with_model("gemini-3.5-transcribe").with_language("en").with_diarization().with_timestamps(),
                &mut failures,
            )
            .await;
            transcribe_case(
                "gemini-3.6-flash diarization (prompt path)",
                &gemini("gemini-3.6-flash"),
                TranscriptionRequest::new(path.as_str()).with_model("gemini-3.6-flash").with_diarization(),
                &mut failures,
            )
            .await;
        } else {
            println!("== Transcription skipped (pass an audio path)");
        }
    }

    if failures.is_empty() {
        println!("\nall passed");
    } else {
        println!("\n{} failed: {:?}", failures.len(), failures);
        std::process::exit(1);
    }
}
