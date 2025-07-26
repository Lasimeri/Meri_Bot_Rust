# Meri Bot Rust

A powerful Discord bot written in Rust using the Serenity framework, featuring real-time AI chat streaming, advanced reasoning capabilities, comprehensive content summarization, enhanced logging for complete visibility into bot operations, and multimodal vision analysis.

## ⚠️ IMPORTANT: Bot Interaction Method

**This bot ONLY responds to direct user ID mentions!**

- **Primary Method**: `<@Meri_> <your prompt>` 
- **Vision Analysis**: `<@Meri_> -v <prompt>` (with image attached)
- **Reply Support**: Reply to any message with `<@Meri_> <question>` to ask about that specific message
- **Legacy Commands**: Traditional prefix commands (^lm, ^reason, ^sum) are still available for backward compatibility

## ⚠️ Security Notice

**NEVER commit sensitive configuration to version control!**

- Your Discord token is like a password - keep it secret
- Model names and prompts may contain proprietary or sensitive information
- The `.gitignore` file is configured to prevent accidental uploads of:
  - `botconfig.txt` (contains Discord token)
  - `lmapiconf.txt` (contains model names and API settings)
  - `serpapi.txt` (contains SerpAPI key for web search)
  - `system_prompt.txt` and `reasoning_prompt.txt` (may contain custom prompts)
- Use the example files as templates and customize your actual configuration files
- If you accidentally commit sensitive data, regenerate tokens and review what was exposed

## 🆕 Enhanced Logging System

**Complete visibility into bot operations with comprehensive logging:**

- **📊 Detailed Logging**: Every command execution is logged with unique UUIDs for tracking
- **🔍 Phase-Based Logging**: Each step of command processing is logged with clear phase indicators
- **📈 Performance Metrics**: Character counts, processing times, and success rates are tracked
- **🛠️ Error Diagnosis**: Detailed error logging with context and recovery suggestions
- **📝 User Experience Tracking**: Logs user interactions, command usage patterns, and response quality
- **🔄 Real-Time Monitoring**: Live logging during streaming operations with progress updates
- **📁 Log File**: All logs are saved to `log.txt` for persistent debugging and analysis
- **⚙️ Configurable Log Levels**: Trace logging can be enabled by changing `RUST_LOG` from "info" to "trace" in `src/main.rs`

## 🔧 Admin Commands (Owner Only)

The bot includes administrative commands that can only be used by the bot owner:

### `^restart` / `^reboot` / `^restartbot`
- **Owner Only**: Restarts the bot gracefully
- **Function**: Saves all conversation contexts, shuts down cleanly, and restarts the bot process
- **Usage**: `^restart`

### `^shutdown` / `^stopbot`
- **Owner Only**: Shuts down the bot gracefully
- **Function**: Saves all conversation contexts and exits the bot process
- **Usage**: `^shutdown`

### `^adminhelp` / `^ahelp`
- **Owner Only**: Shows help for admin commands
- **Function**: Lists all available administrative commands
- **Usage**: `^adminhelp`

### Configuration
To use admin commands, add your Discord user ID to `botconfig.txt`:
```
BOT_OWNER_ID=YOUR_DISCORD_USER_ID_HERE
```

**Note**: If `BOT_OWNER_ID` is not set, the bot will fall back to using `BOT_USER_ID` as the owner.

## 🤖 AI Commands

### 🆕 Primary AI Chat (User ID Mention Only)
- `<@Meri_> <prompt>` - AI chat with real-time streaming and **per-user conversation memory**
  - **Features**: **Real-time streaming responses**, smart message chunking, extended output length (8K tokens), live progress indicators, multi-part message support, robust buffered streaming for improved reliability, **5-minute timeout for complex reasoning**
- `<@Meri_> <prompt>` + **attachments** - RAG-enhanced analysis of documents (PDF, TXT, images, etc.)
  - **Supported file types**: PDF, TXT, MD, CSV, HTML, JSON, XML, JPG, PNG, GIF, WebP
  - **RAG Features**: Document content extraction, context-aware analysis, multimodal support
- `<@Meri_> -v <prompt>` + **image** - Vision analysis with AI (analyze images with custom prompts)
  - **Features**: Advanced image analysis, GIF support (first frame extraction), context-aware prompts
- **Reply Support**: Reply to any message with `<@Meri_> <question>` to ask about that specific message
  - **Features**: RAG-enhanced context, author identification, smart conversation threading
- **Vision in Replies**: Reply to messages with images using `<@Meri_> -v <prompt>` to analyze the image
  - **Features**: Cross-message image analysis, contextual understanding, attachment detection

## Quick Start Examples

### Basic AI Chat
```
<@Meri_> Hello! How are you?
<@Meri_> Explain quantum computing
<@Meri_> Write a Python function to reverse a string
```

### Vision Analysis
```
<@Meri_> -v What's in this image?
<@Meri_> -v Analyze this diagram and explain the workflow
<@Meri_> -v What text do you see in this screenshot?
```

