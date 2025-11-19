use anyhow::Result;
use cnctd_ai::{
    Client, AnthropicConfig, OpenAiConfig, Message, CompletionRequest, 
    RequestOptions, Error
};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    
    println!("=== Error Handling Examples ===\n");
    
    println!("=== Testing Anthropic ===");
    test_anthropic_errors().await;
    
    println!("\n=== Testing OpenAI ===");
    test_openai_errors().await;
    
    println!("\n=== Testing Error Recovery Patterns ===");
    test_error_recovery().await?;
    
    Ok(())
}

async fn test_anthropic_errors() {
    println!("\n1. Testing invalid API key:");
    test_invalid_api_key_anthropic().await;
    
    println!("\n2. Testing invalid model:");
    test_invalid_model_anthropic().await;
    
    println!("\n3. Testing empty messages:");
    test_empty_messages_anthropic().await;
    
    println!("\n4. Testing invalid max_tokens:");
    test_invalid_max_tokens_anthropic().await;
    
    println!("\n5. Testing malformed tool schema:");
    test_malformed_tool_anthropic().await;
}

async fn test_openai_errors() {
    println!("\n1. Testing invalid API key:");
    test_invalid_api_key_openai().await;
    
    println!("\n2. Testing invalid model:");
    test_invalid_model_openai().await;
    
    println!("\n3. Testing empty messages:");
    test_empty_messages_openai().await;
    
    println!("\n4. Testing invalid max_tokens:");
    test_invalid_max_tokens_openai().await;
    
    println!("\n5. Testing malformed tool schema:");
    test_malformed_tool_openai().await;
}

async fn test_invalid_api_key_anthropic() {
    let client = Client::anthropic(
        AnthropicConfig {
            api_key: "invalid_key_12345".into(),
            model: "claude-sonnet-4-20250514".into(),
            version: None,
        },
        None,
    );
    
    match client {
        Ok(client) => {
            let request = CompletionRequest {
                messages: vec![Message::user("Hello")],
                tools: None,
                options: None,
            };
            
            match client.complete(request).await {
                Ok(_) => println!("  Unexpected success!"),
                Err(e) => {
                    println!("  Expected error: {}", e);
                    match e {
                        Error::AuthenticationFailed(_) => println!("  Correctly identified as authentication failure"),
                        Error::ProviderError { provider, message, status_code } => {
                            println!("  Provider: {}", provider);
                            println!("  Message: {}", message);
                            println!("  Status: {:?}", status_code);
                        }
                        _ => println!("  Error type: {:?}", e),
                    }
                }
            }
        }
        Err(e) => println!("  Client creation failed: {}", e),
    }
}

async fn test_invalid_api_key_openai() {
    let client = Client::openai(
        OpenAiConfig {
            api_key: "invalid_key_12345".into(),
            model: "gpt-4o".into(),
            organization: None,
        },
        None,
    );
    
    match client {
        Ok(client) => {
            let request = CompletionRequest {
                messages: vec![Message::user("Hello")],
                tools: None,
                options: None,
            };
            
            match client.complete(request).await {
                Ok(_) => println!("  Unexpected success!"),
                Err(e) => {
                    println!("  Expected error: {}", e);
                    match e {
                        Error::AuthenticationFailed(_) => println!("  Correctly identified as authentication failure"),
                        Error::OpenAiError(oe) => {
                            println!("  OpenAI error: {}", oe);
                        }
                        _ => println!("  Error type: {:?}", e),
                    }
                }
            }
        }
        Err(e) => println!("  Client creation failed: {}", e),
    }
}

async fn test_invalid_model_anthropic() {
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("  Skipped: ANTHROPIC_API_KEY not set");
            return;
        }
    };
    
    let client = Client::anthropic(
        AnthropicConfig {
            api_key,
            model: "invalid-model-name".into(),
            version: None,
        },
        None,
    );
    
    match client {
        Ok(client) => {
            let request = CompletionRequest {
                messages: vec![Message::user("Hello")],
                tools: None,
                options: None,
            };
            
            match client.complete(request).await {
                Ok(_) => println!("  Unexpected success!"),
                Err(e) => {
                    println!("  Expected error: {}", e);
                    match e {
                        Error::ProviderError { provider, message, status_code } => {
                            println!("  Provider: {}", provider);
                            println!("  Message: {}", message);
                            println!("  Status: {:?}", status_code);
                        }
                        _ => println!("  Error type: {:?}", e),
                    }
                }
            }
        }
        Err(e) => println!("  Client creation failed: {}", e),
    }
}

