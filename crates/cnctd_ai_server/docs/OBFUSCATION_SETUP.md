# Obfuscation Setup Guide

This guide explains how to integrate your application with `cnctd_ai_server`'s data obfuscation layer. The obfuscation system protects sensitive entity data (company names, IDs, financial metrics) during AI conversations by replacing real values with deterministic tokens before they reach the LLM, and reversing the process in responses.

## Overview

The obfuscation system intercepts data at four points in every AI conversation:

1. **User -> LLM**: Entity names in user messages are replaced with HMAC tokens
2. **LLM -> Tool**: Tokens in tool call arguments are replaced with real IDs/names before MCP tool execution
3. **Tool -> LLM**: Entity data in tool results is tokenized and numeric metrics are scaled before returning to the LLM
4. **LLM -> User**: Tokens in the LLM's response are replaced with real entity names before sending to the client

This means the LLM never sees your real entity names, IDs, or financial figures. It works with opaque tokens like `channel_a1b2` instead of real names, and scaled numbers instead of real revenue.

## Architecture

```
Your App                    cnctd_ai_server                     LLM
  |                              |                               |
  |--- POST /chat -------------->|                               |
  |   { session_salt: "abc" }    |                               |
  |                              |-- GET source_url ------------->|
  |                              |<-- entity dictionary ---------|  (your app's endpoint)
  |                              |                               |
  |                              |-- obfuscate user msg -------->|
  |                              |<-- LLM response (tokens) -----|
  |                              |-- deobfuscate tool args ----->|  (MCP tool)
  |                              |<-- tool result ---------------|
  |                              |-- obfuscate tool result ----->|
  |                              |<-- LLM response (tokens) -----|
  |                              |-- deobfuscate for user ------>|
  |<-- SSE stream (real names) --|                               |
```

## Server Configuration

Set these environment variables on `cnctd_ai_server`:

| Variable | Required | Description |
|----------|----------|-------------|
| `OBFUSCATION_KEY` | Yes | HMAC secret key for deterministic tokenization. Use a random 32+ character string. This key determines how entity names map to tokens. |
| `OBFUSCATION_SOURCE_URL` | Yes | URL of your application's entity endpoint (see [Source URL Endpoint](#source-url-endpoint) below). |
| `OBFUSCATION_SOURCE_TOKEN` | Yes | Bearer token for authenticating requests to your source URL and the cache invalidation endpoint. |

Obfuscation is **enabled** when all three variables are set. If any are missing, obfuscation is disabled and the server operates in pass-through mode.

Example:

```bash
OBFUSCATION_KEY="your-random-secret-key-at-least-32-chars"
OBFUSCATION_SOURCE_URL="https://your-app.example.com/api/obfuscation/entities"
OBFUSCATION_SOURCE_TOKEN="your-shared-bearer-token"
```

## Source URL Endpoint

Your application must host an HTTP endpoint that returns the entity dictionary. The server fetches this endpoint:

- **Method**: `GET`
- **Headers**: `Authorization: Bearer {OBFUSCATION_SOURCE_TOKEN}`
- **Response**: JSON conforming to the schema below

### Response Schema

```json
{
  "entities": [
    { "type": "channel", "id": 42, "name": "Acme News" },
    { "type": "channel", "id": 99, "name": "Widget Sports" },
    { "type": "bidder", "id": 7, "name": "AdExchange Co" },
    { "type": "bidder", "id": 12, "name": "BidPlatform Inc" },
    { "type": "advertiser", "id": 100, "name": "Example Brand" }
  ],
  "key_inference_overrides": {
    "channel": {
      "id_patterns": ["channelids"],
      "name_patterns": ["channelname", "sitename"]
    }
  },
  "numeric_rules": [
    { "key": "revenue", "min_scale": 0.2, "max_scale": 0.8 },
    { "key": "impressions", "min_scale": 1.5, "max_scale": 3.0 }
  ]
}
```

### Fields

#### `entities` (required)

An array of all entities to obfuscate. Each entity has:

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Entity category (e.g., `"channel"`, `"bidder"`, `"advertiser"`). Lowercase recommended. You define your own types. |
| `id` | integer | The entity's numeric ID in your system. |
| `name` | string | The entity's display name that should be hidden from the LLM. |

