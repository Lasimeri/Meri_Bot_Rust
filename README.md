# Meri Bot Rust

A simple Discord bot written in Rust using the Serenity framework.

## ⚠️ Security Notice

**NEVER commit your Discord bot token to version control!**

- Your Discord token is like a password - keep it secret
- The `.gitignore` file is configured to prevent accidental token uploads
- Use the `bot_config.txt` file or environment variables to set your token
- If you accidentally commit a token, regenerate it immediately in the Discord Developer Portal

## Features

- `^ping` - Test bot response
- `^echo <text>` - Repeat your message
- `^ppfp @user` - Show user's profile picture in a rich embed
- `^help` - Show available commands

## Prerequisites

- Rust (latest stable version)
- A Discord bot token

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

3. **Set up environment variables**
   
   Create a `bot_config.txt` file in the project root:
   ```
   DISCORD_TOKEN=your_bot_token_here
   PREFIX=^
   ```
   
   Note: The PREFIX can be customized to any character(s) you prefer (default is "!")

4. **Build and run**
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
│   └── profilepfp.rs         # Profile picture command
├── target/                   # Rust build artifacts
├── Cargo.toml                # Dependencies
├── bot_config.txt           # Bot configuration (create this)
├── example_bot_config.txt   # Example configuration file
├── run_bot.ps1              # Helper script to run the bot
└── README.md                # This file
```

## Configuration

The bot uses the following configuration:
- Command prefix: Configurable via `PREFIX` environment variable (default: "!")
- Case insensitive commands

## Usage

The bot responds to commands with the configured prefix (default: `^`):
- Type `^help` in any channel the bot can see to get a list of commands
- Commands are case-insensitive

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

### Adding Dependencies

The bot uses several key dependencies:
- `serenity` - Discord API wrapper
- `tokio` - Async runtime
- `reqwest` - HTTP client for downloading images

Add new dependencies to `Cargo.toml` as needed.

## License

This project is open source and available under the [MIT License](LICENSE). 