async fn test_invalid_model_openai() {
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("  Skipped: OPENAI_API_KEY not set");
            return;
        }
    };
    
    let client = Client::openai(
        OpenAiConfig {
            api_key,
            model: "invalid-model-name".into(),
            organization: None,
        },
        None,
    );
    
    match client {
        Ok(client) => {
            let request = CompletionRequest {
                messages: vec![Message::user("Hello")],
                tools: None,
                options: None,
            };
            
            match client.complete(request).await {
                Ok(_) => println!("  Unexpected success!"),
                Err(e) => {
                    println!("  Expected error: {}", e);
                    match e {
                        Error::OpenAiError(oe) => {
                            println!("  OpenAI error: {}", oe);
                        }
                        _ => println!("  Error type: {:?}", e),
                    }
                }
            }
        }
        Err(e) => println!("  Client creation failed: {}", e),
    }
}

async fn test_empty_messages_anthropic() {
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("  Skipped: ANTHROPIC_API_KEY not set");
            return;
        }
    };
    
    let client = Client::anthropic(
        AnthropicConfig {
            api_key,
            model: "claude-sonnet-4-20250514".into(),
            version: None,
        },
        None,
    );
    
    match client {
        Ok(client) => {
            let request = CompletionRequest {
                messages: vec![],
                tools: None,
                options: None,
            };
            
            match client.complete(request).await {
                Ok(_) => println!("  Unexpected success!"),
                Err(e) => {
                    println!("  Expected error: {}", e);
                    match e {
                        Error::InvalidRequest(msg) => println!("  Invalid request: {}", msg),
                        Error::ProviderError { provider, message, status_code } => {
                            println!("  Provider: {}", provider);
                            println!("  Message: {}", message);
                            println!("  Status: {:?}", status_code);
                        }
                        _ => println!("  Error type: {:?}", e),
                    }
                }
            }
        }
        Err(e) => println!("  Client creation failed: {}", e),
    }
}

async fn test_empty_messages_openai() {
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("  Skipped: OPENAI_API_KEY not set");
            return;
        }
    };
    
    let client = Client::openai(
        OpenAiConfig {
            api_key,
            model: "gpt-4o".into(),
            organization: None,
        },
        None,
    );
    
    match client {
        Ok(client) => {
            let request = CompletionRequest {
                messages: vec![],
                tools: None,
                options: None,
            };
            
            match client.complete(request).await {
                Ok(_) => println!("  Unexpected success!"),
                Err(e) => {
                    println!("  Expected error: {}", e);
                    match e {
                        Error::InvalidRequest(msg) => println!("  Invalid request: {}", msg),
                        Error::OpenAiError(oe) => {
                            println!("  OpenAI error: {}", oe);
                        }
                        _ => println!("  Error type: {:?}", e),
                    }
                }
            }
        }
        Err(e) => println!("  Client creation failed: {}", e),
    }
}

async fn test_invalid_max_tokens_anthropic() {
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("  Skipped: ANTHROPIC_API_KEY not set");
            return;
        }
    };
    
    let client = Client::anthropic(
        AnthropicConfig {
            api_key,
            model: "claude-sonnet-4-20250514".into(),
            version: None,
        },
        None,
    );
    
    match client {
        Ok(client) => {
            let request = CompletionRequest {
                messages: vec![Message::user("Hello")],
                tools: None,
                options: Some(RequestOptions {
                    max_tokens: Some(0),
                    temperature: None,
                    top_p: None,
                    stop_sequences: None,
                }),
            };
            
            match client.complete(request).await {
                Ok(_) => println!("  Unexpected success!"),
                Err(e) => {
                    println!("  Expected error: {}", e);
                    match e {
                        Error::ProviderError { provider, message, status_code } => {
                            println!("  Provider: {}", provider);
                            println!("  Message: {}", message);
                            println!("  Status: {:?}", status_code);
                        }
                        _ => println!("  Error type: {:?}", e),
                    }
                }
            }
        }
        Err(e) => println!("  Client creation failed: {}", e),
    }
}

