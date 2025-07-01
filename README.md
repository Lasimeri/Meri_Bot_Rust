# Meri Bot Rust

A simple Discord bot written in Rust using the Serenity framework.

## Features

- `!ping` - Check if the bot is responsive
- `!echo <text>` - Make the bot repeat your message
- `!help` - Display available commands

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
   
   Create a `.env` file in the project root:
   ```
   DISCORD_TOKEN=your_bot_token_here
   PREFIX=!
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
5. Copy the generated URL and open it in your browser
6. Select a server and authorize the bot

## Project Structure

```
meri_bot_rust/
├── src/
│   ├── main.rs        # Entry point
│   └── Meri_Bot.rs    # Main bot logic
├── Cargo.toml         # Dependencies
├── .env              # Environment variables (create this)
└── README.md         # This file
```

## Configuration

The bot uses the following configuration:
- Command prefix: Configurable via `PREFIX` environment variable (default: "!")
- Case insensitive commands

## Development

To add new commands:

1. Add the command function in `Meri_Bot.rs`
2. Add the command to the `#[commands()]` attribute in the General group
3. Implement the command logic

Example:
```rust
#[command]
async fn mycommand(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, "Hello!").await?;
    Ok(())
}
```

## License

This project is open source and available under the [MIT License](LICENSE). 