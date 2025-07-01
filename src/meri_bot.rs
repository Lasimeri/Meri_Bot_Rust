use serenity::{
    async_trait,
    client::{Client, Context, EventHandler},
    framework::{
        standard::{
            macros::{command, group},
            Args, CommandResult, StandardFramework,
        },
    },
    model::{channel::Message, gateway::Ready},
    prelude::GatewayIntents,
};
use std::env;
use std::fs;
use std::collections::HashMap;
use tokio::signal;
use crate::profilepfp::*;
use crate::lm::*;
use crate::reason::*;

// Command group declaration
#[group]
#[commands(ping, echo, help, ppfp, lm, reason)]
struct General;

// Event handler implementation
struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("✅ Bot connected as {}!", ready.user.name);
    }
}

// Function to read configuration from botconfig.txt
fn load_bot_config() -> Result<HashMap<String, String>, String> {
    let config_paths = [
        "botconfig.txt",
        "../botconfig.txt", 
        "../../botconfig.txt",
        "src/botconfig.txt"
    ];
    
    // Clear any existing relevant environment variables
    env::remove_var("DISCORD_TOKEN");
    env::remove_var("PREFIX");
    env::remove_var("RUST_LOG");
    
    for config_path in &config_paths {
        match fs::read_to_string(config_path) {
            Ok(content) => {
                // Remove BOM if present
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                let mut config = HashMap::new();
                
                // Parse the config file line by line
                for line in content.lines() {
                    let line = line.trim();
                    
                    // Skip empty lines and comments
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    
                    // Parse KEY=VALUE format
                    if let Some(equals_pos) = line.find('=') {
                        let key = line[..equals_pos].trim().to_string();
                        let value = line[equals_pos + 1..].trim().to_string();
                        
                        // Set environment variable for compatibility
                        env::set_var(&key, &value);
                        config.insert(key, value);
                    }
                }
                
                return Ok(config);
            }
            Err(_) => {
                // Try next path
                continue;
            }
        }
    }
    
    Err("No botconfig.txt file found in any expected location".to_string())
}

// Command implementations
#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    msg.reply(ctx, "Pong! 🏓").await?;
    Ok(())
}

#[command]
async fn echo(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    let text = args.message();
    if text.is_empty() {
        msg.reply(ctx, "Please provide text to echo!").await?;
    } else {
        msg.reply(ctx, text).await?;
    }
    Ok(())
}

#[command]
async fn help(ctx: &Context, msg: &Message) -> CommandResult {
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    let prefix = env::var("PREFIX").unwrap_or_else(|_| "!".to_string());
    
    let response = format!(
        "**🤖 Meri Bot - Comprehensive Command Guide**\n\n\
        **📋 Basic Commands:**\n\
        • `{0}ping` - Test bot connectivity and response time\n\
        • `{0}echo <text>` - Echo back your message\n\
        • `{0}help` - Display this comprehensive command guide\n\n\
        **🖼️ Profile Picture Commands:**\n\
        • `{0}ppfp @user` - Display user's profile picture in rich embed\n\
        • **Aliases:** `{0}avatar`, `{0}pfp`, `{0}profilepic`\n\
        • **Features:** High-quality embeds, clickable links, animated GIF support\n\n\
        **🤖 AI Chat (LM Studio/Ollama):**\n\
        • `{0}lm <prompt>` - Interactive AI chat with real-time streaming\n\
        • **Aliases:** `{0}llm`, `{0}ai`, `{0}chat`\n\
        • **Features:** Live response streaming, multi-part messages, 8K token support\n\
        • **Requirements:** LM Studio or Ollama with configured models\n\n\
        **🧠 AI Reasoning (Advanced):**\n\
        • `{0}reason <question>` - Specialized AI reasoning with step-by-step analysis\n\
        • **Aliases:** `{0}reasoning`\n\
        • **Features:** Thinking tag filtering, logical breakdown, dedicated reasoning models\n\
        • **Best for:** Complex problems, logical analysis, step-by-step explanations\n\n\
        **💡 User Experience Features:**\n\
        • ⌨️ **Typing indicators** on all commands for immediate feedback\n\
        • 🔄 **Real-time streaming** for AI responses with live updates\n\
        • 📝 **Smart message chunking** respects Discord's 2000 character limit\n\
        • ❌ **Comprehensive error handling** with helpful guidance messages\n\
        • 🎯 **Case-insensitive commands** work with any capitalization\n\n\
        **🛠️ Setup Requirements:**\n\
        • Discord bot token in `botconfig.txt`\n\
        • LM Studio/Ollama configuration in `lmapiconf.txt` (for AI features)\n\
        • System prompts in `system_prompt.txt` and `reasoning_prompt.txt`\n\n\
        **📚 Need help?** Check the README.md for detailed setup instructions!", 
        prefix
    );
    
    msg.reply(ctx, &response).await?;
    Ok(())
}

// Main bot function
pub async fn run() {
    // Load configuration from botconfig.txt file
    match load_bot_config() {
        Ok(_) => println!("✅ Configuration loaded from botconfig.txt"),
        Err(error) => {
            eprintln!("❌ Failed to load botconfig.txt: {}", error);
            eprintln!("Create a botconfig.txt file in the project root with: DISCORD_TOKEN=your_token_here and PREFIX=^");
            return;
        }
    };
    
    // Get Discord token from configuration
    let token = match env::var("DISCORD_TOKEN") {
        Ok(token) => {
            // Validate token is not placeholder
            if token == "YOUR_BOT_TOKEN_HERE" || token.is_empty() {
                eprintln!("❌ DISCORD_TOKEN in botconfig.txt is set to placeholder! Replace with your actual Discord bot token.");
                return;
            }
            token
        }
        Err(_) => {
            eprintln!("❌ DISCORD_TOKEN not found in botconfig.txt file!");
            return;
        }
    };
    
    // Get command prefix from configuration
    let prefix = env::var("PREFIX").unwrap_or_else(|_| "!".to_string());
    println!("🤖 Starting bot with prefix: '{}'", prefix);
    
    // Set up command framework
    let framework = StandardFramework::new()
        .configure(|c| c.prefix(&prefix).case_insensitivity(true))
        .group(&GENERAL_GROUP);

    // Configure bot intents
    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::MESSAGE_CONTENT;

    // Create and start client
    let mut client = match Client::builder(token, intents)
        .event_handler(Handler)
        .framework(framework)
        .await
    {
        Ok(client) => client,
        Err(e) => {
            eprintln!("❌ Error creating Discord client: {:?}", e);
            eprintln!("Check your token in botconfig.txt file");
            return;
        }
    };

    // Set up graceful shutdown on CTRL+C
    println!("🚀 Bot is running... Press Ctrl+C to stop");
    tokio::select! {
        _ = signal::ctrl_c() => {
            println!("\n⏹️ Stopping bot gracefully...");
        }
        result = client.start() => {
            if let Err(why) = result {
                eprintln!("❌ Client error: {:?}", why);
            }
        }
    }
    
    println!("✅ Bot stopped");
} 