async fn test_invalid_max_tokens_openai() {
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("  Skipped: OPENAI_API_KEY not set");
            return;
        }
    };
    
    let client = Client::openai(
        OpenAiConfig {
            api_key,
            model: "gpt-4o".into(),
            organization: None,
        },
        None,
    );
    
    match client {
        Ok(client) => {
            let request = CompletionRequest {
                messages: vec![Message::user("Hello")],
                tools: None,
                options: Some(RequestOptions {
                    max_tokens: Some(0),
                    temperature: None,
                    top_p: None,
                    stop_sequences: None,
                }),
            };
            
            match client.complete(request).await {
                Ok(_) => println!("  Unexpected success!"),
                Err(e) => {
                    println!("  Expected error: {}", e);
                    match e {
                        Error::OpenAiError(oe) => {
                            println!("  OpenAI error: {}", oe);
                        }
                        _ => println!("  Error type: {:?}", e),
                    }
                }
            }
        }
        Err(e) => println!("  Client creation failed: {}", e),
    }
}

async fn test_malformed_tool_anthropic() {
    use cnctd_ai::Tool;
    use serde_json::json;
    
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("  Skipped: ANTHROPIC_API_KEY not set");
            return;
        }
    };
    
    let client = Client::anthropic(
        AnthropicConfig {
            api_key,
            model: "claude-sonnet-4-20250514".into(),
            version: None,
        },
        None,
    );
    
    match client {
        Ok(client) => {
            println!("  Test 5a: Invalid type value (caught by client validation)");
            let tool = Tool {
                name: "test_tool".into(),
                description: "A test tool".into(),
                input_schema: json!({
                    "type": "invalid_type",
                }),
            };
            
            let mut request = CompletionRequest {
                messages: vec![Message::user("Use the test tool")],
                tools: None,
                options: None,
            };
            request.add_tool(tool);
            
            match client.complete(request).await {
                Ok(_) => println!("    NOTE: Anthropic accepted invalid schema - more lenient validation"),
                Err(e) => {
                    println!("    Expected error: {}", e);
                    match e {
                        Error::InvalidRequest(msg) => println!("    Client-side validation caught it: {}", msg),
                        Error::ProviderError { provider, message, status_code } => {
                            println!("    Provider: {}", provider);
                            println!("    Message: {}", message);
                            println!("    Status: {:?}", status_code);
                        }
                        _ => println!("    Error type: {:?}", e),
                    }
                }
            }
            
            println!("\n  Test 5b: Missing type field (caught by client validation)");
            let tool2 = Tool {
                name: "test_tool_2".into(),
                description: "Another test tool".into(),
                input_schema: json!({
                    "properties": {
                        "name": {"type": "string"}
                    }
                }),
            };
            
            let mut request2 = CompletionRequest {
                messages: vec![Message::user("Use the other test tool")],
                tools: None,
                options: None,
            };
            request2.add_tool(tool2);
            
            match client.complete(request2).await {
                Ok(_) => println!("    Unexpected success - validation should have caught this!"),
                Err(e) => {
                    println!("    Expected error: {}", e);
                    match e {
                        Error::InvalidRequest(msg) => println!("    Client-side validation: {}", msg),
                        _ => println!("    Error type: {:?}", e),
                    }
                }
            }
        }
        Err(e) => println!("  Client creation failed: {}", e),
    }
}

async fn test_malformed_tool_openai() {
    use cnctd_ai::Tool;
    use serde_json::json;
    
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("  Skipped: OPENAI_API_KEY not set");
            return;
        }
    };
    
    let client = Client::openai(
        OpenAiConfig {
            api_key,
            model: "gpt-4o".into(),
            organization: None,
        },
        None,
    );
    
    match client {
        Ok(client) => {
            println!("  Test 5a: Invalid type value (caught by client validation)");
            let tool = Tool {
                name: "test_tool".into(),
                description: "A test tool".into(),
                input_schema: json!({
                    "type": "invalid_type",
                }),
            };
            
            let mut request = CompletionRequest {
                messages: vec![Message::user("Use the test tool")],
                tools: None,
                options: None,
            };
            request.add_tool(tool);
            
            match client.complete(request).await {
                Ok(_) => println!("    Unexpected success!"),
                Err(e) => {
                    println!("    Expected error: {}", e);
                    match e {
                        Error::InvalidRequest(msg) => println!("    Client-side validation: {}", msg),
                        Error::OpenAiError(oe) => {
                            println!("    OpenAI error: {}", oe);
                        }
                        _ => println!("    Error type: {:?}", e),
                    }
                }
            }
            
            println!("\n  Test 5b: Missing type field (caught by client validation)");
            let tool2 = Tool {
                name: "test_tool_2".into(),
                description: "Another test tool".into(),
                input_schema: json!({
                    "properties": {
                        "name": {"type": "string"}
                    }
                }),
            };
            
            let mut request2 = CompletionRequest {
                messages: vec![Message::user("Use the other test tool")],
                tools: None,
                options: None,
            };
            request2.add_tool(tool2);
            
            match client.complete(request2).await {
                Ok(_) => println!("    Unexpected success - validation should have caught this!"),
                Err(e) => {
                    println!("    Expected error: {}", e);
                    match e {
                        Error::InvalidRequest(msg) => println!("    Client-side validation: {}", msg),
                        Error::OpenAiError(oe) => {
                            println!("    OpenAI error: {}", oe);
                        }
                        _ => println!("    Error type: {:?}", e),
                    }
                }
            }
        }
        Err(e) => println!("  Client creation failed: {}", e),
    }
}

