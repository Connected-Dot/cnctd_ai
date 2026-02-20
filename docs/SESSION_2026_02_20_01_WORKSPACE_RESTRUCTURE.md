# Session: Workspace Restructure & Session Infrastructure

**Date:** 2026-02-20
**Branch:** main
**Context:** cnctd_ai core + cnctd_ai_server (new subcrate)

## TL;DR

Absorbed the standalone `llm-service` repository into cnctd_ai as a new subcrate `crates/cnctd_ai_server/`, resolving Cargo nested workspace limitations. Also established session summary infrastructure including custom agents, hooks, skills, and settings for persistent development context.

## Summary

This session addressed two major objectives for the cnctd_ai project. The first phase focused on absorbing the standalone `llm-service` Rust project (a Transmit Live work product) into the cnctd_ai repository as a subcrate called `cnctd_ai_server`. This required careful architectural planning due to Cargo's limitation that nested workspaces cannot exist when the outer workspace already includes inner crates. The solution was to keep cnctd_ai as a `[package]` at the root level with subcrates housed under `crates/`, and register both `modules/rust/cnctd_ai` and `modules/rust/cnctd_ai/crates/*` in the parent cnctd monorepo's workspace.

The llm-service codebase contained a 4-point obfuscation system (user-to-LLM, LLM-to-tool, tool-to-LLM, LLM-to-user interception) with HMAC tokenization, Aho-Corasick name matching, numeric scaling, and a Postgres-backed entity dictionary. All 15 source files were migrated with proper dependency paths updated to reference cnctd_ai via `path = "../.."`.

The second phase established session summary infrastructure modeled after patterns from cnctd.world's hooks and skills system. This included two agent definitions (session-summary-writer and post-mortem-writer), shell hooks for session start/end, a skill orchestration workflow, local settings with permissions, and updates to the project's CLAUDE.md documentation.

## Changes Made

### Phase 1: cnctd_ai_server Subcrate

- Created `crates/cnctd_ai_server/` directory with full project structure
- Migrated 15 source files from llm-service:
  - `Cargo.toml` - Package definition with cnctd_ai path dependency
  - `src/config.rs` - Server configuration
  - `src/error.rs` - Error types
  - `src/main.rs` - Entry point
  - `src/state.rs` - Application state
  - `src/obfuscation/mod.rs` - Obfuscation module root
  - `src/obfuscation/entity_dictionary.rs` - Postgres-backed entity lookup
  - `src/obfuscation/numeric_scaler.rs` - Numeric value obfuscation
  - `src/obfuscation/obfuscator.rs` - Core obfuscation orchestrator
  - `src/obfuscation/session.rs` - Session-scoped obfuscation state
  - `src/obfuscation/tokenizer.rs` - HMAC-based name tokenization
  - `src/routes/mod.rs` - Route definitions
  - `src/routes/agents.rs` - Agent endpoints
  - `src/routes/chat.rs` - Chat endpoints
  - `src/routes/health.rs` - Health check endpoint
  - `src/routes/models.rs` - Model listing endpoint
- Updated dependency paths to use `cnctd_ai = { path = "../.." }`

### Phase 2: Session Summary Infrastructure

- Created `.claude/agents/session-summary-writer.md` - Detailed agent specification for analyzing JSONL transcripts and producing structured markdown summaries
- Created `.claude/agents/post-mortem-writer.md` - Agent specification for incident analysis documentation
- Created `.claude/hooks/session-start.sh` - Injects previous session context at conversation start
- Created `.claude/hooks/session-end.sh` - Logs session end timestamps and syncs settings
- Created `.claude/skills/session-summary/SKILL.md` - Orchestration workflow for the session summary process
- Created `.claude/settings.local.json` - Permissions configuration and hook wiring
- Updated `CLAUDE.md` with project structure documentation including cnctd_ai_server details

## Files Modified