The `type` field serves as a namespace: channel ID 42 and bidder ID 42 are distinct entities that get different tokens. The type name also gives the LLM semantic context (it sees `channel_a1b2` and understands it's a channel).

**Important**: Include ALL entities that might appear in tool results or user messages. Missing entities will pass through unobfuscated.

#### `key_inference_overrides` (optional)

Extra patterns for mapping JSON keys to entity types, beyond what the server auto-derives.

**Auto-derived patterns**: For each entity type name, the server automatically generates these ID field patterns:
- `{type}_id` (e.g., `channel_id`)
- `{type}id` (e.g., `channelid`)
- `{type}ids` (e.g., `channelids`)
- `{type}_ids` (e.g., `channel_ids`)

These patterns tell the server: "when I see a JSON key called `channel_id`, its value is a channel entity ID."

**When to use overrides**: If your tool results use non-standard key names that don't match the auto-derived patterns. Overrides are **additive** -- they add to the auto-derived patterns, they don't replace them.

Each override entry is keyed by entity type name (lowercase) and has:

| Field | Type | Description |
|-------|------|-------------|
| `id_patterns` | string[] | Additional JSON key names that hold IDs of this entity type. |
| `name_patterns` | string[] | Additional JSON key names that hold names of this entity type. |

Example: If your tool returns `{"sitename": "Acme News"}` for channels, add `"sitename"` to channel's `name_patterns`.

#### `numeric_rules` (optional)

Custom numeric scaling rules. When the server encounters a numeric value under a matching JSON key, it multiplies by a random factor (chosen once per session) within the given range.

| Field | Type | Description |
|-------|------|-------------|
| `key` | string | The JSON key name to match (e.g., `"revenue"`, `"impressions"`). |
| `min_scale` | float | Minimum scaling factor. |
| `max_scale` | float | Maximum scaling factor. |

If omitted (`null` or absent), the server uses built-in defaults:

| Key | Min Scale | Max Scale | Effect |
|-----|-----------|-----------|--------|
| `revenue`, `gross_revenue` | 0.2 | 0.8 | Reduces by 20-80% |
| `impressions`, `requests`, `bid_requests`, `bid_responses`, `ad_filled`, `ad_impression`, `complete`, `unique_users` | 1.5 | 3.0 | Inflates 1.5-3x |
| `cpm`, `avg_winning_bid` | 0.3 | 0.9 | Reduces by 10-70% |
| `fillrate`, `fill_rate`, `completion_rate`, `render_rate`, `win_rate` | 1.0 | 1.0 | No scaling (rates preserved) |

**How scaling works**: Within a session, each metric key gets one random factor from its range. All values for that metric are scaled by the same factor, so relative comparisons remain valid. The LLM can still say "Channel A has 2x the impressions of Channel B" -- the ratio is preserved, but the absolute numbers are fake.

## Client Integration

### Sending Chat Requests

Include `session_salt` in your chat requests to enable obfuscation:

```json
POST /chat
{
  "model": "claude-sonnet-4-20250514",
  "messages": [
    { "role": "user", "content": "Show me revenue for Acme News" }
  ],
  "session_salt": "user-123-session-456",
  "execute_tools": true,
  "tools": ["*"]
}
```

The `session_salt` is an arbitrary string that identifies the obfuscation session. Requests with the same salt share the same token mappings (deterministic within a session). Use a stable identifier like `{user_id}-{conversation_id}`.

If `session_salt` is omitted, the server generates a random UUID (tokens won't be consistent across requests).

### SSE Event Stream

The response is a Server-Sent Events stream. Obfuscation adds these event types:

#### `token_map` (emitted once at stream start)

The full entity-to-token mapping for this session. Useful for building an obfuscation inspector UI.

```json
{
  "type": "token_map",
  "data": {
    "entity_count": 42,
    "entries": [
      { "type": "channel", "id": 42, "name": "Acme News", "token": "channel_a1b2" },
      { "type": "bidder", "id": 7, "name": "AdExchange Co", "token": "bidder_c3d4" }
    ]
  }
}
```

#### `obfuscation_event` (emitted per interception)

Shows what was transformed at each interception point. Useful for debugging and building transparency UIs.

```json
{
  "type": "obfuscation_event",
  "data": {
    "stage": "user_to_llm",
    "tool_name": null,
    "tool_call_id": null,
    "before": "Show me revenue for Acme News",
    "after": "Show me revenue for channel_a1b2"
  }
}
```

Stages: `user_to_llm`, `llm_to_tool`, `tool_to_llm`, `llm_to_user`.

#### Standard events

- `text_delta` -- Streaming text chunks (already deobfuscated for the client)
- `tool_use_start`, `tool_use_delta`, `tool_use_complete` -- Tool call progress
- `tool_executing` -- Tool is being called (with obfuscated args as seen by LLM)
- `tool_result` -- Tool result with **real** (unobfuscated) data
- `done` -- Final event with usage stats

## Cache Invalidation

The server caches entity dictionaries per session salt (default TTL: 1 hour). When your entity data changes (new channels added, names updated, etc.), call the invalidation endpoint to force a re-fetch:

```
POST /obfuscation/invalidate
Authorization: Bearer {OBFUSCATION_SOURCE_TOKEN}
Content-Type: application/json

{
  "session_salt": "user-123-session-456"
}
```

**Responses**:
- `200 OK` -- Session invalidated (or didn't exist). Next request with this salt will re-fetch from the source URL.
- `401 Unauthorized` -- Invalid or missing bearer token.
- `404 Not Found` -- Obfuscation is not enabled on the server.

**When to invalidate**: Call this endpoint when your entity data changes. You can also invalidate all active sessions by calling it for each known salt, or simply wait for the TTL to expire.

## Example: Node.js / TypeScript Integration

### Hosting the source endpoint

```typescript
// Express/Fastify handler
app.get('/api/obfuscation/entities', async (req, res) => {
    // Verify bearer token
    const token = req.headers.authorization?.replace('Bearer ', '');
    if (token !== process.env.OBFUSCATION_SOURCE_TOKEN) {
        return res.status(401).json({ error: 'Unauthorized' });
    }

    // Fetch entities from your database
    const channels = await db.query('SELECT id, name FROM channel');
    const bidders = await db.query('SELECT id, name FROM bidder');
    const advertisers = await db.query('SELECT id, name FROM advertiser');

    res.json({
        entities: [
            ...channels.map(c => ({ type: 'channel', id: c.id, name: c.name })),
            ...bidders.map(b => ({ type: 'bidder', id: b.id, name: b.name })),
            ...advertisers.map(a => ({ type: 'advertiser', id: a.id, name: a.name })),
        ],
        key_inference_overrides: {
            channel: {
                id_patterns: ['channelids'],
                name_patterns: ['sitename', 'channelname'],
            },
        },
        // Use server defaults for numeric scaling
        // numeric_rules: null
    });
});
```

### Sending a chat request

```typescript
const response = await fetch('https://ai-server.example.com/chat', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
        model: 'claude-sonnet-4-20250514',
        messages: [{ role: 'user', content: 'What channels have the highest revenue?' }],
        session_salt: `${userId}-${conversationId}`,
        execute_tools: true,
        tools: ['*'],
    }),
});

// Process SSE stream
const reader = response.body.getReader();
const decoder = new TextDecoder();

while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    const text = decoder.decode(value);
    for (const line of text.split('\n')) {
        if (!line.startsWith('data: ')) continue;
        const event = JSON.parse(line.slice(6));

        switch (event.type) {
            case 'token_map':
                // Store for obfuscation inspector UI
                console.log(`Loaded ${event.data.entity_count} entity tokens`);
                break;
            case 'obfuscation_event':
                // Show in transparency/debug panel
                console.log(`[${event.data.stage}] ${event.data.before} -> ${event.data.after}`);
                break;
            case 'text_delta':
                // Already deobfuscated -- safe to display to user
                process.stdout.write(event.data.text);
                break;
            case 'tool_result':
                // Contains REAL data (not obfuscated)
                console.log(`Tool ${event.data.name}: ${event.data.result}`);
                break;
            case 'done':
                console.log(`\nTokens: ${event.data.usage.total_tokens}`);
                break;
        }
    }
}
```

### Invalidating the cache

```typescript
// Call when entity data changes
async function invalidateObfuscationCache(sessionSalt: string) {
    await fetch('https://ai-server.example.com/obfuscation/invalidate', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
            'Authorization': `Bearer ${process.env.OBFUSCATION_SOURCE_TOKEN}`,
        },
        body: JSON.stringify({ session_salt: sessionSalt }),
    });
}
```

## How Tokens Work

Tokens are deterministic within a session (same entity always gets the same token) but different across sessions (different salts produce different tokens).

**Format**: `{entity_type}_{hex_suffix}`

Examples:
- `channel_a1b2` -- A channel entity
- `bidder_c3d4` -- A bidder entity
- `advertiser_e5f6` -- An advertiser entity

The suffix is derived from HMAC-SHA256 of the entity type + ID, keyed with the `OBFUSCATION_KEY` and salted with the `session_salt`. This means:
- Same entity + same salt = same token (consistent within a conversation)
- Same entity + different salt = different token (no cross-session correlation)
- The LLM sees meaningful type prefixes (`channel_`, `bidder_`) so it understands the semantic category

## Security Considerations

- **OBFUSCATION_KEY**: Keep this secret. It determines the HMAC-to-token mapping. Changing it invalidates all existing sessions.
- **OBFUSCATION_SOURCE_TOKEN**: Shared secret between your app and the server. Use a strong random value. Rotate periodically.
- **Session salts**: Don't use predictable values (like sequential integers). UUIDs or `{userId}-{conversationId}` patterns work well.
- **Source endpoint**: Should only be accessible to the AI server. Consider network-level restrictions in addition to bearer token auth.
- **Entity coverage**: Any entity not in your dictionary will pass through unobfuscated. Ensure your source endpoint returns a complete list.