async fn test_error_recovery() -> Result<()> {
    println!("\n1. Graceful degradation - try primary, fallback to secondary:");
    test_graceful_degradation().await;
    
    println!("\n2. Retry with exponential backoff pattern:");
    test_retry_pattern().await;
    
    Ok(())
}

async fn test_graceful_degradation() {
    let anthropic_key = std::env::var("ANTHROPIC_API_KEY").ok();
    let openai_key = std::env::var("OPENAI_API_KEY").ok();
    
    let request = CompletionRequest {
        messages: vec![Message::user("What is 2+2?")],
        tools: None,
        options: None,
    };
    
    let mut response_text = None;
    
    if let Some(api_key) = anthropic_key {
        println!("  Attempting with Anthropic...");
        match Client::anthropic(
            AnthropicConfig {
                api_key,
                model: "claude-sonnet-4-20250514".into(),
                version: None,
            },
            None,
        ) {
            Ok(client) => {
                match client.complete(request.clone()).await {
                    Ok(response) => {
                        println!("  Success with Anthropic!");
                        response_text = Some(response.text().to_string());
                    }
                    Err(e) => {
                        println!("  Anthropic failed: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("  Failed to create Anthropic client: {}", e);
            }
        }
    }
    
    if response_text.is_none() {
        if let Some(api_key) = openai_key {
            println!("  Falling back to OpenAI...");
            match Client::openai(
                OpenAiConfig {
                    api_key,
                    model: "gpt-4o".into(),
                    organization: None,
                },
                None,
            ) {
                Ok(client) => {
                    match client.complete(request).await {
                        Ok(response) => {
                            println!("  Success with OpenAI!");
                            response_text = Some(response.text().to_string());
                        }
                        Err(e) => {
                            println!("  OpenAI failed: {}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("  Failed to create OpenAI client: {}", e);
                }
            }
        }
    }
    
    if let Some(text) = response_text {
        println!("  Final answer: {}", text);
    } else {
        println!("  All providers failed!");
    }
}

async fn test_retry_pattern() {
    use tokio::time::{sleep, Duration};
    
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("  Skipped: ANTHROPIC_API_KEY not set");
            return;
        }
    };
    
    let client = match Client::anthropic(
        AnthropicConfig {
            api_key,
            model: "claude-sonnet-4-20250514".into(),
            version: None,
        },
        None,
    ) {
        Ok(client) => client,
        Err(e) => {
            println!("  Failed to create client: {}", e);
            return;
        }
    };
    
    let request = CompletionRequest {
        messages: vec![Message::user("Hello")],
        tools: None,
        options: None,
    };
    
    let max_retries = 3;
    let mut retry_count = 0;
    let base_delay = Duration::from_millis(1000);
    
    loop {
        match client.complete(request.clone()).await {
            Ok(response) => {
                println!("  Success on attempt {}", retry_count + 1);
                println!("  Response: {}", response.text());
                break;
            }
            Err(e) => {
                retry_count += 1;
                
                if retry_count >= max_retries {
                    println!("  Failed after {} attempts", max_retries);
                    println!("  Last error: {}", e);
                    break;
                }
                
                let should_retry = match e {
                    Error::RateLimited { .. } => true,
                    Error::NetworkError(_) => true,
                    Error::ProviderError { status_code: Some(status), .. } 
                        if status >= 500 && status < 600 => true,
                    _ => false,
                };
                
                if should_retry {
                    let delay = base_delay * 2_u32.pow(retry_count - 1);
                    println!("  Attempt {} failed: {}", retry_count, e);
                    println!("  Retrying in {:?}...", delay);
                    sleep(delay).await;
                } else {
                    println!("  Non-retryable error: {}", e);
                    break;
                }
            }
        }
    }
}