### Document Analysis (with file attachments)
```
<@Meri_> Summarize this PDF document
<@Meri_> What are the key points in this text file?
<@Meri_> Analyze this CSV data
```

### Reply-Based Context
```
# Reply to any message with:
<@Meri_> What does this mean?
<@Meri_> Can you explain this further?
<@Meri_> -v What's happening in this image?
```

### 📋 Basic Commands (Legacy)
- `^ping` - Test bot response with typing indicator
- `^echo <text>` - Repeat your message
- `^help` - Show comprehensive command list with categories

### 🖼️ Profile Picture Commands (Legacy)
- `^ppfp @user` - Show user's profile picture in a rich embed
  - **Aliases**: `^avatar`, `^pfp`, `^profilepic`
  - **Features**: High-quality embeds, clickable links, memory-efficient processing

### 🤖 Legacy AI Chat Commands
- `^lm <prompt>` - Chat with AI via LM Studio/Ollama
  - **Aliases**: `^llm`, `^ai`, `^chat` 
  - **Features**: **Real-time streaming responses**, smart message chunking, extended output length (8K tokens), live progress indicators, multi-part message support, robust buffered streaming for improved reliability, **5-minute timeout for complex reasoning**
- `^lm --seed <number> <prompt>` - Reproducible AI responses with specific seed
  - **Features**: **Deterministic responses** for testing and debugging, same output for same input, no conversation history (ensures reproducibility), **5-minute timeout for complex reasoning**
- `^lm -v <prompt>` + **image** - Vision analysis (analyze attached images)
  - **Features**: Advanced image analysis, GIF support, attachment detection in replies
- `^lm -s <search query>` - AI-enhanced web search with intelligent query optimization and result summarization
  - **Aliases**: `^lm --search <query>`
  - **Features**: **AI query refinement**, **intelligent summarization with embedded links**, real-time progress updates, fallback to basic search, **5-minute timeout for complex reasoning**
- `^reason <question>` - Deep reasoning with specialized AI model
  - **Aliases**: `^reasoning`
  - **Features**: **Real-time streaming with thinking tag filtering**, step-by-step reasoning, dedicated reasoning model (Qwen3 4B), automatic `<think>` content removal, logical explanations, robust buffered streaming for improved reliability, **5-minute timeout for complex reasoning**
- `^reason -s <search query>` - Reasoning-enhanced web search with analytical insights
  - **Aliases**: `^reasoning -s`, `^reasoning --search`
  - **Features**: **Analytical research synthesis**, reasoning-focused query optimization, embedded source links, specialized reasoning model analysis (Qwen3 4B), **buffered chunking** (posts content in 2000-character chunks), **5-minute timeout for complex reasoning**

### 📺 Content Summarization Commands (Legacy)
- `^sum <url>` - Summarize webpage content or YouTube videos using AI reasoning model
  - **Aliases**: `^summarize`, `^webpage`
  - **Features**: 
    - **YouTube transcript extraction** with yt-dlp (automatic subtitle download)
    - **🆕 Intelligent caching** - Subtitles are cached by URL hash to avoid re-downloading
    - **HTML content extraction** and intelligent cleaning
    - **RAG (map-reduce) summarization** for long content (chunks content >8K chars)
    - **Automatic reasoning tag filtering** (removes `<think>` sections from responses)
    - **5-minute timeout** for reliable processing of complex content
    - **Streaming responses** with progress updates
    - **Smart message chunking** for long summaries
    - **Enhanced logging** with detailed step-by-step tracking and error diagnosis
  - **Examples**: `^sum https://youtube.com/watch?v=...`, `^sum https://example.com`
  - **Requirements**: yt-dlp installed for YouTube support

### 📊 Content Ranking Commands
- `^rank <url>` - Rank and analyze content using Qwen3 reranking model (qwen3-reranker-4b)
  - **Aliases**: `^analyze`, `^evaluate`
  - **Features**:
    - **Multi-dimensional ranking** across 5 key factors (Content Quality, Relevance, Engagement, Educational Value, Technical Excellence)
    - **1-10 scale scoring** with detailed explanations for each factor
    - **YouTube and webpage support** with specialized analysis for each content type
    - **Customizable system prompts** via `rank_system_prompt.txt`
    - **RAG processing** for comprehensive content analysis
    - **Streaming responses** with real-time ranking updates
    - **Actionable feedback** with strengths and improvement suggestions
  - **Examples**: `^rank https://youtube.com/watch?v=...`, `^rank https://example.com`
  - **Requirements**: yt-dlp installed for YouTube support

### 🔍 Web Search Commands
- `^lm -s <search query>` - AI-enhanced web search with intelligent processing
  - **AI Mode (with LM Studio)**: Query refinement → web search → AI summarization with embedded links
  - **Basic Mode (fallback)**: Direct DuckDuckGo search with formatted results
  - **Features**: Real-time progress updates, smart query optimization, comprehensive summaries with embedded source links
  - **Examples**: `^lm -s rust programming`, `^lm -s "discord bot tutorial"`

