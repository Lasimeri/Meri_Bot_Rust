# Meri Bot Rust - Logging and Configuration

This document covers the logging system and configuration options for Meri Bot Rust.

## Logging Configuration

The bot uses the `log` and `env_logger` crates for structured logging. You can control the logging level through the `RUST_LOG` environment variable in your `botconfig.txt` file:

```
RUST_LOG=error  # Only show errors (default)
RUST_LOG=warn   # Show warnings and errors
RUST_LOG=info   # Show info, warnings, and errors
RUST_LOG=debug  # Show all logs including debug
```

## Configuration Files

### Main Configuration (`botconfig.txt`)

Basic bot configuration:
```
DISCORD_TOKEN=your_bot_token_here
PREFIX=^
RUST_LOG=debug
```

### LM Studio Configuration (`lmapiconf.txt`)

**Both the `lm` and `sum` commands now use `lmapiconf.txt`** for LM Studio API configuration:

```
# LM Studio API Configuration
LM_STUDIO_BASE_URL=http://127.0.0.1:1234
LM_STUDIO_TIMEOUT=120
DEFAULT_MODEL=qwen/qwen3-4b
DEFAULT_REASON_MODEL=qwen/qwen3-4b
DEFAULT_TEMPERATURE=0.3
DEFAULT_MAX_TOKENS=1000
MAX_DISCORD_MESSAGE_LENGTH=2000
RESPONSE_FORMAT_PADDING=100
```

## Sum Command Configuration

The `sum` command now uses the same configuration system as the `lm` command:

### System Prompts

The sum command loads system prompts from text files using multi-path fallback:

**For General Summarization:**
- `summarization_prompt.txt` (preferred)
- `example_summarization_prompt.txt` (fallback)
- Built-in fallback if no file found

**For YouTube Summarization:**
- `youtube_summarization_prompt.txt` (preferred)
- `example_youtube_summarization_prompt.txt` (fallback)
- Built-in fallback if no file found

### How It Works

1. **Configuration Loading**: Uses `load_lm_config()` from the search module to load `lmapiconf.txt`
2. **Model Selection**: Uses the `DEFAULT_REASON_MODEL` from the config for summarization
3. **System Prompts**: Loads appropriate prompts from text files based on content type
4. **Streaming**: Uses the same SSE streaming approach as the lm command
5. **Message Limits**: Respects `MAX_DISCORD_MESSAGE_LENGTH` and `RESPONSE_FORMAT_PADDING` from config

### Example Usage

```bash
# General webpage summarization
^sum https://example.com/article

# YouTube video summarization
^sum https://youtube.com/watch?v=video_id
```

## YouTube Subtitle Extraction

The sum command requires `yt-dlp` to be installed for YouTube video summarization:

```bash
# Install yt-dlp
pip install yt-dlp

# Or update existing installation
yt-dlp -U
```

### Common Issues

1. **"Did not get any data blocks"** - Update yt-dlp to the latest version
2. **"Sign in to confirm you're not a bot"** - YouTube temporary restriction, try again later
3. **"No automatic captions"** - Video doesn't have subtitles/captions available

## Logging Output

With `RUST_LOG=debug`, the sum command provides detailed logging:

```
📺 Sum command initiated by user...
🔗 Received URL: https://...
🔧 Loading LM configuration from lmapiconf.txt...
✅ LM configuration loaded successfully
🧠 Using reasoning model: qwen/qwen3-4b
🌐 API endpoint: http://127.0.0.1:1234
🎯 Processing YouTube URL: https://...
🎥 Attempting to fetch YouTube transcript using yt-dlp...
🔍 Checking yt-dlp version...
✅ yt-dlp version: 2025.06.30
🔄 Running yt-dlp command for automatic subtitles...
📄 Looking for subtitle file: yt_transcript_...
📖 Read subtitle file: 15847 characters
🧹 VTT content cleaned: 2458 characters
✅ YouTube transcript fetched successfully
📄 Summarization prompt loaded from: example_youtube_summarization_prompt.txt
🤖 Preparing AI request...
🧠 Using model: qwen/qwen3-4b
📡 Sending streaming request to LM Studio...
📡 LM Studio API Response Status: 200
🔄 Starting to process SSE stream...
🏁 Streaming completed
✅ Summary streaming completed successfully
```

This provides full visibility into the entire process from configuration loading to final summary delivery. 