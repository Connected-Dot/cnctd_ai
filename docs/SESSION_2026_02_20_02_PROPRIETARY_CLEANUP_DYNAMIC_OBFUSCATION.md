# Session: Proprietary Cleanup & Dynamic Obfuscation via HTTP Source

**Date:** 2026-02-20
**Branch:** main
**Context:** cnctd_ai core + cnctd_ai_server obfuscation system

## TL;DR

Audited and removed all proprietary references from the codebase to prepare cnctd_ai as a public repository. Simultaneously completed the dynamic obfuscation overhaul -- replacing hardcoded entity types and Postgres-backed dictionaries with a fully dynamic HTTP source URL pattern. Published as v0.1.23 to crates.io.

## Summary

This session (spanning two conversations due to context limits) addressed two intertwined goals: making the cnctd_ai repository safe for public open-source release, and completing the architectural shift to fully dynamic obfuscation in cnctd_ai_server.

**Proprietary cleanup**: A systematic grep-based audit identified proprietary references in 8 files across the codebase. These included hardcoded company names in documentation examples, client-specific environment variable names, personal filesystem paths in example code, references to the original standalone repository name, and client-specific workflow instructions in agent definitions. Every instance was replaced with generic alternatives -- real company names became placeholders like "Acme News" and "Widget Sports," environment variables were renamed to generic forms (`TRANSMIT_MCP_URL` became `MCP_SERVER_URL`), and hardcoded paths were replaced with environment variable lookups.

**Dynamic obfuscation**: The obfuscation system was overhauled from a model where entity types were defined by a Rust enum (`EntityType::Channel`, `EntityType::Bidder`, etc.) backed by Postgres queries, to a fully dynamic system where the calling application hosts an HTTP endpoint that returns its own entity dictionary. The new `KeyInferenceEngine` automatically derives key matching patterns from arbitrary type names (e.g., type "channel" generates patterns `channel_id`, `channelid`, `channel_ids`, `channelids`). A new cache invalidation endpoint (`POST /obfuscation/invalidate`) lets calling applications trigger dictionary refreshes when their data changes.

**Public repo guardrails**: Added "Public Repo Safety" sections to both the session-summary-writer and post-mortem-writer agent definitions, codifying rules that prevent future session documentation from inadvertently including client names, proprietary details, personal data, or internal URLs.

The session concluded with a version bump to 0.1.23, a commit of all 23 changed files (873 insertions, 305 deletions), push to GitHub, and publish to crates.io.

## Changes Made

### Proprietary Reference Removal

- Renamed `transmit_mcp_url` config field to `mcp_server_url` in `config.rs` and `state.rs`
- Renamed `TRANSMIT_MCP_URL` environment variable to `MCP_SERVER_URL`
- Replaced hardcoded Google Drive path in `examples/video_analysis.rs` with `VIDEO_PATH` env var lookup
- Genericized all company names in `OBFUSCATION_SETUP.md` (ESPN -> Acme News, Fox Sports -> Widget Sports, Magnite -> AdExchange Co, PubMatic -> BidPlatform Inc, Nike -> Example Brand)
- Removed "from llm-service" references and client-specific IP notes from `CLAUDE.md`
- Removed "Working on Behalf of Clients" section from `CLAUDE.md`
- Cleaned client-specific references from `.claude/agents/session-summary-writer.md` and `.claude/agents/post-mortem-writer.md`
- Updated prior session summary `docs/SESSION_2026_02_20_01_WORKSPACE_RESTRUCTURE.md` to remove proprietary references

### Dynamic Obfuscation System

- **New file**: `crates/cnctd_ai_server/src/obfuscation/source.rs` -- Defines `ObfuscationSource`, `SourceEntity`, `SourceResponse` types and `fetch_source()` async function for HTTP entity dictionary retrieval
- **Refactored**: `entity_dictionary.rs` -- Replaced `EntityType` enum and Postgres queries with `String`-keyed `HashMap` lookups; `EntityDictionary` now built from `SourceEntity` vectors
- **Refactored**: `tokenizer.rs` -- `HmacTokenizer` now accepts dynamic type names as strings instead of `EntityType` enum variants; Aho-Corasick patterns built from string type names
- **Refactored**: `obfuscator.rs` -- Added `KeyInferenceEngine` that auto-generates key matching patterns from type names; `Obfuscator` now works with string-typed entities throughout
- **Refactored**: `session.rs` -- `ObfuscationSession` now fetches entity data via HTTP source URL instead of Postgres; added `invalidate_cache()` method for triggered refreshes
- **Refactored**: `numeric_scaler.rs` -- Added support for custom per-metric scaling rules from the source response, with sensible defaults as fallback
- **New file**: `crates/cnctd_ai_server/src/routes/obfuscation.rs` -- `POST /obfuscation/invalidate` endpoint with bearer token auth
- **Updated**: `routes/mod.rs` -- Registered new obfuscation route module
- **Updated**: `routes/chat.rs` -- Adapted to use dynamic deobfuscator from session state
- **Updated**: `main.rs` -- Registered obfuscation routes on the Axum router

### Documentation

