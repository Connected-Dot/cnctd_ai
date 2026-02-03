//! Example: Document/PDF Input
//!
//! This example demonstrates how to send documents to AI models for analysis.
//!
//! **Important Provider Differences:**
//! - Anthropic: Base64 document blocks only support PDFs. For text files,
//!   include content directly in the message.
//! - Gemini: Supports various document types including text/plain, text/csv
//!   via document blocks.
//!
//! Run with: cargo run --example document_input

use anyhow::Result;
use cnctd_ai::{
    AnthropicConfig, GeminiConfig, Client, CompletionRequest, Message,
    DocumentContent,
};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    println!("=== Document Input Example ===\n");

    // Test with Anthropic
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        println!("Testing with Anthropic Claude...\n");
        test_anthropic_document().await?;
    } else {
        println!("Skipping Anthropic test (ANTHROPIC_API_KEY not set)\n");
    }

    // Test with Gemini
    if std::env::var("GEMINI_API_KEY").is_ok() {
        println!("\nTesting with Google Gemini...\n");
        test_gemini_document().await?;
    } else {
        println!("Skipping Gemini test (GEMINI_API_KEY not set)\n");
    }

    Ok(())
}

async fn test_anthropic_document() -> Result<()> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")?;

    let client = Client::anthropic(
        AnthropicConfig {
            api_key,
            model: "claude-sonnet-4-20250514".into(),
            version: None,
        },
        None,
    )?;

    // Sample text document content
    // Note: Anthropic's base64 document blocks only support PDFs.
    // For text files, we include the content directly in the message.
    let sample_text = r#"# Project Requirements Document

## Overview
This document outlines the requirements for the new customer portal.

## Functional Requirements
1. User authentication via OAuth 2.0
2. Dashboard showing account summary
3. Transaction history with filtering
4. Profile management
5. Support ticket system

## Non-Functional Requirements
- Response time < 200ms for all pages
- 99.9% uptime SLA
- Support for 10,000 concurrent users
- WCAG 2.1 AA accessibility compliance

## Timeline
- Phase 1: Authentication & Dashboard (Q1)
- Phase 2: Transactions & Profile (Q2)
- Phase 3: Support System (Q3)"#;

    println!("Document: requirements.txt (included as text)");
    println!("Note: Anthropic base64 document blocks only support PDFs.");
    println!("      For text files, include content directly in the message.\n");

    // For Anthropic with non-PDF documents, include content directly in the message
    let request = CompletionRequest {
        messages: vec![
            Message::user(format!(
                "Here is a requirements document:\n\n{}\n\nPlease summarize this document and identify the key milestones.",
                sample_text
            )),
        ],
        tools: None,
        built_in_tools: None,
        tool_config: None,
        options: None,
    };

    let response = client.complete(request).await?;

    println!("Claude's Analysis:\n");
    println!("{}", response.text());
    println!("\n---");
    println!("Tokens used: {}", response.usage.total_tokens);

    Ok(())
}

async fn test_gemini_document() -> Result<()> {
    let api_key = std::env::var("GEMINI_API_KEY")?;

    let client = Client::gemini(
        GeminiConfig {
            api_key,
            model: "gemini-2.0-flash".into(),
        },
        None,
    )?;

    // Create a sample CSV document
    let sample_csv = r#"Name,Department,Salary,Start Date
Alice Johnson,Engineering,95000,2022-03-15
Bob Smith,Marketing,72000,2021-06-01
Carol Williams,Engineering,105000,2020-01-10
David Brown,Sales,68000,2023-02-28
Eve Davis,Engineering,88000,2022-09-01
"#;

    let base64_doc = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        sample_csv.as_bytes()
    );

    let doc = DocumentContent::csv(base64_doc)
        .with_filename("employees.csv");

    println!("Document: employees.csv");
    println!("Type: {}", doc.media_type);
    println!();

    let request = CompletionRequest {
        messages: vec![
            Message::user_with_document(
                "Analyze this employee data. What's the average salary by department? Who has the highest salary?",
                doc,
            ),
        ],
        tools: None,
        built_in_tools: None,
        tool_config: None,
        options: None,
    };

    let response = client.complete(request).await?;

    println!("Gemini's Analysis:\n");
    println!("{}", response.text());
    println!("\n---");
    println!("Tokens used: {}", response.usage.total_tokens);

    Ok(())
}

// Example of loading a document from file (commented out since we don't have a test file)
#[allow(dead_code)]
async fn load_pdf_example() -> Result<()> {
    // Load a PDF file directly
    let doc = DocumentContent::from_file("path/to/document.pdf").await?;

    println!("Loaded document:");
    println!("  Filename: {:?}", doc.filename);
    println!("  Media type: {}", doc.media_type);
    println!("  Data size: {} bytes (base64)", doc.data.len());

    Ok(())
}