**New Subcrate (cnctd_ai_server):**
- `crates/cnctd_ai_server/Cargo.toml` - Package manifest with path dependencies
- `crates/cnctd_ai_server/src/config.rs` - Server configuration structs
- `crates/cnctd_ai_server/src/error.rs` - Error type definitions
- `crates/cnctd_ai_server/src/main.rs` - Server entry point
- `crates/cnctd_ai_server/src/state.rs` - Application state management
- `crates/cnctd_ai_server/src/obfuscation/mod.rs` - Obfuscation module exports
- `crates/cnctd_ai_server/src/obfuscation/entity_dictionary.rs` - Entity lookup from Postgres
- `crates/cnctd_ai_server/src/obfuscation/numeric_scaler.rs` - Numeric value scaling
- `crates/cnctd_ai_server/src/obfuscation/obfuscator.rs` - Core obfuscation logic
- `crates/cnctd_ai_server/src/obfuscation/session.rs` - Per-session obfuscation state
- `crates/cnctd_ai_server/src/obfuscation/tokenizer.rs` - HMAC tokenization with Aho-Corasick
- `crates/cnctd_ai_server/src/routes/mod.rs` - Route module organization
- `crates/cnctd_ai_server/src/routes/agents.rs` - Agent API endpoints
- `crates/cnctd_ai_server/src/routes/chat.rs` - Chat API endpoints
- `crates/cnctd_ai_server/src/routes/health.rs` - Health check endpoint
- `crates/cnctd_ai_server/src/routes/models.rs` - Model listing endpoint

**Infrastructure:**
- `.claude/agents/session-summary-writer.md` - Session summary agent definition
- `.claude/agents/post-mortem-writer.md` - Post-mortem agent definition
- `.claude/hooks/session-start.sh` - Session start hook
- `.claude/hooks/session-end.sh` - Session end hook
- `.claude/skills/session-summary/SKILL.md` - Summary orchestration skill
- `.claude/settings.local.json` - Local Claude settings

**Documentation:**
- `CLAUDE.md` - Updated with cnctd_ai_server documentation and project structure

## Architecture Notes

### Cargo Workspace Resolution
The key architectural decision was how to handle the nested workspace problem. Cargo does not support a workspace-within-workspace pattern when the outer workspace already includes inner crates. The solution adopted:

1. **cnctd_ai remains a `[package]`** at the repository root (not a `[workspace]`)
2. **Subcrates live under `crates/`** (e.g., `crates/cnctd_ai_server/`)
3. **The parent cnctd monorepo** registers both `"modules/rust/cnctd_ai"` and `"modules/rust/cnctd_ai/crates/*"` in its workspace members
4. **Subcrates reference cnctd_ai** via `path = "../.."`

This preserves the ability to `cargo build` from the monorepo root while keeping related code co-located.

### IP Protection Boundary
The obfuscation system in cnctd_ai_server is a Transmit Live work product, distinct from the Connected Dot open-source platform code in cnctd_ai core. This boundary is important for licensing and IP attribution.

### Obfuscation Architecture
The 4-point interception system provides data protection at every stage of the LLM conversation:
1. **User -> LLM**: Obfuscate sensitive data before sending to the model
2. **LLM -> Tool**: Obfuscate tool call arguments
3. **Tool -> LLM**: Obfuscate tool results before returning to the model
4. **LLM -> User**: De-obfuscate the final response for the user

Components use HMAC-SHA256 for deterministic tokenization (same input always produces same token within a session), Aho-Corasick for efficient multi-pattern name matching, and configurable numeric scaling for financial data protection.

## Next Steps
- [ ] Verify cnctd_ai_server builds successfully within the parent monorepo workspace
- [ ] Add integration tests for the obfuscation pipeline
- [ ] Wire up the session-summary-writer agent as a post-session automation (current limitation: custom agent types not available as subagent invocations from Claude Code)
- [ ] Consider publishing cnctd_ai_server to the private cargo registry
- [ ] Investigate whether inventory-manager (Node/TypeScript) needs any interface updates for the new crate structure
