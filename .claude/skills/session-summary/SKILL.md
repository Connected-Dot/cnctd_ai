---
name: session-summary
description: Generate a session summary document from the current conversation. Use when wrapping up a productive coding session or when the user explicitly requests a summary.
---

# Session Summary Workflow

This skill orchestrates the full session summary workflow:
1. Commits any uncommitted code changes
2. Launches the session-summary-writer agent in background
3. Waits for completion and writes the summary file
4. Commits the summary to the repo

## Step 1: Save Code First

Before generating the summary, ensure all code is committed:

```
1. Run `git status` to check for uncommitted changes
2. If there are changes:
   - Stage relevant files (avoid .env, credentials)
   - Commit with a descriptive message
   - Push to the current branch
3. Tell the user: "Code saved. Now generating session summary..."
```

## Step 2: Launch Summary Agent

Launch the session-summary-writer agent in background mode:

```
Task tool parameters:
- subagent_type: "session-summary-writer"
- run_in_background: true
- prompt: |
    Write a session summary for the cnctd_ai project.

    Transcript location: ~/.claude/projects/<PROJECT_PATH_HASH>/<SESSION_ID>.jsonl
    Working directory: <CURRENT_WORKING_DIR>
    Branch: <CURRENT_BRANCH>

    Where PROJECT_PATH_HASH is the working directory with / replaced by -
```

**Important**: The prompt must include the actual paths for this session.

## Step 3: Poll for Completion

Use TaskOutput to check the agent's progress:

```
TaskOutput tool parameters:
- task_id: <id from Task result>
- block: true
- timeout: 120000  (2 minutes should be enough)
```

The agent returns output in this exact format:
```
===FILENAME===
SESSION_YYYY_MM_DD_NN_TOPIC.md
===COMMIT_MESSAGE===
Add session summary: <topic>
===CONTENT===
<full markdown content>
===END===
```

## Step 4: Write and Commit Summary

Parse the output and handle file operations:

1. Extract filename, commit message, and content from the agent output
2. Write the file to the docs directory:
   `/Users/kyleebner/Development/ConnectedDot/cnctd/modules/rust/cnctd_ai/docs/<filename>`
3. Commit and push:
   ```bash
   cd /Users/kyleebner/Development/ConnectedDot/cnctd/modules/rust/cnctd_ai
   git add docs/SESSION_*.md
   git commit -m "<commit message from agent>"
   git push
   ```

## Step 5: Confirm Completion

Tell the user:
- "Session summary saved: docs/<filename>"
- Include a brief TL;DR from the summary content

## Error Handling

- If agent times out: Offer to retry or skip the summary
- If file write fails: Show the error and suggest manual steps
- If commit fails: Check if there are conflicts, offer resolution
