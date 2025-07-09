# Meri Bot Rust

A powerful Discord bot written in Rust using the Serenity framework, featuring real-time AI chat streaming, advanced reasoning capabilities, and comprehensive user interaction features.

## ⚠️ Security Notice

**NEVER commit sensitive configuration to version control!**

- Your Discord token is like a password - keep it secret
- Model names and prompts may contain proprietary or sensitive information
- The `.gitignore` file is configured to prevent accidental uploads of:
  - `botconfig.txt` (contains Discord token)
  - `lmapiconf.txt` (contains model names and API settings)
  - `system_prompt.txt` and `reasoning_prompt.txt` (may contain custom prompts)
- Use the example files as templates and customize your actual configuration files
- If you accidentally commit sensitive data, regenerate tokens and review what was exposed

## Features

### 📋 Basic Commands
- `^ping` - Test bot response with typing indicator
- `^echo <text>` - Repeat your message
- `^help` - Show comprehensive command list with categories

### 🖼️ Profile Picture Commands  
- `^ppfp @user` - Show user's profile picture in a rich embed
  - **Aliases**: `^avatar`, `^pfp`, `^profilepic`
  - **Features**: High-quality embeds, clickable links, memory-efficient processing

### 🤖 AI Chat Commands
- `^lm <prompt>` - Chat with AI via LM Studio/Ollama
  - **Aliases**: `^llm`, `^ai`, `^chat` 
  - **Features**: **Real-time streaming responses**, smart message chunking, extended output length (8K tokens), live progress indicators, multi-part message support, robust buffered streaming for improved reliability
- `^lm -s <search query>` - AI-enhanced web search with intelligent query optimization and result summarization
  - **Aliases**: `^lm --search <query>`
  - **Features**: **AI query refinement**, **intelligent summarization with embedded links**, real-time progress updates, fallback to basic search
- `^reason <question>` - Deep reasoning with specialized AI model
  - **Aliases**: `^reasoning`
  - **Features**: **Real-time streaming with thinking tag filtering**, step-by-step reasoning, dedicated reasoning model (DeepSeek R1), automatic `<think>` content removal, logical explanations, robust buffered streaming for improved reliability
- `^reason -s <search query>` - Reasoning-enhanced web search with analytical insights
  - **Aliases**: `^reasoning -s`, `^reasoning --search`
  - **Features**: **Analytical research synthesis**, reasoning-focused query optimization, embedded source links, specialized reasoning model analysis (Qwen3 4B), **buffered chunking** (posts content in 2000-character chunks)

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
- **Robust Connection Handling** - Uses a `connect_timeout` to prevent premature disconnection during long AI generations, ensuring that even slow responses are fully received.
- **Buffered Stream Processing** - Assembles incoming data into a line buffer before parsing. This prevents errors caused by data packets being split across network chunks, making the stream processing significantly more reliable.
- **Live Discord Message Editing** - Messages update in real-time every 0.8 seconds with the latest content from the stream.
- **Thinking Tag Filtering** - Automatically removes `<think>...</think>` content from reasoning responses in real-time.
- **Graceful Error Handling** - If a Discord message fails to update mid-stream, the entire operation is safely halted to prevent content loss, and the error is logged.

## Prerequisites

