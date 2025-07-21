# Configuration Reference Guide

## Overview

This guide explains all configuration files used by Meri Bot Rust, their purposes, and how to set them up properly.

## 📁 Required Configuration Files

### `botconfig.txt` - Bot Core Configuration
**Purpose**: Core bot settings and Discord token
**Required**: Yes
**Security**: Contains sensitive Discord token

```txt
DISCORD_TOKEN=your_actual_discord_token_here
PREFIX=^
RUST_LOG=trace
```

**Settings**:
- `DISCORD_TOKEN`: Your Discord bot token (required)
- `PREFIX`: Command prefix (default: `^`)
- `RUST_LOG`: Logging level (recommended: `trace`)

### `lmapiconf.txt` - AI Model Configuration
**Purpose**: LM Studio/Ollama API settings
**Required**: For AI commands (^lm, ^reason, ^sum)
**Security**: Contains model names and API settings

```txt
LM_STUDIO_BASE_URL=http://127.0.0.1:11434
LM_STUDIO_TIMEOUT=30
DEFAULT_MODEL=your-chat-model-name
DEFAULT_REASON_MODEL=qwen2.5:4b
DEFAULT_TEMPERATURE=0.8
DEFAULT_MAX_TOKENS=8192
MAX_DISCORD_MESSAGE_LENGTH=2000
RESPONSE_FORMAT_PADDING=50
```

**Settings**:
- `LM_STUDIO_BASE_URL`: AI server URL (Ollama: `http://127.0.0.1:11434`)
- `LM_STUDIO_TIMEOUT`: Request timeout in seconds
- `DEFAULT_MODEL`: Your chat model name
- `DEFAULT_REASON_MODEL`: Reasoning model name
- `DEFAULT_TEMPERATURE`: AI creativity (0.0-1.0)
- `DEFAULT_MAX_TOKENS`: Maximum response length
- `MAX_DISCORD_MESSAGE_LENGTH`: Discord message limit
- `RESPONSE_FORMAT_PADDING`: Buffer for formatting

### `system_prompt.txt` - AI Personality
**Purpose**: Defines AI behavior and personality
**Required**: For AI commands
**Security**: May contain custom prompts

```txt
You are Meri, a helpful AI assistant. You are knowledgeable, friendly, and always try to provide accurate and helpful responses.
```

## 🔧 Optional Configuration Files

### `serpapi.txt` - Web Search API
**Purpose**: SerpAPI key for enhanced web search
**Required**: For reasoning-enhanced search (^reason -s)
**Security**: Contains API key

```txt
your_serpapi_key_here
```

### `reasoning_prompt.txt` - Reasoning Instructions
**Purpose**: Specialized prompt for reasoning tasks
**Required**: No (falls back to system_prompt.txt)
**Security**: May contain custom prompts

```txt
You are an expert at logical reasoning and step-by-step analysis. Think through problems carefully and explain your reasoning process.
```

### `reasoning_search_analysis_prompt.txt` - Search Analysis
**Purpose**: Prompt for analyzing search results
**Required**: No (uses reasoning_prompt.txt if not found)

### `refine_search_prompt.txt` - Query Refinement
**Purpose**: Prompt for improving search queries
**Required**: No (uses system_prompt.txt if not found)

### `summarize_search_prompt.txt` - Search Summarization
**Purpose**: Prompt for summarizing search results
**Required**: No (uses system_prompt.txt if not found)

### `youtube_prompt_generation_prompt.txt` - YouTube Analysis
**Purpose**: Prompt for generating YouTube analysis
**Required**: No (uses system_prompt.txt if not found)

### `youtube_summarization_prompt.txt` - YouTube Summarization
**Purpose**: Prompt for YouTube video summarization
**Required**: No (uses system_prompt.txt if not found)

## 🛡️ Security Considerations

### Sensitive Files (Never Commit)
- `botconfig.txt` - Contains Discord token
- `lmapiconf.txt` - Contains model names and API settings
- `serpapi.txt` - Contains SerpAPI API key
- `system_prompt.txt` - May contain custom prompts
- `reasoning_prompt.txt` - May contain custom prompts

### Safe to Commit
- All `example_*.txt` files
- Documentation files
- Source code files

## 📋 Setup Checklist

### Basic Bot Setup
- [ ] Create `botconfig.txt` with Discord token
- [ ] Test bot connectivity with `^ping`

### AI Chat Setup
- [ ] Create `lmapiconf.txt` with AI server settings
- [ ] Create `system_prompt.txt` with AI personality
- [ ] Test AI chat with `^lm Hello!`

### Reasoning Setup
- [ ] Ensure `DEFAULT_REASON_MODEL` is set in `lmapiconf.txt`
- [ ] Create `reasoning_prompt.txt` (optional)
- [ ] Test reasoning with `^reason Why is the sky blue?`

### Web Search Setup
- [ ] Create `serpapi.txt` with API key (optional)
- [ ] Test search with `^lm -s test query`

### YouTube Setup
- [ ] Install yt-dlp
- [ ] Test YouTube summarization with `^sum https://youtube.com/...`

## 🔍 Troubleshooting

### Configuration Loading Issues
```bash
# Check if files exist
ls -la *.txt

# Verify file permissions
chmod 644 *.txt

# Check file encoding (should be UTF-8)
file *.txt
```

### Common Errors
- **"No such file or directory"**: File doesn't exist
- **"Missing required setting"**: Required field is empty
- **"Invalid URL format"**: URL is malformed
- **"Configuration error"**: File format is incorrect

### Validation Commands
```bash
# Test bot configuration
grep "DISCORD_TOKEN" botconfig.txt

# Test AI configuration
grep "DEFAULT_MODEL" lmapiconf.txt

# Test SerpAPI configuration
cat serpapi.txt
```

## 📚 Example Files

All example files are provided as templates:
- `example_botconfig.txt`
- `example_lmapiconf.txt`
- `example_system_prompt.txt`
- `example_reasoning_prompt.txt`
- And more...

Copy these files and customize them for your setup.

## 🆘 Getting Help

If you have configuration issues:
1. Check the log file for detailed error messages
2. Verify all required files exist and are properly formatted
3. Ensure sensitive files are not committed to version control
4. Test each feature individually to isolate issues
5. Review the main README.md for detailed setup instructions 