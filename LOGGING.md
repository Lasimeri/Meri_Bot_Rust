# Enhanced Logging System Documentation

## Overview

The Meri Bot Rust includes a comprehensive logging system that provides complete visibility into bot operations. Every command execution, processing step, and error condition is logged with detailed context, making debugging and monitoring straightforward and effective.

## 🎯 Key Features

### 📊 Comprehensive Coverage
- **Every command execution** is logged with unique UUIDs for tracking
- **Phase-based logging** with clear step indicators
- **Performance metrics** including character counts and processing times
- **Error diagnosis** with detailed context and recovery suggestions
- **User experience tracking** for command usage patterns
- **Real-time monitoring** during streaming operations

### 🔍 Log Levels
- **TRACE** - Most detailed level, shows every step and data transformation
- **DEBUG** - Detailed debugging information for development
- **INFO** - General operational information
- **WARN** - Warning conditions that don't stop operation
- **ERROR** - Error conditions that may affect functionality

### 📁 Log Storage
- All logs are saved to `log.txt` in the project root
- Persistent storage across bot restarts
- Automatic log rotation and management
- Human-readable timestamps and formatting

## 🚀 Logging Configuration

### Environment Setup
The logging system is configured via the `RUST_LOG` environment variable in `botconfig.txt`:

```
RUST_LOG=info
```

**Recommended Settings:**
- `info` - Standard operational logging (default)
- `debug` - Detailed logging for production monitoring
- `trace` - Full visibility for development and debugging (can be enabled in `src/main.rs`)
- `warn` - Only warnings and errors
- `error` - Only error conditions

### Log File Location
- **Primary**: `log.txt` in the project root directory
- **Automatic Creation**: Log file is created automatically on first run
- **Persistent**: Logs are preserved across bot restarts

### Enabling Trace Logging
To enable trace logging for detailed debugging:

1. **Edit `src/main.rs`** (around line 810):
   ```rust
   // Change from:
   std::env::set_var("RUST_LOG", "info");
   
   // To:
   std::env::set_var("RUST_LOG", "trace");
   ```

2. **Recompile and restart** the bot
3. **Check `log.txt`** for detailed trace-level logs
4. **Remember to disable** trace logging in production for performance

## 📋 Command-Specific Logging

### 🎯 User ID Mention Logging
The primary interaction method includes comprehensive logging:

```rust
// Example log entries for user mentions
[2024-01-15T10:30:45Z INFO  main] Bot mentioned via user ID - Raw message content: '<@1385309017881968761> Hello!'
[2024-01-15T10:30:45Z INFO  main] Direct user ID mention without reply from user Alice
[2024-01-15T10:30:45Z INFO  main] Prompt: 'Hello!'
[2024-01-15T10:30:45Z INFO  main] Processing user ID mention RAG request in reply
[2024-01-15T10:30:45Z INFO  main] User Bob asking about message from Alice
```

### 🤖 AI Chat Command (`^lm`)
Comprehensive logging for the AI chat functionality:

```rust
// Configuration loading
[2024-01-15T10:30:45Z INFO  commands::lm] Loading LM configuration from lmapiconf.txt
[2024-01-15T10:30:45Z INFO  commands::lm] Using model: qwen3:4b
[2024-01-15T10:30:45Z INFO  commands::lm] Base URL: http://127.0.0.1:11434

// Request processing
[2024-01-15T10:30:45Z INFO  commands::lm] Processing LM request for user Alice (ID: 123456789)
[2024-01-15T10:30:45Z INFO  commands::lm] Input length: 45 characters
[2024-01-15T10:30:45Z INFO  commands::lm] Context messages: 3 user, 2 assistant

// Streaming response
[2024-01-15T10:30:46Z INFO  commands::lm] Starting streaming response
[2024-01-15T10:30:46Z INFO  commands::lm] Received chunk: 128 characters
[2024-01-15T10:30:46Z INFO  commands::lm] Updated Discord message (Part 1)
[2024-01-15T10:30:47Z INFO  commands::lm] Finalizing response: 2048 total characters, 2 messages sent
```

### 🧠 AI Reasoning Command (`^reason`)
Enhanced logging for reasoning operations with thinking tag filtering:

```rust
// Configuration and setup
[2024-01-15T10:30:45Z INFO  commands::reason] Loading reasoning configuration
[2024-01-15T10:30:45Z INFO  commands::reason] Using reasoning model: qwen3:4b
[2024-01-15T10:30:45Z INFO  commands::reason] Loading reasoning system prompt from reasoning_prompt.txt

// Request processing
[2024-01-15T10:30:45Z INFO  commands::reason] Processing reasoning request: "Why is the sky blue?"
[2024-01-15T10:30:45Z INFO  commands::reason] Input length: 23 characters

// Thinking tag filtering
[2024-01-15T10:30:46Z INFO  commands::reason] Filtering thinking tags from response
[2024-01-15T10:30:46Z INFO  commands::reason] Removed 156 characters of thinking content
[2024-01-15T10:30:46Z INFO  commands::reason] Final response: 892 characters (filtered from 1048)

// Streaming statistics
[2024-01-15T10:30:47Z INFO  commands::reason] Reasoning completed: 892 characters, 1 message, 156 chars filtered
```

### 🖼️ Vision Analysis Module (`vis.rs`)
Comprehensive logging for vision analysis and image processing:

```rust
// Image attachment processing
[2024-01-15T10:30:45Z INFO  commands::vis] Processing attachment: image.png (image/png)
[2024-01-15T10:30:45Z INFO  commands::vis] Downloaded 2048 bytes
[2024-01-15T10:30:45Z INFO  commands::vis] Final processing complete - 2048 bytes encoded to base64

// GIF processing
[2024-01-15T10:30:45Z INFO  commands::vis] Detected GIF file, processing for vision compatibility...
[2024-01-15T10:30:45Z INFO  commands::vis] Successfully loaded GIF image
[2024-01-15T10:30:45Z INFO  commands::vis] Successfully processed GIF - converted to image/png
[2024-01-15T10:30:45Z INFO  commands::vis] Final processing complete - 1536 bytes encoded to base64

// Vision request processing
[2024-01-15T10:30:45Z INFO  commands::vis] Processing vision request: "What's in this image?"
[2024-01-15T10:30:45Z INFO  commands::vis] Image size: 2048 bytes, format: image/png
[2024-01-15T10:30:45Z INFO  commands::vis] Creating vision message with multimodal content
[2024-01-15T10:30:46Z INFO  commands::vis] Starting vision model streaming response
[2024-01-15T10:30:47Z INFO  commands::vis] Vision analysis completed: 1024 characters, 1 message
```

### 📺 Content Summarization Command (`^sum`)
Comprehensive logging for the summarization command with enhanced tracking:

```rust
// Command execution with UUID tracking
[2024-01-15T10:30:45Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Starting sum command
[2024-01-15T10:30:45Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] URL: https://youtube.com/watch?v=dQw4w9WgXcQ
[2024-01-15T10:30:45Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] User: Alice (ID: 123456789)

// YouTube transcript processing
[2024-01-15T10:30:45Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Detected YouTube URL, attempting transcript extraction
[2024-01-15T10:30:45Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Checking yt-dlp version: yt-dlp 2023.12.30
[2024-01-15T10:30:45Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Creating subtitles directory: ./subtitles
[2024-01-15T10:30:46Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Running yt-dlp command: yt-dlp --write-sub --write-auto-sub --skip-download --sub-format vtt
[2024-01-15T10:30:47Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] yt-dlp completed successfully
[2024-01-15T10:30:47Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Found subtitle file: ./subtitles/dQw4w9WgXcQ.en.vtt

// VTT content processing
[2024-01-15T10:30:47Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Reading VTT file: 2048 bytes
[2024-01-15T10:30:47Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Cleaning VTT content: 45 lines processed
[2024-01-15T10:30:47Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Removed 12 timestamp lines, 8 formatting tags
[2024-01-15T10:30:47Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Final content: 1567 characters

// AI summarization
[2024-01-15T10:30:47Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Starting AI summarization
[2024-01-15T10:30:47Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Content length: 1567 characters (within single-pass limit)
[2024-01-15T10:30:47Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Loading summarization prompt from youtube_summarization_prompt.txt
[2024-01-15T10:30:48Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Sending request to AI model
[2024-01-15T10:30:49Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Received streaming response
[2024-01-15T10:30:50Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Processing chunk: 256 characters
[2024-01-15T10:30:51Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Finalizing summary: 1024 characters, 1 message sent

// Final statistics
[2024-01-15T10:30:51Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Sum command completed successfully
[2024-01-15T10:30:51Z INFO  commands::sum] [UUID: 550e8400-e29b-41d4-a716-446655440000] Statistics: 1024 chars output, 1 message, 6.2s total time
```

### 🔍 Web Search Commands
Logging for AI-enhanced and reasoning-enhanced search:

```rust
// AI-enhanced search
[2024-01-15T10:30:45Z INFO  commands::lm] Starting AI-enhanced search: "rust programming tutorial"
[2024-01-15T10:30:45Z INFO  commands::lm] Loading search configuration
[2024-01-15T10:30:45Z INFO  commands::lm] Refining query with AI model
[2024-01-15T10:30:46Z INFO  commands::lm] Refined query: "rust programming language tutorial guide"
[2024-01-15T10:30:46Z INFO  commands::lm] Performing web search with DuckDuckGo
[2024-01-15T10:30:47Z INFO  commands::lm] Found 10 search results
[2024-01-15T10:30:47Z INFO  commands::lm] Generating AI summary with embedded links
[2024-01-15T10:30:48Z INFO  commands::lm] Search completed: 2048 characters, 1 message

// Reasoning-enhanced search
[2024-01-15T10:30:45Z INFO  commands::reason] Starting reasoning-enhanced search: "quantum computing applications"
[2024-01-15T10:30:45Z INFO  commands::reason] Refining query for analytical research
[2024-01-15T10:30:46Z INFO  commands::reason] Refined query: "quantum computing practical applications research analysis"
[2024-01-15T10:30:46Z INFO  commands::reason] Performing SerpAPI search
[2024-01-15T10:30:47Z INFO  commands::reason] Found 8 search results
[2024-01-15T10:30:47Z INFO  commands::reason] Analyzing results with reasoning model
[2024-01-15T10:30:48Z INFO  commands::reason] Filtering thinking tags: removed 234 characters
[2024-01-15T10:30:48Z INFO  commands::reason] Search completed: 1800 characters, 1 message, 234 chars filtered
```

### 🖼️ Profile Picture Command (`^ppfp`)
Logging for profile picture display functionality:

```rust
// Profile picture request
[2024-01-15T10:30:45Z INFO  commands::ping] Processing profile picture request for user: Alice
[2024-01-15T10:30:45Z INFO  commands::ping] User avatar URL: https://cdn.discordapp.com/avatars/123456789/avatar.png
[2024-01-15T10:30:45Z INFO  commands::ping] Downloading avatar image: 1024 bytes
[2024-01-15T10:30:45Z INFO  commands::ping] Creating rich embed with avatar information
[2024-01-15T10:30:45Z INFO  commands::ping] Profile picture displayed successfully
```

## 🛠️ Error Logging

### Configuration Errors
```rust
[2024-01-15T10:30:45Z ERROR commands::lm] Failed to load lmapiconf.txt: No such file or directory
[2024-01-15T10:30:45Z ERROR commands::lm] Configuration error: Missing required setting DEFAULT_MODEL
[2024-01-15T10:30:45Z ERROR commands::lm] Invalid URL format: http://invalid-url:port
```

### Network and API Errors
```rust
[2024-01-15T10:30:45Z ERROR commands::lm] HTTP request failed: reqwest::Error { kind: Connect, source: ... }
[2024-01-15T10:30:45Z ERROR commands::lm] API response error: 404 Not Found
[2024-01-15T10:30:45Z ERROR commands::lm] Stream processing error: Unexpected end of stream
```

### Vision Analysis Errors
```rust
[2024-01-15T10:30:45Z ERROR commands::vis] Failed to download image: HTTP 404 Not Found
[2024-01-15T10:30:45Z ERROR commands::vis] Image processing failed: Unsupported image format
[2024-01-15T10:30:45Z ERROR commands::vis] GIF processing failed: Invalid GIF data
[2024-01-15T10:30:45Z ERROR commands::vis] Base64 encoding failed: Invalid image data
```

### YouTube Processing Errors
```rust
[2024-01-15T10:30:45Z ERROR commands::sum] yt-dlp not found in PATH
[2024-01-15T10:30:45Z ERROR commands::sum] yt-dlp command failed: exit code 1
[2024-01-15T10:30:45Z ERROR commands::sum] No subtitles available for video
[2024-01-15T10:30:45Z ERROR commands::sum] VTT file is empty or corrupted
```

### Discord API Errors
```rust
[2024-01-15T10:30:45Z ERROR commands::lm] Failed to update Discord message: HTTP 429 (Rate Limited)
[2024-01-15T10:30:45Z ERROR commands::lm] Message too long: 2500 characters (limit: 2000)
[2024-01-15T10:30:45Z ERROR commands::lm] Discord API error: Missing Permissions
```

### SerpAPI Errors
```rust
[2024-01-15T10:30:45Z ERROR commands::reason] SerpAPI key not found in serpapi.txt
[2024-01-15T10:30:45Z ERROR commands::reason] SerpAPI request failed: Invalid API key
[2024-01-15T10:30:45Z ERROR commands::reason] SerpAPI rate limit exceeded: Too many requests
```

## 📊 Performance Metrics

### Response Time Tracking
```rust
[2024-01-15T10:30:45Z INFO  commands::lm] Request started at: 2024-01-15T10:30:45.123Z
[2024-01-15T10:30:48Z INFO  commands::lm] Request completed at: 2024-01-15T10:30:48.456Z
[2024-01-15T10:30:48Z INFO  commands::lm] Total processing time: 3.333 seconds
```

