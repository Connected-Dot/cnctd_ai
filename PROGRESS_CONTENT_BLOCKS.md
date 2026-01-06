# Content Blocks Implementation Progress

## Goal
Fix HTTP 400 error: "tool_use ids were found without tool_result blocks immediately after"

Anthropic API requires all tool_result blocks from one assistant turn to be in a **single** user message:
```json
{
  "role": "user",
  "content": [
    {"type": "tool_result", "tool_use_id": "id1", "content": "result1"},
    {"type": "tool_result", "tool_use_id": "id2", "content": "result2"}
  ]
}
```

Current implementation creates separate user messages per tool result.

## Changes Required

### cnctd_ai
1. [x] Add `ContentBlock` enum to represent text, tool_use, tool_result
2. [ ] Add `Message::tool_results()` constructor for multiple tool results
3. [ ] Update serialization to group tool results properly
4. [ ] Test with multiple tool results in one turn

### cnctd.world  
1. [ ] Update `results_to_messages()` to return single message with all results
2. [ ] Test tool streaming end-to-end

## Progress Log

### Session Start
- Reviewed MESSAGE_CONTENT_BLOCKS_AND_TOOL_KNOWLEDGE.md architecture doc
- Identified root cause: each tool result becomes separate API message
- Starting implementation in cnctd_ai

