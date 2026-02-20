# Session: Documentation Staleness Audit and Cleanup
**Date:** 2026-02-20
**Branch:** main
**Context:** cnctd_ai core

## TL;DR
Audited all official documentation files (README.md, CLAUDE.md, OBFUSCATION_SETUP.md, AGENT_FRAMEWORK.md) for stale or outdated references. Fixed three issues in README.md and one in AGENT_FRAMEWORK.md. CLAUDE.md and OBFUSCATION_SETUP.md were already clean.

## Summary
After several rapid sessions of restructuring, feature work, and dependency upgrades, the project's public-facing documentation had accumulated a few stale references that no longer reflected reality. This session performed a targeted audit of the four main documentation files to identify and fix outdated content.

The audit focused specifically on "actually stale" items -- hardcoded version numbers that would confuse new users, promotional labels like "(NEW!)" that had outlived their usefulness, "(coming soon)" markers for features that either already exist or were removed, and references to non-existent files. Model names in code examples (e.g., `claude-sonnet-4-20250514`, `gpt-4o`) were explicitly left alone, as these serve as illustrative examples and do not need to track the latest model releases.

## Changes Made

### README.md -- Installation Section
- Replaced hardcoded `cnctd_ai = "0.1.5"` TOML snippet with `cargo add cnctd_ai` bash command, which always installs the latest version and avoids the version going stale again
- Changed the code block language hint from `toml` to `bash` to match the new content

### README.md -- Heading and Link Labels
- Removed "(NEW!)" from the "Agent Framework" heading -- the agent framework is now an established feature, not a new addition
- Removed "(coming soon)" from the API Documentation link to `docs.rs/cnctd_ai` -- the link works and should not be qualified with a disclaimer

### AGENT_FRAMEWORK.md -- Example File References
- Removed the bullet point referencing `agent_custom_tools.rs` with "(coming soon)" -- this example file does not exist and there are no immediate plans to create it

### No Changes Needed
- `CLAUDE.md` -- Already accurate and up to date after the v0.1.23 session updates
- `crates/cnctd_ai_server/docs/OBFUSCATION_SETUP.md` -- Already accurate after the dynamic obfuscation refactor

## Files Modified

**Library (cnctd_ai):**
- `README.md` -- Replaced hardcoded version with `cargo add`, removed stale "(NEW!)" and "(coming soon)" labels
- `docs/AGENT_FRAMEWORK.md` -- Removed reference to non-existent example file

## Architecture Notes
The decision to use `cargo add cnctd_ai` instead of a version-pinned TOML snippet is a low-maintenance pattern. It means the README never needs updating when a new version is published. For projects where users should pin to a specific version range, a different approach (e.g., showing `cargo add cnctd_ai@0.1` for semver range) could be considered, but for this library the simple form is appropriate.

Model names in code examples were intentionally left as-is. These are illustrative -- they show the user what kind of string goes in the model field, not which specific model to use today. Updating them with every model release would create unnecessary churn.

## Next Steps
- [ ] No immediate follow-up needed -- documentation is now clean
- [ ] Consider adding a CI check or periodic reminder to audit docs after major version bumps
