use axum::extract::{Path, State};
use axum::Json;
use cnctd_ai::{Agent, CompletionRequest, Message, RequestOptions};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

use crate::error::AppError;
use crate::routes::chat::load_tools;
use crate::state::{AgentRunState, AppState};

#[derive(Debug, Deserialize)]
pub struct RunAgentRequest {
    pub model: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    pub task: String,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    #[serde(default = "default_max_duration")]
    pub max_duration_secs: u64,
}

fn default_max_iterations() -> usize {
    10
}
fn default_max_duration() -> u64 {
    120
}

pub async fn run_agent(
    State(state): State<AppState>,
    Json(req): Json<RunAgentRequest>,
) -> Result<Json<Value>, AppError> {
    let run_id = uuid::Uuid::new_v4().to_string();

    // Store initial state
    {
        let mut runs = state.agent_runs.write().await;
        runs.insert(
            run_id.clone(),
            AgentRunState {
                status: "running".to_string(),
                result: None,
                events: Vec::new(),
                total_prompt_tokens: 0,
                total_completion_tokens: 0,
                iterations: 0,
                duration_ms: None,
                error: None,
            },
        );
    }

    let run_id_clone = run_id.clone();
    let state_clone = state.clone();

    // Spawn the agent execution in background
    tokio::spawn(async move {
        let start = Instant::now();

        let result = execute_agent(&state_clone, &req).await;

        let elapsed = start.elapsed();
        let mut runs = state_clone.agent_runs.write().await;

        match result {
            Ok(trace) => {
                if let Some(run) = runs.get_mut(&run_id_clone) {
                    run.status = "completed".to_string();
                    run.result = trace.get("result").cloned();
                    run.total_prompt_tokens = trace
                        .get("total_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;
                    run.iterations = trace
                        .get("iterations")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    run.duration_ms = Some(elapsed.as_millis() as u64);
                    if let Some(events) = trace.get("events").and_then(|v| v.as_array()) {
                        run.events = events.clone();
                    }
                }
            }
            Err(e) => {
                if let Some(run) = runs.get_mut(&run_id_clone) {
                    run.status = "failed".to_string();
                    run.error = Some(e.to_string());
                    run.duration_ms = Some(elapsed.as_millis() as u64);
                }
            }
        }
    });

    Ok(Json(json!({
        "run_id": run_id,
        "status": "running",
    })))
}

async fn execute_agent(state: &AppState, req: &RunAgentRequest) -> Result<Value, AppError> {
    let client = crate::routes::chat::build_client(state, &req.model)?;
    let tools = load_tools(state, &req.tools).await;

    let gateway = state.gateway.as_ref();

    let mut builder = Agent::builder(&client)
        .max_iterations(req.max_iterations)
        .max_duration(Duration::from_secs(req.max_duration_secs));

    if let Some(sp) = &req.system_prompt {
        builder = builder.system_prompt(sp);
    }

    if let Some(gw) = gateway {
        builder = builder.gateway(gw);
    }

    let agent = builder.build();

    let completion_req = CompletionRequest {
        messages: vec![Message::user(&req.task)],
        tools: if tools.is_empty() { None } else { Some(tools) },
        built_in_tools: None,
        tool_config: None,
        options: Some(RequestOptions {
            max_tokens: Some(4096),
            ..Default::default()
        }),
    };

    let trace = agent.run(&req.task, completion_req).await?;

    // Serialize the trace
    let trace_json = json!({
        "result": trace.result,
        "stop_reason": format!("{:?}", trace.stop_reason),
        "duration_secs": trace.duration.as_secs_f64(),
        "total_tokens": trace.total_tokens,
        "iterations": trace.iterations,
        "errors": trace.errors,
        "successful_tool_calls": trace.successful_tool_calls,
        "events": trace.events.iter().map(|e| json!(format!("{:?}", e))).collect::<Vec<_>>(),
    });

    Ok(trace_json)
}

pub async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let runs = state.agent_runs.read().await;
    match runs.get(&run_id) {
        Some(run) => Ok(Json(json!({
            "run_id": run_id,
            "status": run.status,
            "result": run.result,
            "events": run.events,
            "total_prompt_tokens": run.total_prompt_tokens,
            "total_completion_tokens": run.total_completion_tokens,
            "iterations": run.iterations,
            "duration_ms": run.duration_ms,
            "error": run.error,
        }))),
        None => Err(AppError::BadRequest(format!("Run not found: {run_id}"))),
    }
}
