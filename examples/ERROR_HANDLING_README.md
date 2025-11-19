# Error Handling Example

This example demonstrates comprehensive error handling patterns for the cnctd_ai library.

## What It Tests

### Invalid API Key
- Tests authentication failures with both providers
- Shows how errors are surfaced when credentials are invalid
- Demonstrates `Error::AuthenticationFailed` or `Error::ProviderError` variants

### Invalid Model Name
- Tests provider responses when requesting non-existent models
- Shows provider-specific error messages
- Helps understand model availability checking

### Empty Messages
- Tests validation of message arrays
- Shows how providers handle empty conversation history
- Demonstrates client-side or server-side validation

### Invalid Request Options
- Tests boundary conditions like `max_tokens: 0`
- Shows parameter validation errors
- Helps understand acceptable ranges for options

### Malformed Tool Schemas
- Tests invalid JSON schemas in tool definitions
- Shows schema validation errors
- Demonstrates proper tool schema requirements

### Error Recovery Patterns

#### Graceful Degradation
- Try primary provider (Anthropic)
- Automatically fallback to secondary provider (OpenAI) on failure
- Real-world pattern for high availability

#### Retry with Exponential Backoff
- Identifies retryable errors (rate limits, network errors, 5xx status codes)
- Implements exponential backoff (1s, 2s, 4s, etc.)
- Demonstrates production-ready retry logic
- Gives up after max attempts

## Running the Example

```bash
# Set up environment variables
export ANTHROPIC_API_KEY=your_key_here
export OPENAI_API_KEY=your_key_here

# Run the example
cargo run --example error_handling
```

## Expected Output

The example will:
1. Test various error conditions with both providers
2. Print detailed error information including:
   - Error type (enum variant)
   - Provider-specific error messages
   - HTTP status codes where applicable
3. Demonstrate graceful degradation between providers
4. Show retry logic with exponential backoff

## What You'll Learn

### Error Types
- `Error::AuthenticationFailed` - Invalid API keys
- `Error::ProviderError` - Provider-specific errors with status codes
- `Error::InvalidRequest` - Client-side validation failures
- `Error::RateLimited` - Rate limit exceeded
- `Error::NetworkError` - Connection issues
- `Error::OpenAiError` - OpenAI SDK errors
- `Error::AnthropicError` - Anthropic SDK errors

### Error Handling Patterns
- Pattern matching on error variants
- Extracting detailed error information
- Deciding which errors are retryable
- Implementing fallback strategies
- Using exponential backoff for retries

### Production Considerations
- Always handle authentication errors gracefully
- Implement retry logic for transient failures
- Consider fallback providers for critical paths
- Log detailed error information for debugging
- Set reasonable retry limits to avoid infinite loops

## Notes

- Some tests intentionally create errors to demonstrate error handling
- Tests that require API keys will be skipped if keys aren't set
- Provider-specific error messages may vary over time as APIs evolve
- Rate limit testing is excluded to avoid actually hitting rate limits
