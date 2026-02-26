# Session: Media Upload Bugfixes -- Obfuscation & Anthropic Provider
**Date:** 2026-02-26
**Branch:** main
**Context:** cnctd_ai core + cnctd_ai_server

## TL;DR
Fixed two bugs that broke image and document uploads in the chat UI. The obfuscation layer was stripping media from user messages, and the Anthropic provider was rejecting non-PDF documents with HTTP 400. Both fixes were committed, pushed, and redeployed.

## Summary
This session addressed two bugs discovered during live testing of the media input support that was added in previous sessions (a 10-phase implementation). The first bug was in the cnctd_ai_server obfuscation pipeline: when obfuscating user messages, the code was constructing a new `Message::user(&obfuscated)` but not copying over the `images` and `documents` fields from the original message. This meant any user message containing images or documents would have the media silently dropped before reaching the LLM.

The second bug was in the Anthropic provider within the core cnctd_ai library. Anthropic's API only supports `application/pdf` as the media type for document content blocks. When users uploaded CSV, TXT, or other text-based files, the provider was sending them as document blocks, causing Anthropic to return an HTTP 400 error. The fix introduced a three-way branch: PDFs use the native document block, text-based formats (text/*, application/json, application/xml) are decoded from base64 and sent as labeled text blocks, and unsupported binary formats (DOCX, XLSX, etc.) are skipped with a warning logged to stderr.

Both fixes were committed and the stack was redeployed successfully. Google-specific features (grounding, code execution) remain broken but are deprioritized. Future work includes an asset upload system using Cloudflare R2 and a database asset table.

## Changes Made

### Obfuscation Media Preservation
- Fixed `crates/cnctd_ai_server/src/routes/chat.rs` to preserve `images` and `documents` on obfuscated messages
- The obfuscation loop was creating a new `Message::user(&obfuscated)` which defaults images/documents to `None`
- Added `obf_msg.images = m.images.clone()` and `obf_msg.documents = m.documents.clone()` after constructing the obfuscated message
- This ensures media attachments survive the obfuscation pass and reach the LLM provider intact

### Anthropic Non-PDF Document Handling
- Fixed `src/client/anthropic.rs` to handle non-PDF document uploads gracefully
- Added three-way branch for document content blocks:
  1. **PDF** (`application/pdf`): Uses native Anthropic document block with base64 source (existing behavior)
  2. **Text-based** (`text/*`, `application/json`, `application/xml`): Decodes base64 data, wraps content in a labeled text block formatted as `[File: {filename} ({media_type})]\n{content}`
  3. **Binary non-PDF** (DOCX, XLSX, etc.): Skipped with a warning logged via `eprintln!`
- Also updated the condition that triggers multi-block message building from `msg.has_images()` to `msg.has_images() || msg.has_documents()`

## Files Modified

**Library (cnctd_ai):**
- `src/client/anthropic.rs` -- Added three-way document handling branch for PDF vs text-based vs unsupported binary formats; updated condition to check for documents alongside images

**Server (cnctd_ai_server):**
- `crates/cnctd_ai_server/src/routes/chat.rs` -- Preserved `images` and `documents` fields when constructing obfuscated user messages

## Architecture Notes

The obfuscation pipeline's message reconstruction pattern is worth noting as a recurring risk area. Any time a new `Message` is constructed from an existing one (for obfuscation, filtering, or transformation), all fields must be explicitly copied. The `Message` struct has many optional fields (`images`, `documents`, `videos`, `cache_control`, `tool_uses`, `tool_call_id`, `tool_results`, `reasoning_items`), and `Message::user()` only sets `role` and `content`. Future changes to the Message struct should audit all call sites that reconstruct messages.

The Anthropic text-based document fallback uses a simple labeled format (`[File: name (type)]\n...content...`). This is adequate for CSV and plain text but may lose formatting for richer text formats. If structured file understanding becomes important, a more sophisticated extraction pipeline could be added later.

## Next Steps
- [ ] Asset upload system: Cloudflare R2 storage + database asset table for persistent media management
- [ ] Google Gemini provider: grounding and code execution features are broken (deprioritized)
- [ ] Consider adding `Message::from_with_obfuscated_content()` or similar helper to reduce risk of field-dropping during message reconstruction
- [ ] Test document upload with more edge cases (very large files, unusual MIME types, empty files)
