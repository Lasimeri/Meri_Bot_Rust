# Meri Bot Rust

A simple Discord bot written in Rust using the Serenity framework.

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
  - **Features**: Buffered responses, smart message chunking, extended output length, progress indicators
- `^reason <question>` - Deep reasoning with specialized AI model
  - **Aliases**: `^reasoning`
  - **Features**: Step-by-step reasoning, dedicated reasoning model, logical explanations

### 💡 User Experience
- **Typing indicators** on all commands for immediate feedback
- **Error handling** with helpful guidance messages
- **Configuration validation** with clear error messages

## Prerequisites

- Rust (latest stable version)
- A Discord bot token
- LM Studio (for AI chat functionality) - optional

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
   DEFAULT_REASON_MODEL=your-reasoning-model-name
   DEFAULT_TEMPERATURE=0.8
   DEFAULT_MAX_TOKENS=8192                    # Extended for longer responses
   MAX_DISCORD_MESSAGE_LENGTH=2000           # Discord's limit
   RESPONSE_FORMAT_PADDING=50                # Buffer for formatting
   ```
   
   **Important:** 
   - All settings are mandatory - no defaults provided. See `example_lmapiconf.txt` for guidance.
   - Replace `your-chat-model-name` and `your-reasoning-model-name` with your actual model names.
   - For reasoning tasks, consider using models optimized for logical analysis (e.g., qwen, deepseek-r1, etc.).

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
│   ├── main.rs                # Entry point
│   ├── meri_bot.rs           # Main bot logic
│   ├── profilepfp.rs         # Profile picture command
│   ├── lm.rs                 # LM Studio AI chat command
│   └── reason.rs             # AI reasoning command
├── target/                   # Rust build artifacts
├── Cargo.toml                # Dependencies
├── botconfig.txt            # Bot configuration (create this)
├── example_botconfig.txt    # Example bot configuration file
├── lmapiconf.txt            # LM Studio/Ollama API configuration (required for AI commands)
├── example_lmapiconf.txt    # Example LM API configuration template
├── system_prompt.txt        # AI system prompt (required for AI commands)
├── reasoning_prompt.txt     # Optional: Specialized prompt for reasoning command
├── example_system_prompt.txt     # Example system prompt template
├── example_reasoning_prompt.txt  # Example reasoning prompt template
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
4. `^lm Hello!` - Test AI chat (requires configuration)
5. `^reason Why did the sky turn red at sunset?` - Test AI reasoning (requires configuration)

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

The `^lm` command provides AI chat functionality via LM Studio or Ollama:

- **Usage**: `^lm <your prompt>` 
- **Aliases**: `^llm <prompt>`, `^ai <prompt>`, `^chat <prompt>`
- **Features**:
  - **Buffered response system** for handling long AI outputs
  - **Smart word-boundary chunking** into 2000-character Discord messages
  - **Live progress indicators** showing response generation status
  - **Extended response length** (up to 8192 tokens by default)
  - **Multiple response parts** with clear numbering and character counts
  - **Intelligent model management** (no manual loading/unloading)
  - **Comprehensive error handling** and user feedback
  - **Configurable parameters** (temperature=0.8, tokens, formatting)
- **Requirements**:
  - LM Studio (default: localhost:1234) or Ollama (default: localhost:11434)
  - Valid model loaded in your AI server
  - Complete `lmapiconf.txt` configuration (8 required settings)
  - `system_prompt.txt` file for AI personality/behavior

### AI Reasoning Command (LM Studio/Ollama)

The `^reason` command provides advanced AI reasoning capabilities:

- **Usage**: `^reason <your reasoning question>` 
- **Aliases**: `^reasoning <question>`
- **Features**:
  - **Dedicated reasoning model** for complex logical problems
  - **Step-by-step thinking** with detailed explanations
  - **Specialized system prompt** optimized for reasoning tasks
  - **Same buffering system** as the chat command for long responses
  - **Fallback prompts** if custom reasoning prompt files aren't found
- **Requirements**:
  - Same as LM chat command plus `DEFAULT_REASON_MODEL` configuration
  - Optional: `reasoning_prompt.txt` file for specialized reasoning instructions
  - Falls back to `system_prompt.txt` if reasoning prompt not found

## Development

To add new commands:

1. Create a new command function (in `meri_bot.rs` or a separate module file)
2. Add the module to `main.rs` if using a separate file
3. Import the command in `meri_bot.rs`
4. Add the command to the `#[commands()]` attribute in the General group
5. Implement the command logic

Example:
```rust
#[command]
async fn mycommand(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, "Hello!").await?;
    Ok(())
}
```

### Dependencies

The bot uses these key dependencies:

#### Core Framework
- `serenity` (0.11) - Discord API wrapper with command framework
- `tokio` (1.x) - Async runtime with full features

#### HTTP & Networking  
- `reqwest` (0.11) - HTTP client with JSON and streaming support
- `futures-util` (0.3) - Stream processing utilities
- `tokio-stream` (0.1) - Async stream utilities with io-util features

#### Data Handling
- `serde` (1.0) - JSON serialization/deserialization with derive macros
- `serde_json` (1.0) - JSON processing for API requests/responses

All dependencies are specified in `Cargo.toml` with appropriate feature flags for optimal performance and functionality.