### 💡 User Experience
- **Typing indicators** on all commands for immediate feedback
- **Real-time streaming** - Watch AI responses appear live as they're generated
- **Smart message chunking** - Automatically splits long responses across multiple Discord messages
- **Error handling** with helpful guidance messages
- **Configuration validation** with clear error messages
- **Progress tracking** - Live character counts and generation status

### ⚡ Streaming Technology
- **Robust Connection Handling** - Uses a **5-minute timeout** to prevent hanging requests while ensuring complete responses from AI models for complex reasoning tasks
- **Buffered Stream Processing** - Assembles incoming data into a line buffer before parsing. This prevents errors caused by data packets being split across network chunks, making the stream processing significantly more reliable.
- **Live Discord Message Editing** - Messages update in real-time every 0.8 seconds with the latest content from the stream.
- **Thinking Tag Filtering** - Automatically removes `<think>...</think>` content from reasoning responses in real-time.
- **YouTube Transcript Processing** - Automatic subtitle extraction using yt-dlp with intelligent VTT cleaning and RAG summarization for long content.
- **Graceful Error Handling** - If a Discord message fails to update mid-stream, the entire operation is safely halted to prevent content loss, and the error is logged.

## Prerequisites

- Rust (latest stable version)
- A Discord bot token
- LM Studio or Ollama (for AI chat and reasoning functionality) - optional
- yt-dlp (for YouTube transcript extraction) - optional, required for YouTube summarization
- Internet connection (for web search functionality)
- SerpAPI key (for enhanced web search functionality) - optional

## Setup