- Rust (latest stable version)
- A Discord bot token
- LM Studio or Ollama (for AI chat and reasoning functionality) - optional
- Internet connection (for web search functionality)

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
   RUST_LOG=info
   ```
   
   **Note:** The PREFIX can be customized to any character(s) you prefer

4. **Set up AI Chat (Optional - for LM Studio/Ollama functionality)**
   
   Create configuration files in the project root:
   ```bash
   # Copy configuration files from examples and customize  
   cp example_lmapiconf.txt lmapiconf.txt
   cp example_system_prompt.txt system_prompt.txt
   cp example_reasoning_prompt.txt reasoning_prompt.txt  # Optional for reasoning command
   ```
   
   Edit `lmapiconf.txt` with your AI server settings:
   ```
   LM_STUDIO_BASE_URL=http://127.0.0.1:11434  # Ollama default
   LM_STUDIO_TIMEOUT=30
   DEFAULT_MODEL=your-chat-model-name
   DEFAULT_REASON_MODEL=deepseek/deepseek-r1-0528-qwen3-8b  # DeepSeek R1 reasoning model
   DEFAULT_TEMPERATURE=0.8
   DEFAULT_MAX_TOKENS=8192                    # Extended for longer responses
   MAX_DISCORD_MESSAGE_LENGTH=2000           # Discord's limit
   RESPONSE_FORMAT_PADDING=50                # Buffer for formatting
   ```
   
   **Important:** 
   - All settings are mandatory - no defaults provided. See `example_lmapiconf.txt` for guidance.
   - Replace `your-chat-model-name` and `your-reasoning-model-name` with your actual model names.
   - For reasoning tasks, consider using models optimized for logical analysis (e.g., qwen, deepseek-r1, etc.).
   - The `system_prompt.txt` configures the AI's behavior and personality for chat interactions.

5. **Build and run**
   ```bash
   cargo build --release
   cargo run
   ```

## Inviting the Bot to Your Server

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

### Option 2: Direct Environment Variables
```powershell
$env:DISCORD_TOKEN="your_actual_discord_token"
$env:PREFIX="^"
cargo run
```

## Project Structure

```
meri_bot_rust/
├── src/
│   ├── main.rs                # Entry point and command group setup (simplified!)
│   ├── help.rs               # Help command implementation
│   ├── ping.rs               # Ping command with response time
│   ├── echo.rs               # Echo command implementation
│   ├── profilepfp.rs         # Profile picture command
│   ├── lm.rs                 # LM Studio AI chat and search commands
│   ├── reason.rs             # AI reasoning command  
│   └── search.rs             # DuckDuckGo web search functionality
├── target/                   # Rust build artifacts
├── Cargo.toml                # Dependencies
├── botconfig.txt            # Bot configuration (create this)
├── example_botconfig.txt    # Example bot configuration file
├── lmapiconf.txt            # LM Studio/Ollama API configuration (required for AI commands)
├── example_lmapiconf.txt    # Example LM API configuration template
├── system_prompt.txt        # AI system prompt (required for AI commands)
├── reasoning_prompt.txt     # Optional: Specialized prompt for reasoning command
├── reasoning_search_analysis_prompt.txt # Optional: Reasoning-focused search analysis prompt
├── refine_search_prompt.txt     # Optional: AI search query refinement prompt
├── summarize_search_prompt.txt  # Optional: AI search result summarization prompt
├── example_system_prompt.txt     # Example system prompt template
├── example_reasoning_prompt.txt  # Example reasoning prompt template
├── example_reasoning_search_analysis_prompt.txt # Example reasoning search analysis template
├── example_refine_search_prompt.txt    # Example search refinement prompt template
├── example_summarize_search_prompt.txt # Example search summarization prompt template
├── run_bot.ps1              # Helper script to run the bot
└── README.md                # This file
```

## Configuration

The bot uses the following configuration:
- Command prefix: Configurable via `PREFIX` environment variable (default: "!")
- Case insensitive commands

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

### AI Reasoning Command (LM Studio/Ollama)

The `^reason` command provides advanced AI reasoning capabilities with real-time streaming and thinking content filtering:

- **Usage**: `^reason <your reasoning question>` 
- **Aliases**: `^reasoning <question>`
- **Core Features**:
  - **🧠 Dedicated reasoning model** - DeepSeek R1 model optimized for logical analysis and step-by-step thinking
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
  - **Current Model**: `deepseek/deepseek-r1-0528-qwen3-8b` (supports thinking tags and advanced reasoning)

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
  - **🧠 Analytical Focus** - Uses DeepSeek R1 reasoning model for deeper analysis beyond simple summarization
- **📝 Research-Oriented** - Optimizes queries for academic and analytical content
- **🔗 Embedded Links** - Source links naturally integrated in analytical responses
- **⚡ Real-time Progress** - Live updates during the analysis process
- **📝 Buffered Chunking** - Content is posted in discrete 2000-character chunks with proper formatting
- **🛡️ Robust Fallback** - Falls back to basic search when reasoning enhancement fails
- **🎯 Specialized Prompts** - Uses reasoning-specific prompts for analytical synthesis
- **🧹 Thinking Tag Filtering** - Automatically removes `<think>` content during processing

### Setup for Reasoning-Enhanced Search

To enable reasoning-enhanced search features, ensure you have:

1. **Configuration Files**:
   ```bash
   # Copy and customize the reasoning analysis prompt template
   cp example_reasoning_search_analysis_prompt.txt reasoning_search_analysis_prompt.txt
   
   # Optional: Use existing search prompt templates
   cp example_refine_search_prompt.txt refine_search_prompt.txt
   cp example_summarize_search_prompt.txt summarize_search_prompt.txt
   ```

2. **LM Studio/Ollama Setup** (same as AI chat):
   - Valid `lmapiconf.txt` configuration with `DEFAULT_REASON_MODEL=deepseek/deepseek-r1-0528-qwen3-8b`
   - Running LM Studio or Ollama instance
   - DeepSeek R1 reasoning model loaded and accessible

3. **Specialized Features**:
   - **Reasoning Model**: Uses `deepseek/deepseek-r1-0528-qwen3-8b` for analytical capabilities
   - **Analytical Prompts**: Specialized prompts for reasoning-focused analysis
   - **Fallback Behavior**: Uses regular search prompts if reasoning-specific ones aren't found
   - **Independent Operation**: Works separately from regular AI chat and search functions

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

### Setup for AI-Enhanced Search

To enable AI-enhanced search features, ensure you have:

1. **Configuration Files**:
   ```bash
   # Copy and customize the search prompt templates
   cp example_refine_search_prompt.txt refine_search_prompt.txt
   cp example_summarize_search_prompt.txt summarize_search_prompt.txt
   ```

2. **LM Studio/Ollama Setup** (same as AI chat):
   - Valid `lmapiconf.txt` configuration
   - Running LM Studio or Ollama instance
   - Model loaded and accessible

3. **Fallback Behavior**:
   - If AI configuration fails, automatically falls back to basic search
   - No configuration required for basic search functionality
   - All search attempts will work, with varying levels of intelligence

### 🔍 Enhanced Web Search with Embedded Links

The AI-enhanced search functionality provides intelligent processing with embedded source links directly in the response.

**How It Works:**
1. **Query Refinement** - AI optimizes your search query for better results
2. **Web Search** - Performs DuckDuckGo search with the refined query
3. **AI Summarization** - Creates comprehensive responses with embedded source links

**User Experience:**
```
User: ^lm -s rust programming tutorial
Bot: 🧠 Refining search query...
     🔍 Searching with optimized query...
     🤖 Generating AI summary...
     
     **Rust Programming Fundamentals**
     
     Rust is a systems programming language focused on **safety**, **speed**, and **concurrency**. 
     Here are the key learning resources:
     
     **Getting Started:**
     • [The Rust Book](https://doc.rust-lang.org/book/) - Official comprehensive guide
     • [Rustlings](https://github.com/rust-lang/rustlings) - Interactive exercises
     • **Rust by Example** - Practical code examples and explanations
     
     ---
     *🔍 Searched: rust programming tutorial → rust programming language tutorial guide*
```

**Benefits:**
- **🔗 Embedded Links** - Source links naturally integrated in the response text
- **📊 Smart Formatting** - Discord markdown with bold text, code blocks, and organized structure
- **🧠 AI Processing** - Intelligent synthesis of multiple search results
- **🛡️ Robust Fallback** - Falls back to basic search when AI enhancement fails

**Configuration:**
- Enhanced mode requires LM Studio/Ollama setup with `lmapiconf.txt`
- Optional `refine_search_prompt.txt` and `summarize_search_prompt.txt` for customization
- Basic search works without any configuration - just needs internet connection

## Development

To add new commands:

1. Create a new command function in a separate module file (e.g., `src/mycommand.rs`)
2. Add the module declaration to `main.rs`: `mod mycommand;`
3. Import the command constant in `main.rs`: `use crate::mycommand::MYCOMMAND_COMMAND;`
4. Add the command to the `#[commands()]` attribute in the General group in `main.rs`
5. Implement the command logic in your module file

Example command module (`src/mycommand.rs`):
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

Then update `main.rs`:
```rust
mod mycommand;
use crate::mycommand::MYCOMMAND_COMMAND;

#[group]
#[commands(ping, echo, help, ppfp, lm, reason, mycommand)]
struct General;
```

### Configuration Loading

Both the `^lm` and `^reason` commands use robust multi-path configuration loading:
- Searches for `lmapiconf.txt` in multiple locations: current directory, parent directories, and src/
- Each command loads configuration independently for maximum reliability
- Comprehensive error messages help diagnose configuration issues
- Console logging shows which configuration files and models are being used

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

All dependencies are specified in `Cargo.toml` with appropriate feature flags for optimal performance and functionality.