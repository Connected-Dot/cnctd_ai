use rmcp::model::{CallToolResult, RawContent};

/// Convert CallToolResult content to a simple string representation
///
/// This is a convenience function for extracting text content from tool results.
/// Note that this flattens the result and loses structured information like images
/// and resources, which are replaced with placeholder text.
///
/// For applications that need to preserve full tool result structure, work with
/// the CallToolResult directly instead of using this helper.
///
/// # Example
///
/// ```no_run
/// use cnctd_ai::mcp::{McpClient, tool_result_to_string};
///
/// # async fn example(client: McpClient) -> Result<(), Box<dyn std::error::Error>> {
/// let result = client.call_tool("some_tool", None).await?;
/// let text = tool_result_to_string(&result);
/// println!("Tool returned: {}", text);
/// # Ok(())
/// # }
/// ```
pub fn tool_result_to_string(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|annotated| match &annotated.raw {
            RawContent::Text(text) => Some(text.clone()),
            RawContent::Image(_) => Some(rmcp::model::RawTextContent {
                text: "[image]".to_string(),
                meta: None,
            }),
            RawContent::Resource(_) => Some(rmcp::model::RawTextContent {
                text: "[resource]".to_string(),
                meta: None,
            }),
            RawContent::Audio(_) => Some(rmcp::model::RawTextContent {
                text: "[audio]".to_string(),
                meta: None,
            }),
            RawContent::ResourceLink(_) => Some(rmcp::model::RawTextContent {
                text: "[resource-link]".to_string(),
                meta: None,
            }),
        })
        .map(|raw_text| raw_text.text)
        .collect::<Vec<_>>()
        .join("\n")
}