1. **Create a Discord Application and Bot**
   - Go to [Discord Developer Portal](https://discord.com/developers/applications)
   - Create a new application
   - Go to the "Bot" section
   - Create a bot and copy the token

2. **Clone the repository**
   ```bash
   git clone <your-repo-url>
   cd meri_bot_rust
   ```

3. **Set up Discord Bot Configuration**
   
   Create a `botconfig.txt` file in the project root:
   ```bash
   # Copy from the example and customize
   cp example_botconfig.txt botconfig.txt
   ```
   
   Edit `botconfig.txt` with your settings:
   ```
   DISCORD_TOKEN=your_actual_discord_token_here
   PREFIX=^
   RUST_LOG=trace
   ```
   
   **Note:** The PREFIX can be customized to any character(s) you prefer

4. **Set up AI Chat (Optional - for LM Studio/Ollama functionality)**
   
   Create configuration files in the project root:
   ```bash
   # Copy configuration files from examples and customize  
   cp example_lmapiconf.txt lmapiconf.txt
   cp example_system_prompt.txt system_prompt.txt
   cp example_reasoning_prompt.txt reasoning_prompt.txt  # Optional for reasoning command
   cp example_rank_system_prompt.txt rank_system_prompt.txt  # Optional for ranking command
   ```
   
   Edit `lmapiconf.txt` with your AI server settings:
   ```
   LM_STUDIO_BASE_URL=http://127.0.0.1:11434  # Ollama default
   LM_STUDIO_TIMEOUT=30
   DEFAULT_MODEL=your-chat-model-name
   DEFAULT_REASON_MODEL=qwen2.5:4b  # Qwen3 4B reasoning model
   DEFAULT_TEMPERATURE=0.8
   DEFAULT_MAX_TOKENS=8192                    # Extended for longer responses
   DEFAULT_SEED=                              # Optional: seed for reproducible responses
   MAX_DISCORD_MESSAGE_LENGTH=2000           # Discord's limit
   RESPONSE_FORMAT_PADDING=50                # Buffer for formatting
   ```
   
   **Important:** 
   - All settings are mandatory - no defaults provided. See `example_lmapiconf.txt` for guidance.
   - Replace `your-chat-model-name` and `your-reasoning-model-name` with your actual model names.
   - For reasoning tasks, consider using models optimized for logical analysis (e.g., qwen3:4b, qwen3:8b, etc.).
   - The `system_prompt.txt` configures the AI's behavior and personality for chat interactions.
   - The `rank_system_prompt.txt` configures the Qwen3 reranking model's analysis criteria and output format.

5. **Set up SerpAPI (Optional - for enhanced web search)**
   
   Create a `serpapi.txt` file in the project root:
   ```
   your_serpapi_key_here
   ```
   
   **Note:** Get your SerpAPI key from [SerpAPI](https://serpapi.com/). This enables enhanced web search functionality with reasoning-enhanced search.

6. **Build and run**
   ```bash
   cargo build --release
   cargo run
   ```

## YouTube Support Setup (Optional)

For YouTube video summarization, install yt-dlp:

### Windows
```powershell
# Using winget
winget install yt-dlp

# Or using pip
pip install yt-dlp

# Or download from https://github.com/yt-dlp/yt-dlp/releases
```

### macOS
```bash
# Using Homebrew
brew install yt-dlp

# Or using pip
pip install yt-dlp
```

### Linux
```bash
# Using package manager
sudo apt install yt-dlp  # Ubuntu/Debian
sudo dnf install yt-dlp  # Fedora

# Or using pip
pip install yt-dlp
```

### Verify Installation
```bash
yt-dlp --version
```

**Note**: The `^sum` command will automatically detect if yt-dlp is available and provide helpful error messages if it's not installed.

## 🆕 YouTube Subtitle Caching System

The bot now includes an intelligent caching system for YouTube subtitles to improve performance and reduce bandwidth usage:

### How It Works
- **URL Hashing**: Each YouTube URL is converted to a SHA-256 hash for consistent file naming
- **Cache Storage**: Subtitles are stored in `subtitles/cached_{hash}.vtt` format
- **Automatic Detection**: Before downloading, the bot checks if cached subtitles exist
- **Validation**: Cached files are validated for content integrity before use
- **Fallback**: If cached files are invalid, the bot automatically re-downloads

### Benefits
- **⚡ Faster Processing**: Subsequent requests for the same video are instant
- **🌐 Reduced Bandwidth**: No need to re-download subtitles from YouTube
- **🔄 Reliability**: Reduces dependency on YouTube's availability
- **💾 Storage Efficient**: Uses SHA-256 hashing for compact, unique filenames

### Cache Management
- **Location**: `subtitles/` directory
- **Format**: `cached_{sha256_hash}.vtt`
- **Automatic Cleanup**: Old temporary files are automatically managed
- **Manual Cleanup**: You can safely delete cached files to free space

### Example Cache Files
```
subtitles/
├── cached_a1b2c3d4e5f6...vtt  # Cached subtitles for video 1
├── cached_f6e5d4c3b2a1...vtt  # Cached subtitles for video 2
└── yt_transcript_*.vtt        # Temporary files (auto-managed)
```

## Inviting the Bot to Your Server

### Automatic Invite Link (Recommended)
When you start the bot, it will automatically display an invite link in the terminal:

```
🎉 Bot is ready! Invite link:
🔗 https://discord.com/api/oauth2/authorize?client_id=1385309017881968761&permissions=274877910016&scope=bot
📋 Copy this link to invite the bot to your server
```

Simply copy and paste this link into your browser to invite the bot to your server.

### Manual Invite Link Creation
If you need to create a custom invite link:

1. In the Discord Developer Portal, go to your application
2. Navigate to OAuth2 → URL Generator
3. Select scopes: `bot`
4. Select bot permissions:
   - Send Messages
   - Read Messages/View Channels
   - Read Message History
   - Attach Files
5. Copy the generated URL and open it in your browser
6. Select a server and authorize the bot

## Running the Bot

### Option 1: Using the Helper Script (Recommended)
```powershell
# Run with token as parameter (Windows)
.\run_bot.ps1 -Token "your_actual_discord_token"

# Or set environment variable first
$env:DISCORD_TOKEN="your_actual_discord_token"
.\run_bot.ps1
```

### Option 2: Using the Bot Commands Interface (Windows)
```cmd
# Run the bot commands interface
bot_commands.bat
```

This opens a dedicated command window for bot management with the following commands:
- `help` - Show available bot commands
- `status` - Show bot status
- `sysinfo` - Get system information
- `processes` - List running processes
- `disk` - Get disk space information
- `network` - Get network information
- `new` - Open a new command prompt window
- `exit` - Close the window

### Option 3: Direct Environment Variables
```powershell
$env:DISCORD_TOKEN="your_actual_discord_token"
$env:PREFIX="^"
cargo run
```

## Project Structure

```
meri_bot_rust/
├── src/
│   ├── main.rs                # Entry point and command group setup
│   ├── commands/
│   │   ├── mod.rs             # Command module declarations
│   │   ├── ping.rs            # Ping command with response time
│   │   ├── echo.rs            # Echo command implementation
│   │   ├── lm.rs              # LM Studio AI chat and search commands
│   │   ├── reason.rs          # AI reasoning command  
│   │   ├── sum.rs             # Content summarization command
│   │   ├── search.rs          # DuckDuckGo web search functionality
│   │   ├── vis.rs             # Vision analysis and image processing
│   │   └── help.rs            # Help command system
│   ├── terminal.rs            # External command execution and system management
│   └── terminal_manager.rs    # Terminal output management and prompt handling
├── contexts/                  # Persistent conversation history storage
├── subtitles/                 # YouTube subtitle cache directory
├── target/                    # Rust build artifacts
├── Cargo.toml                 # Dependencies
├── botconfig.txt             # Bot configuration (create this)
├── example_botconfig.txt     # Example bot configuration file
├── lmapiconf.txt             # LM Studio/Ollama API configuration (required for AI commands)
├── example_lmapiconf.txt     # Example LM API configuration template
├── system_prompt.txt         # AI system prompt (required for AI commands)
├── reasoning_prompt.txt      # Optional: Specialized prompt for reasoning command
├── rank_system_prompt.txt    # Optional: Qwen3 reranking model prompt for content analysis
├── reasoning_search_analysis_prompt.txt # Optional: Reasoning-focused search analysis prompt
├── refine_search_prompt.txt     # Optional: AI search query refinement prompt
├── summarize_search_prompt.txt  # Optional: AI search result summarization prompt
├── youtube_prompt_generation_prompt.txt # Optional: YouTube prompt generation
├── youtube_summarization_prompt.txt # Optional: YouTube-specific summarization prompt
├── example_system_prompt.txt     # Example system prompt template
├── example_reasoning_prompt.txt  # Example reasoning prompt template
├── example_rank_system_prompt.txt # Example Qwen3 reranking prompt template
├── example_reasoning_search_analysis_prompt.txt # Example reasoning search analysis template
├── example_refine_search_prompt.txt    # Example search refinement prompt template
├── example_summarize_search_prompt.txt # Example search summarization prompt template
├── example_youtube_prompt_generation_prompt.txt # Example YouTube prompt generation template
├── example_youtube_summarization_prompt.txt # Example YouTube summarization template
├── serpapi.txt               # SerpAPI key for enhanced web search (optional)
├── bot_commands.bat          # Windows bot command interface
├── run_bot.ps1              # Helper script to run the bot
├── log.txt                  # Comprehensive logging output
├── LOGGING.md               # Detailed logging documentation
├── PLANNING.md              # Development planning and roadmap
└── README.md                # This file
```

## Configuration

The bot uses the following configuration:
- Command prefix: Configurable via `PREFIX` environment variable (default: "^")
- Case insensitive commands
- Comprehensive logging to `log.txt` with trace-level detail

## Usage

The bot responds to commands with the configured prefix (default: `^`):
- Type `^help` in any channel the bot can see to get a comprehensive command list
- Commands are case-insensitive and show typing indicators
- All commands provide helpful error messages and usage guidance

### Quick Start
1. `^ping` - Test basic bot functionality
2. `^help` - View all available commands with categories and aliases  
3. `^ppfp @user` - Try the profile picture feature
4. `^lm -s rust programming` - Test AI-enhanced search (with AI config) or basic search (fallback)
5. `^lm Hello!` - Test AI chat (requires configuration)
6. `^reason Why did the sky turn red at sunset?` - Test AI reasoning (requires configuration)
7. `^reason -s quantum computing applications` - Test reasoning-enhanced analytical search
8. `^sum https://youtube.com/watch?v=...` - Test YouTube video summarization

### Profile Picture Command

The `^ppfp` command displays user profile pictures in rich embeds:

- **Usage**: `^ppfp @username` 
- **Aliases**: `^avatar @username`, `^pfp @username`, `^profilepic @username`
- **Features**:
  - Shows user's profile picture in a rich embed
  - Supports animated GIFs, PNG, JPG, and WebP formats
  - Clickable title links to high-resolution original image
  - Memory-efficient: downloads images to RAM, no disk storage
  - Shows requester information and timestamp

### AI Chat Command (LM Studio/Ollama)

The `^lm` command provides real-time AI chat functionality via LM Studio or Ollama:

- **Usage**: `^lm <your prompt>` 
- **Aliases**: `^llm <prompt>`, `^ai <prompt>`, `^chat <prompt>`
- **Core Features**:
  - **🔄 Real-time streaming** - Responses appear live as the AI generates them
  - **⚡ Live message editing** - Discord messages update every 0.8 seconds during generation
  - **📝 Smart word-boundary chunking** - Automatically splits responses across multiple 2000-character Discord messages
  - **📊 Live progress tracking** - See character counts and generation status in real-time
  - **🎯 Extended response length** - Up to 8192 tokens by default for comprehensive answers
  - **🔢 Multi-part responses** - Numbered parts (Part 1/N) for long responses with completion indicators
- **Technical Features**:
  - **🛠️ Intelligent model management** - No manual loading/unloading required
  - **🔧 Configurable parameters** - Temperature (0.8), tokens, and formatting customizable
  - **❌ Comprehensive error handling** - Detailed error messages and recovery guidance
  - **⚙️ Server-Sent Events (SSE)** - Efficient streaming protocol for real-time updates
  - **🔌 Robust Connection Handling** - Uses a connection timeout and line buffering to ensure reliable stream processing, even for long-running AI tasks.
- **Requirements**:
  - LM Studio (default: localhost:1234) or Ollama (default: localhost:11434)
  - Valid model loaded in your AI server
  - Complete `lmapiconf.txt` configuration (8 required settings)
  - `system_prompt.txt` file for AI personality/behavior

### 🎲 Reproducible Responses with Seeds

The `^lm --seed` command provides deterministic AI responses for testing and debugging:

- **Usage**: `^lm --seed <number> <prompt>`
- **Example**: `^lm --seed 42 What is the meaning of life?`
- **Features**:
  - **🎯 Deterministic Output** - Same input + same seed = same response every time
  - **🧪 Testing & Debugging** - Perfect for verifying model behavior and debugging prompts
  - **📊 Academic Research** - Reproducible experiments and consistent results
  - **🎨 Creative Consistency** - Get the same creative output for content generation
  - **🔒 No Context History** - Seed requests don't use conversation history (ensures reproducibility)
  - **⚡ Same Performance** - Real-time streaming with all standard features
- **Configuration**:
  - **Global Seed**: Set `DEFAULT_SEED=42` in `lmapiconf.txt` for all responses
  - **Per-Request Seed**: Use `^lm --seed <number> <prompt>` for specific requests
  - **Seed Range**: Any non-negative integer (0, 1, 42, 12345, etc.)
- **Use Cases**:
  - **Testing**: Verify prompt changes produce expected results
  - **Debugging**: Reproduce issues with consistent model behavior
  - **Research**: Academic experiments requiring reproducibility
  - **Content Creation**: Generate consistent creative content
  - **Quality Assurance**: Ensure model responses meet standards

### AI Reasoning Command (LM Studio/Ollama)

The `^reason` command provides advanced AI reasoning capabilities with real-time streaming and thinking content filtering:

- **Usage**: `^reason <your reasoning question>` 
- **Aliases**: `^reasoning <question>`
- **Core Features**:
  - **🧠 Dedicated reasoning model** - Qwen3 4B model optimized for logical analysis and step-by-step thinking
  - **🔄 Real-time streaming** - Watch reasoning unfold live as the AI processes your question
  - **🎯 Thinking tag filtering** - Automatically removes `<think>...</think>` content in real-time during streaming
  - **📋 Step-by-step explanations** - Detailed logical breakdowns and reasoning processes
  - **⚙️ Specialized system prompts** - Optimized prompts for reasoning tasks and logical analysis
  - **🔄 Real-time message editing** - Discord messages update every 0.8 seconds during generation
  - **📝 Smart word-boundary chunking** - Automatically splits responses across multiple 2000-character Discord messages
- **Advanced Thinking Tag Filtering**:
  - **🔍 Real-time filtering** - `<think>` content is removed as responses stream, not after completion
  - **🧹 Clean user experience** - Only the final reasoning conclusions appear in Discord
  - **🔄 Multi-block support** - Handles multiple thinking sections within a single response
  - **🛡️ Robust handling** - Properly manages unclosed thinking tags and malformed content
  - **📊 Statistics tracking** - Shows how much thinking content was filtered out
  - **❓ Empty response handling** - Helpful messages when responses contain only thinking content
- **Technical Features**:
  - **📝 Same streaming architecture** - Uses the same real-time message editing as chat command
  - **🔢 Multi-part responses** - Long reasoning explanations split intelligently across Discord messages
  - **📁 Fallback prompts** - Uses `system_prompt.txt` if `reasoning_prompt.txt` isn't found
  - **🔧 Independent configuration** - Separate model configuration and multi-path file search
  - **🔌 Robust Connection Handling** - Employs a connection timeout and line buffering to maintain a stable connection to the API, even during lengthy or delayed responses.
- **Requirements**:
  - Same as LM chat command plus `DEFAULT_REASON_MODEL` configuration
  - Optional: `reasoning_prompt.txt` file for specialized reasoning instructions
  - Falls back to `system_prompt.txt` if reasoning prompt not found
  - **Current Model**: `qwen2.5:4b` (supports thinking tags and advanced reasoning)

### Content Summarization Command

The `^sum` command provides comprehensive content summarization with enhanced logging and error handling:

- **Usage**: `^sum <url>` 
- **Aliases**: `^summarize <url>`, `^webpage <url>`
- **Core Features**:
  - **📺 YouTube Support** - Automatic transcript extraction using yt-dlp
  - **🌐 Webpage Support** - HTML content extraction and intelligent cleaning
  - **🧠 AI Summarization** - Uses reasoning model for intelligent content analysis
  - **📝 RAG Processing** - Map-reduce summarization for long content (>8K characters)
  - **🔄 Real-time streaming** - Live progress updates during processing
  - **📊 Smart chunking** - Automatically splits long summaries across multiple messages
  - **🎯 Thinking tag filtering** - Removes `<think>` sections from responses
  - **⏱️ 60-second timeout** - Reliable processing with timeout protection
- **Enhanced Logging Features**:
  - **🔍 Step-by-step tracking** - Every phase of processing is logged with unique UUIDs
  - **📊 Performance metrics** - Character counts, processing times, and success rates
  - **🛠️ Error diagnosis** - Detailed error logging with context and recovery suggestions
  - **📈 Progress monitoring** - Live updates during YouTube transcript extraction and content processing
  - **🔧 Configuration validation** - Logs configuration loading and validation steps
- **YouTube Processing**:
  - **📥 Automatic subtitle download** - Uses yt-dlp for reliable transcript extraction
  - **🧹 VTT cleaning** - Intelligent cleaning of subtitle timestamps and formatting
  - **🔄 Retry logic** - Automatic retry with rate limiting for failed downloads
  - **📁 File management** - Efficient subtitle file handling and cleanup
- **Webpage Processing**:
  - **🌐 HTTP requests** - Robust web content fetching with error handling
  - **🧹 HTML cleaning** - Intelligent removal of scripts, styles, and formatting
  - **📝 Content validation** - Ensures extracted content is meaningful and complete
- **Examples**:
  ```
  ^sum https://youtube.com/watch?v=dQw4w9WgXcQ
  ^sum https://example.com/article
  ^summarize https://github.com/rust-lang/rust
  ```
- **Requirements**:
  - yt-dlp installed for YouTube support (optional but recommended)
  - Internet connection for web content fetching
  - LM Studio/Ollama configuration for AI summarization

### Reasoning-Enhanced Web Search Command

The `^reason -s` command provides analytical web search capabilities using the reasoning model for deeper insights:

- **Usage**: `^reason -s <search query>` or `^reason --search <search query>`
- **Reasoning-Enhanced Mode** (when LM Studio/Ollama is configured):
  1. **🧠 Query Optimization** - Reasoning model refines your query for analytical research
  2. **🔍 Web Search** - Searches SerpAPI with the optimized query
  3. **🤖 Analytical Synthesis** - Reasoning model provides deep analysis with embedded links
  4. **📊 Progress Updates** - Real-time status: "Refining for reasoning analysis..." → "Searching..." → "Analyzing with reasoning model..."
  5. **📝 Buffered Chunking** - Content is accumulated in a buffer and posted in 2000-character chunks with proper text wrapping
- **Basic Mode** (fallback when AI is unavailable):
  - Direct SerpAPI search with formatted results
  - Shows top 5 results with titles, descriptions, and clickable links
- **Examples**:
  ```
  ^reason -s quantum computing applications
  ^reason -s "climate change economic impacts"
  ^reasoning --search artificial intelligence ethics
  ```
- **Key Features**:
  - **🧠 Analytical Focus** - Uses Qwen3 4B reasoning model for deeper analysis beyond simple summarization
- **📝 Research-Oriented** - Optimizes queries for academic and analytical content
- **🔗 Embedded Links** - Source links naturally integrated in analytical responses
- **⚡ Real-time Progress** - Live updates during the analysis process
- **📝 Buffered Chunking** - Content is posted in discrete 2000-character chunks with proper formatting
- **🛡️ Robust Fallback** - Falls back to basic search when reasoning enhancement fails
- **🎯 Specialized Prompts** - Uses reasoning-specific prompts for analytical synthesis
- **🧹 Thinking Tag Filtering** - Automatically removes `<think>` content during processing

### AI-Enhanced Web Search Command

The `^lm -s` command provides intelligent web search functionality with AI assistance:

- **Usage**: `^lm -s <search query>` or `^lm --search <search query>`
- **AI-Enhanced Mode** (when LM Studio/Ollama is configured):
  1. **🧠 Query Optimization** - AI refines your search query for better results
  2. **🔍 Web Search** - Searches DuckDuckGo with the optimized query
  3. **🤖 Intelligent Summary** - AI synthesizes results into a comprehensive response
  4. **📊 Progress Updates** - Real-time status: "Refining query..." → "Searching..." → "Summarizing..."
- **Basic Mode** (fallback when AI is unavailable):
  - Direct DuckDuckGo search with formatted results
  - Shows top 5 results with titles, descriptions, and clickable links
- **Examples**:
  ```
  ^lm -s rust programming tutorial
  ^lm -s "how to create discord bot"
  ^lm --search async programming patterns
  ```
- **Key Features**:
  - **🚀 Dual Mode Operation** - AI-enhanced with graceful fallback
  - **📝 Smart Formatting** - Discord-optimized responses with bold text and code blocks
  - **🔗 Source Citations** - Includes links to most relevant sources
  - **⚡ Real-time Progress** - Live updates during the search process
  - **🛡️ Robust Error Handling** - Comprehensive fallback strategies

### Vision Analysis Module

The bot includes a dedicated vision analysis module (`vis.rs`) that provides advanced image processing capabilities:

- **GIF Support**: Automatically extracts the first frame from animated GIFs and converts to PNG for vision model compatibility
- **Multiple Formats**: Supports JPG, PNG, GIF, and WebP image formats
- **Base64 Encoding**: Converts images to base64 data URIs for multimodal AI processing
- **Error Handling**: Robust error handling with fallback mechanisms
- **Memory Efficient**: Processes images in memory without persistent disk storage
- **Cross-Platform**: Works on Windows, macOS, and Linux

## Development

To add new commands:

1. Create a new command function in a separate module file (e.g., `src/commands/mycommand.rs`)
2. Add the module declaration to `src/commands/mod.rs`
3. Import the command constant in `src/main.rs`: `use crate::commands::mycommand::MYCOMMAND_COMMAND;`
4. Add the command to the `#[commands()]` attribute in the General group in `src/main.rs`
5. Implement the command logic in your module file

Example command module (`src/commands/mycommand.rs`):
```rust
use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};

#[command]
pub async fn mycommand(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    msg.reply(ctx, "Hello!").await?;
    Ok(())
}
```

Then update `src/commands/mod.rs`:
```rust
pub mod mycommand;
```

And update `src/main.rs`:
```rust
use crate::commands::mycommand::MYCOMMAND_COMMAND;

#[group]
#[commands(ping, echo, lm, reason, sum, mycommand)]
struct General;
```

### Configuration Loading

All commands use robust multi-path configuration loading:
- Searches for configuration files in multiple locations: current directory, parent directories, and src/
- Each command loads configuration independently for maximum reliability
- Comprehensive error messages help diagnose configuration issues
- Console logging shows which configuration files and models are being used

### Enhanced Logging System

The bot includes a comprehensive logging system that provides complete visibility into operations:

- **📊 Log Levels**: trace, debug, info, warn, error with appropriate filtering
- **🔍 Unique Tracking**: Each command execution gets a unique UUID for tracking
- **📈 Performance Metrics**: Character counts, processing times, and success rates
- **🛠️ Error Context**: Detailed error logging with stack traces and recovery suggestions
- **📝 User Analytics**: Command usage patterns and response quality tracking
- **🔄 Real-Time Updates**: Live logging during streaming operations
- **📁 Persistent Storage**: All logs saved to `log.txt` for analysis

### Dependencies

The bot uses these key dependencies:

#### Core Framework
- `serenity` (0.11) - Discord API wrapper with command framework
- `tokio` (1.x) - Async runtime with full features

#### HTTP & Networking  
- `reqwest` (0.11) - HTTP client with JSON, streaming, and blocking support
- `futures-util` (0.3) - Stream processing utilities
- `tokio-stream` (0.1) - Async stream utilities with io-util features

#### Web Scraping & Search
- `serpapi` (1.0) - Official SerpAPI client for web search functionality

#### Data Handling
- `serde` (1.0) - JSON serialization/deserialization with derive macros
- `serde_json` (1.0) - JSON processing for API requests/responses

#### Image Processing
- `image` (0.24) - Image processing with GIF, PNG, JPEG support
- `base64` (0.22) - Base64 encoding for image data URIs
- `mime` (0.3) - MIME type handling
- `mime_guess` (2.0) - MIME type detection

#### Document Processing
- `pdf-extract` (0.6) - PDF text extraction

#### Logging & Utilities
- `env_logger` (0.10) - Environment-based logging configuration
- `log` (0.4) - Logging facade for Rust
- `uuid` (1.0) - UUID generation for request tracking
- `chrono` (0.4) - Date and time handling
- `regex` (1.10) - Regular expression support
- `once_cell` (1.19) - One-time initialization
- `lazy_static` (1.4) - Lazy static initialization

All dependencies are specified in `Cargo.toml` with appropriate feature flags for optimal performance and functionality.

## Troubleshooting

### Common Issues

1. **Bot not responding to mentions**
   - Check that the bot has the correct permissions
   - Verify the bot user ID in the code matches your bot
   - Ensure the bot is online and connected

2. **AI commands not working**
   - Verify LM Studio or Ollama is running
   - Check `lmapiconf.txt` configuration
   - Ensure models are loaded in your AI server
   - Check the log file for detailed error messages

3. **YouTube summarization failing**
   - Install yt-dlp: `pip install yt-dlp` or use package manager
   - Verify yt-dlp is in your PATH: `yt-dlp --version`
   - Check the log file for specific error details

4. **Vision analysis not working**
   - Ensure you have the required image processing dependencies
   - Check that the image format is supported (JPG, PNG, GIF, WebP)
   - Verify the AI model supports vision capabilities
   - Check the log file for processing errors

5. **Web search not working**
   - Verify your SerpAPI key in `serpapi.txt` is valid
   - Check internet connectivity
   - Ensure the search query is properly formatted
   - Check the log file for API errors

6. **Logging issues**
   - Ensure `RUST_LOG=trace` is set in `botconfig.txt`
   - Check that `log.txt` is writable
   - Verify no other processes are locking the log file

### Getting Help

- Check the `log.txt` file for detailed error messages and debugging information
- Review the `LOGGING.md` file for logging system documentation
- Examine the `PLANNING.md` file for development roadmap and known issues
- Ensure all configuration files are properly set up using the example templates

## License

This project is licensed under the MIT License - see the LICENSE file for details.