### Character and Token Statistics
```rust
[2024-01-15T10:30:48Z INFO  commands::lm] Input statistics: 45 characters, 12 tokens
[2024-01-15T10:30:48Z INFO  commands::lm] Output statistics: 2048 characters, 512 tokens
[2024-01-15T10:30:48Z INFO  commands::lm] Message count: 1 (within Discord limits)
```

### Image Processing Statistics
```rust
[2024-01-15T10:30:48Z INFO  commands::vis] Image processing statistics: 2048 bytes, PNG format
[2024-01-15T10:30:48Z INFO  commands::vis] Base64 encoding: 2731 characters
[2024-01-15T10:30:48Z INFO  commands::vis] Processing time: 0.5 seconds
```

### Streaming Performance
```rust
[2024-01-15T10:30:48Z INFO  commands::lm] Streaming performance: 45.6 characters/second
[2024-01-15T10:30:48Z INFO  commands::lm] Update frequency: 0.8 seconds average
[2024-01-15T10:30:48Z INFO  commands::lm] Buffer efficiency: 98.2% (minimal fragmentation)
```

## 🔧 Log Analysis and Debugging

### Finding Specific Requests
Search for UUIDs to track specific command executions:
```bash
grep "550e8400-e29b-41d4-a716-446655440000" log.txt
```

### Error Analysis
Find all errors in the log:
```bash
grep "ERROR" log.txt
```

### Performance Analysis
Find slow requests:
```bash
grep "Total processing time" log.txt | grep -E "[0-9]+\.[0-9]+ seconds"
```

### User Activity Tracking
Find all interactions from a specific user:
```bash
grep "User: Alice" log.txt
```

### Vision Analysis Tracking
Find all vision-related operations:
```bash
grep "GIF_VISION\|commands::vis" log.txt
```

### Search Operation Tracking
Find all search-related operations:
```bash
grep "search\|SerpAPI" log.txt
```

## 📈 Log File Management

### File Size Monitoring
The log file can grow large over time. Monitor its size:
```bash
ls -lh log.txt
```

### Log Rotation (Manual)
To prevent the log file from becoming too large:
```bash
# Archive current log
mv log.txt log.txt.$(date +%Y%m%d)

# Create new log file
touch log.txt
```

### Log Cleanup
Remove old log files:
```bash
# Remove logs older than 30 days
find . -name "log.txt.*" -mtime +30 -delete
```

## 🎯 Best Practices

### For Development
1. **Use TRACE level** for maximum visibility during development
2. **Search by UUID** to track specific command executions
3. **Monitor error patterns** to identify recurring issues
4. **Check performance metrics** to optimize slow operations
5. **Track vision processing** for image analysis debugging

### For Production
1. **Use DEBUG or INFO level** to balance visibility and performance
2. **Monitor log file size** and implement rotation if needed
3. **Set up log monitoring** to alert on error conditions
4. **Archive logs regularly** for historical analysis
5. **Monitor API usage** for rate limiting and quota management

### For Troubleshooting
1. **Start with ERROR level** to identify critical issues
2. **Use UUID tracking** to follow specific user requests
3. **Check configuration loading** logs for setup issues
4. **Monitor network and API** logs for connectivity problems
5. **Review vision processing** logs for image analysis issues

## 🔍 Log Format Reference

### Standard Log Entry Format
```
[timestamp] [level] [module] [message]
```

### UUID Tracking Format
```
[timestamp] [level] [module] [UUID: uuid] [message]
```

### Performance Metric Format
```
[timestamp] [level] [module] [UUID: uuid] Statistics: [metrics]
```

### Error Format
```
[timestamp] [level] [module] [UUID: uuid] Error: [error_description]
```

### Vision Processing Format
```
[timestamp] [level] [module] [GIF_VISION] [message]
```

## 📚 Related Documentation

- **README.md** - Main project documentation
- **PLANNING.md** - Development roadmap and known issues
- **Configuration Files** - Example templates and setup guides

## 🆘 Getting Help

If you encounter logging issues:

1. **Check log file permissions** - Ensure the bot can write to `log.txt`
2. **Verify RUST_LOG setting** - Make sure it's set in `botconfig.txt`
3. **Check disk space** - Ensure there's room for log file growth
4. **Review recent changes** - New features may add logging requirements
5. **Search existing logs** - Look for similar issues in the log history
6. **Check vision dependencies** - Ensure image processing libraries are available
7. **Verify API keys** - Check SerpAPI and other service configurations

The enhanced logging system provides complete visibility into bot operations, making it easy to diagnose issues, monitor performance, and understand user interactions across all features including the new vision analysis capabilities. 