- **New file**: `crates/cnctd_ai_server/docs/OBFUSCATION_SETUP.md` -- Comprehensive integration guide for calling applications, covering source endpoint contract, entity format, key inference rules, numeric scaling configuration, cache invalidation, and SSE event format. All examples use generic placeholder names.
- **Updated**: `CLAUDE.md` -- Added cnctd_ai_server section with obfuscation architecture docs, environment variable table, and updated project structure tree

### Public Repo Safety Guardrails

- Added "Public Repo Safety" sections to `.claude/agents/session-summary-writer.md` and `.claude/agents/post-mortem-writer.md` with explicit rules:
  - Never include real client or company names
  - Never include proprietary business details
  - Never include personal data or internal URLs
  - Use generic descriptions when referencing client-specific work

## Files Modified

**Library (cnctd_ai):**
- `Cargo.toml` - Version bump to 0.1.23
- `README.md` - General updates
- `CLAUDE.md` - Removed proprietary references, added cnctd_ai_server documentation
- `examples/video_analysis.rs` - Replaced hardcoded path with `VIDEO_PATH` env var

**Server (cnctd_ai_server):**
- `crates/cnctd_ai_server/Cargo.toml` - Dependency updates
- `crates/cnctd_ai_server/docs/OBFUSCATION_SETUP.md` - New integration guide
- `crates/cnctd_ai_server/src/config.rs` - Renamed `transmit_mcp_url` to `mcp_server_url`
- `crates/cnctd_ai_server/src/main.rs` - Registered obfuscation routes
- `crates/cnctd_ai_server/src/state.rs` - Updated to use new config field name
- `crates/cnctd_ai_server/src/obfuscation/entity_dictionary.rs` - String-keyed entity lookup (major refactor)
- `crates/cnctd_ai_server/src/obfuscation/mod.rs` - Added `source` module export
- `crates/cnctd_ai_server/src/obfuscation/numeric_scaler.rs` - Custom per-metric scaling rules
- `crates/cnctd_ai_server/src/obfuscation/obfuscator.rs` - KeyInferenceEngine + dynamic types
- `crates/cnctd_ai_server/src/obfuscation/session.rs` - HTTP fetch + cache invalidation
- `crates/cnctd_ai_server/src/obfuscation/tokenizer.rs` - Dynamic type name support
- `crates/cnctd_ai_server/src/obfuscation/source.rs` - New HTTP source types and fetch
- `crates/cnctd_ai_server/src/routes/chat.rs` - Dynamic deobfuscator integration
- `crates/cnctd_ai_server/src/routes/mod.rs` - New obfuscation route module
- `crates/cnctd_ai_server/src/routes/obfuscation.rs` - New cache invalidation endpoint

**Infrastructure:**
- `.claude/agents/session-summary-writer.md` - Removed client refs, added public repo safety
- `.claude/agents/post-mortem-writer.md` - Removed client refs, added public repo safety
- `.claude/settings.local.json` - Permission updates

**Documentation:**
- `docs/SESSION_2026_02_20_01_WORKSPACE_RESTRUCTURE.md` - Genericized proprietary references

## Architecture Notes

### HTTP Source URL Pattern

The obfuscation system's shift from Postgres-backed entity dictionaries to HTTP source URLs is a significant architectural change. The calling application now owns its data entirely -- cnctd_ai_server has zero knowledge of what entity types exist, what they're called, or how they're structured. It simply calls a `GET` endpoint and receives:

```json
{
  "entities": [
    { "type": "channel", "id": 123, "name": "Acme News" }
  ],
  "key_inference_overrides": { ... },
  "numeric_rules": { ... }
}
```

This makes the obfuscation system truly reusable across different domains -- it works equally well for ad-tech entities, e-commerce products, or any other domain that needs LLM data protection.

### KeyInferenceEngine

The `KeyInferenceEngine` automatically generates key matching patterns from entity type names. For a type named "channel", it generates: `channel_id`, `channelid`, `channel_ids`, `channelids`. This means calling applications don't need to enumerate every possible key format -- the engine handles common conventions. Custom overrides can be provided for edge cases via the `key_inference_overrides` field in the source response.

### Cache Invalidation

The `POST /obfuscation/invalidate` endpoint allows calling applications to trigger a dictionary refresh when their data changes (e.g., a new channel is created). The endpoint is authenticated via the same bearer token used for source URL requests (`OBFUSCATION_SOURCE_TOKEN`). This avoids polling and keeps the obfuscation layer in sync with the source of truth.

### Public Repo Decision

Session summaries are kept in the repository (under `docs/`) rather than stored externally. The rationale: git provides free multi-machine sync, the summaries are useful project history, and the guardrails codified in the agent definitions prevent proprietary leaks. This is a pragmatic choice that trades a small amount of repo size for significant developer convenience.

## Next Steps

- [ ] Integration test the HTTP source URL fetch with a mock server
- [ ] Test cache invalidation endpoint end-to-end
- [ ] Update calling application to host the new source endpoint format
- [ ] Monitor that the KeyInferenceEngine patterns cover all real-world key formats
- [ ] Consider adding TTL-based auto-refresh as a complement to manual invalidation
- [ ] Verify v0.1.23 works correctly when consumed as a crates.io dependency
