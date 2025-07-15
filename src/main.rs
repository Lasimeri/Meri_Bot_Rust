mod profilepfp;
mod lm;
mod reason;
mod search;
mod help;
mod ping;
mod echo;
mod sum;

mod vis;

use serenity::{
    async_trait,
    client::{Client, Context, EventHandler},
    framework::standard::{StandardFramework, macros::group, Args, Delimiter},
    model::gateway::Ready,
    prelude::GatewayIntents,
    prelude::TypeMapKey,
};
use std::env;
use std::fs;
use std::collections::HashMap;
use tokio::signal;
use tokio::sync::mpsc;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use serenity::model::id::UserId;
use crate::search::ChatMessage;
use serde::{Serialize, Deserialize};
use std::path::Path;
use std::io::Write;
use chrono::{DateTime, Utc};

use serenity::model::channel::Message;

// Enhanced context structure with 50/50 balance and persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    pub user_messages: Vec<ChatMessage>,
    pub assistant_messages: Vec<ChatMessage>,
    pub last_updated: DateTime<Utc>,
    pub total_interactions: usize,
}

impl UserContext {
    pub fn new() -> Self {
        Self {
            user_messages: Vec::new(),
            assistant_messages: Vec::new(),
            last_updated: Utc::now(),
            total_interactions: 0,
        }
    }

    pub fn add_user_message(&mut self, message: ChatMessage) {
        self.user_messages.push(message);
        self.maintain_balance();
        self.last_updated = Utc::now();
        self.total_interactions += 1;
    }

    pub fn add_assistant_message(&mut self, message: ChatMessage) {
        self.assistant_messages.push(message);
        self.maintain_balance();
        self.last_updated = Utc::now();
    }

    pub fn get_conversation_messages(&self) -> Vec<ChatMessage> {
        let mut messages = Vec::new();
        let user_len = self.user_messages.len();
        let assistant_len = self.assistant_messages.len();
        let max_len = std::cmp::max(user_len, assistant_len);

        for i in 0..max_len {
            if i < user_len {
                messages.push(self.user_messages[i].clone());
            }
            if i < assistant_len {
                messages.push(self.assistant_messages[i].clone());
            }
        }

        messages
    }

    fn maintain_balance(&mut self) {
        // Keep only the last 50 messages of each type
        if self.user_messages.len() > 50 {
            self.user_messages.drain(0..self.user_messages.len() - 50);
        }
        if self.assistant_messages.len() > 50 {
            self.assistant_messages.drain(0..self.assistant_messages.len() - 50);
        }
    }

    pub fn clear(&mut self) {
        self.user_messages.clear();
        self.assistant_messages.clear();
        self.last_updated = Utc::now();
    }

    pub fn total_messages(&self) -> usize {
        self.user_messages.len() + self.assistant_messages.len()
    }
}

// Persistence functions for contexts
pub async fn save_contexts_to_disk(
    lm_contexts: &HashMap<UserId, UserContext>,
    reason_contexts: &HashMap<UserId, UserContext>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Create contexts directory if it doesn't exist
    let contexts_dir = Path::new("contexts");
    if !contexts_dir.exists() {
        std::fs::create_dir_all(contexts_dir)?;
    }

    // Save LM contexts
    let lm_file = contexts_dir.join("lm_contexts.json");
    let lm_json = serde_json::to_string_pretty(lm_contexts)?;
    let mut lm_file_handle = std::fs::File::create(&lm_file)?;
    lm_file_handle.write_all(lm_json.as_bytes())?;

    // Save Reason contexts
    let reason_file = contexts_dir.join("reason_contexts.json");
    let reason_json = serde_json::to_string_pretty(reason_contexts)?;
    let mut reason_file_handle = std::fs::File::create(&reason_file)?;
    reason_file_handle.write_all(reason_json.as_bytes())?;

    println!("💾 Saved {} LM contexts and {} Reason contexts to disk", 
        lm_contexts.len(), reason_contexts.len());
    Ok(())
}

pub async fn load_contexts_from_disk() -> Result<(HashMap<UserId, UserContext>, HashMap<UserId, UserContext>), Box<dyn std::error::Error + Send + Sync>> {
    let contexts_dir = Path::new("contexts");
    
    // Load LM contexts
    let lm_file = contexts_dir.join("lm_contexts.json");
    let lm_contexts = if lm_file.exists() {
        let lm_content = std::fs::read_to_string(&lm_file)?;
        serde_json::from_str::<HashMap<UserId, UserContext>>(&lm_content)?
    } else {
        HashMap::new()
    };

    // Load Reason contexts
    let reason_file = contexts_dir.join("reason_contexts.json");
    let reason_contexts = if reason_file.exists() {
        let reason_content = std::fs::read_to_string(&reason_file)?;
        serde_json::from_str::<HashMap<UserId, UserContext>>(&reason_content)?
    } else {
        HashMap::new()
    };

    println!("📂 Loaded {} LM contexts and {} Reason contexts from disk", 
        lm_contexts.len(), reason_contexts.len());
    
    Ok((lm_contexts, reason_contexts))
}







// TypeMap key for LM chat context
pub struct LmContextMap;
impl TypeMapKey for LmContextMap {
    type Value = HashMap<UserId, UserContext>;
}

// TypeMap key for Reason chat context
pub struct ReasonContextMap;
impl TypeMapKey for ReasonContextMap {
    type Value = HashMap<UserId, UserContext>;
}







// TypeMap key for storing user conversation histories to enable context-aware queries about other users' conversations
pub struct UserConversationHistoryMap;
impl TypeMapKey for UserConversationHistoryMap {
    type Value = HashMap<UserId, Vec<ChatMessage>>;
}

// Import all command constants generated by the #[command] macro
use crate::help::HELP_COMMAND;
use crate::ping::PING_COMMAND;
use crate::echo::ECHO_COMMAND;
use crate::profilepfp::PPFP_COMMAND;
use crate::lm::{LM_COMMAND, CLEARCONTEXT_COMMAND};
use crate::reason::REASON_COMMAND;
use crate::sum::SUM_COMMAND;


// Command group declaration - includes all available commands
#[group]
#[commands(ping, echo, help, ppfp, lm, clearcontext, reason, sum)]
struct General;

// Event handler implementation
struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        // log::info!("✅ Bot connected as {}! (ID: {})", ready.user.name, ready.user.id);
        // log::info!("📊 Connected to {} guilds", ready.guilds.len());
        println!("✅ Bot connected as {}!", ready.user.name);
    }

    async fn message(&self, ctx: Context, msg: Message) {
        // Only respond to direct user ID mentions
        let bot_user_id = "1385309017881968761";
        let is_mentioned_by_id = msg.content.contains(&format!("<@{}>", bot_user_id));
        
        if is_mentioned_by_id {
            let bot_id = ctx.cache.current_user_id();
            
            println!("[MAIN] Bot mentioned via user ID - Raw message content: '{}'", msg.content);
            
            // Extract the prompt after removing the user ID mention
            let prompt = msg.content
                .replace(&format!("<@{}>", bot_user_id), "")
                .trim()
                .to_string();
                
            // Check if this is a reply to another message
            if let Some(referenced) = &msg.referenced_message {
                // Check if the referenced message was authored by the bot itself
                // Allow responses to bot's own messages, but add some basic loop prevention
                if referenced.author.id == bot_id {
                    println!("[MAIN] User is asking about bot's own message - allowing response with loop prevention");
                    
                    // Basic loop prevention: don't respond if the bot's message was very recent (< 5 seconds)
                    // This prevents rapid back-and-forth but allows legitimate questions about bot responses
                    let message_age = msg.timestamp.timestamp() - referenced.timestamp.timestamp();
                    if message_age < 5 {
                        println!("[MAIN] Bot's referenced message is too recent ({} seconds) - preventing potential rapid loop", message_age);
                        let _ = msg.reply(&ctx, "Please wait a moment before asking about my recent response to avoid loops.").await;
                        return;
                    }
                }
                    
                println!("[MAIN] User ID mention used in reply to {}", referenced.author.name);
                println!("[MAIN] Prompt: '{}'", prompt);
                
                // Check for -v flag - directly invoke lm command for proper handling
                if prompt.starts_with("-v") || prompt.starts_with("--vision") {
                    println!("[MAIN] Detected vision request in user ID mention");
                    println!("[MAIN] User {} requesting vision analysis", msg.author.name);
                    println!("[MAIN] Invoking lm command directly for proper reply and attachment handling");
                    
                    // Create Args from the prompt and call lm command directly
                    let args = Args::new(&prompt, &[Delimiter::Single(' ')]);
                    if let Err(e) = lm::lm(&ctx, &msg, args).await {
                        println!("[MAIN] Vision request failed with error: {}", e);
                        let _ = msg.reply(&ctx, format!("Vision analysis error: {}", e)).await;
                    } else {
                        println!("[MAIN] Vision request completed successfully");
                    }
                } else {
                    // RAG-enhanced LM handling for user ID mentions in replies
                    let rag_input = format!(
                        "CONTEXT: The user {} is asking you about a message posted by {}.\n\n\
                        ORIGINAL MESSAGE BY {}:\n\
                        \"{}\"\n\n\
                        USER'S QUESTION ABOUT THIS MESSAGE:\n\
                        \"{}\"\n\n\
                        Please respond to {}'s question specifically about {}'s message above. \
                        Reference the original message content when relevant to provide context and clarity.",
                        msg.author.name,
                        referenced.author.name,
                        referenced.author.name,
                        referenced.content,
                        prompt,
                        msg.author.name,
                        referenced.author.name
                    );
                    
                    println!("[MAIN] Processing user ID mention RAG request in reply");
                    println!("[MAIN] User {} asking about message from {}", msg.author.name, referenced.author.name);
                    println!("[MAIN] RAG input: '{}'", rag_input);
                        
                    if let Err(e) = lm::handle_lm_request(&ctx, &msg, &rag_input).await {
                        println!("[MAIN] User ID mention RAG request failed: {}", e);
                        let _ = msg.reply(&ctx, format!("LM error: {}", e)).await;
                    }
                }
            } else {
                // Direct user ID mention without reply - treat as regular lm command
                println!("[MAIN] Direct user ID mention without reply from user {}", msg.author.name);
                println!("[MAIN] Prompt: '{}'", prompt);
                
                if prompt.is_empty() {
                    let _ = msg.reply(&ctx, "Please provide a prompt! Usage: `<@Meri_> <your prompt>`\n\nTo ask about a specific message, reply to that message with your question.").await;
                    return;
                }
                
                // Check for -v flag in direct mentions - directly invoke lm command for proper handling
                if prompt.starts_with("-v") || prompt.starts_with("--vision") {
                    println!("[MAIN] Detected vision request in direct user ID mention");
                    println!("[MAIN] User {} requesting vision analysis", msg.author.name);
                    println!("[MAIN] Invoking lm command directly for proper attachment handling");
                    
                    // Create Args from the prompt and call lm command directly
                    let args = Args::new(&prompt, &[Delimiter::Single(' ')]);
                    if let Err(e) = lm::lm(&ctx, &msg, args).await {
                        println!("[MAIN] Vision request failed with error: {}", e);
                        let _ = msg.reply(&ctx, format!("Vision analysis error: {}", e)).await;
                    } else {
                        println!("[MAIN] Vision request completed successfully");
                    }
                } else {
                    // Add context for regular direct mentions
                    let direct_input = format!(
                        "CONTEXT: The user {} is asking you a direct question (not about a specific message).\n\n\
                        USER'S QUESTION:\n\
                        \"{}\"\n\n\
                        Please respond to {}'s question directly.",
                        msg.author.name,
                        prompt,
                        msg.author.name
                    );
                    
                    if let Err(e) = lm::handle_lm_request(&ctx, &msg, &direct_input).await {
                        println!("[MAIN] Direct user ID mention request failed: {}", e);
                        let _ = msg.reply(&ctx, format!("LM error: {}", e)).await;
                    }
                }
            }
        }
    }
}

// Function to read configuration from botconfig.txt with multi-path fallback
async fn handle_command_line(shutdown_tx: mpsc::Sender<String>) {
    use tokio::io::AsyncWriteExt;
    use tokio::time::{sleep, Duration};
    
    println!("📝 Command line interface active. Type 'help' for available commands.");
    
    // Wait for bot to connect and show connection messages before showing prompt
    sleep(Duration::from_millis(1500)).await;
    
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let mut stdout = io::stdout();
    
    // Show initial prompt after bot startup messages
    if let Err(_) = stdout.write_all(b"\n> ").await {
        eprintln!("❌ Failed to write initial prompt");
        return;
    }
    if let Err(_) = stdout.flush().await {
        eprintln!("❌ Failed to flush initial prompt");
        return;
    }
    
    loop {
        // Read command
        match reader.next_line().await {
            Ok(Some(line)) => {
                let command = line.trim().to_lowercase();
                
                match command.as_str() {
                    "quit" | "q" | "exit" => {
                        println!("⏹️  Shutting down bot...");
                        if let Err(_) = shutdown_tx.send("quit".to_string()).await {
                            eprintln!("❌ Failed to send shutdown signal");
                        }
                        break;
                    }
                    "help" | "h" => {
                        println!("🤖 Available commands:");
                        println!("  quit, q, exit  - Stop the bot gracefully");
                        println!("  help, h        - Show this help message");
                        println!("  status         - Show bot status");
                    }
                    "status" => {
                        println!("🤖 Bot Status: Running");
                        println!("📡 Discord connection: Active");
                        println!("💬 Command interface: Active");
                    }
                    "" => {
                        // Empty line, do nothing
                    }
                    _ => {
                        println!("❓ Unknown command: '{}'. Type 'help' for available commands.", command);
                    }
                }
                
                // Display prompt for next command (unless quitting)
                if !matches!(command.as_str(), "quit" | "q" | "exit") {
                    if let Err(_) = stdout.write_all(b"> ").await {
                        eprintln!("❌ Failed to write prompt");
                        break;
                    }
                    if let Err(_) = stdout.flush().await {
                        eprintln!("❌ Failed to flush prompt");
                        break;
                    }
                }
            }
            Ok(None) => {
                // EOF reached
                break;
            }
            Err(e) => {
                eprintln!("❌ Error reading command line: {}", e);
                break;
            }
        }
    }
}

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
                
                println!("✅ Configuration loaded from {}", config_path);
                return Ok(config);
            }
            Err(_) => {
                // Try next path
                continue;
            }
        }
    }
    
    Err("No botconfig.txt file found in any expected location (., .., ../.., src/)".to_string())
}

#[tokio::main]
async fn main() {
    // Initialize logger - must be done before any logging calls
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error"))
        .format_timestamp_secs()
        .init();
    
    // log::info!("🚀 Meri Bot starting up...");
    
    // Load configuration from botconfig.txt file
    match load_bot_config() {
        Ok(_) => {
            // log::info!("✅ Configuration loaded from botconfig.txt");
            println!("✅ Configuration loaded from botconfig.txt");
        },
        Err(error) => {
            log::error!("❌ Failed to load botconfig.txt: {}", error);
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
                log::error!("❌ DISCORD_TOKEN in botconfig.txt is set to placeholder value");
                eprintln!("❌ DISCORD_TOKEN in botconfig.txt is set to placeholder! Replace with your actual Discord bot token.");
                return;
            }
            // log::debug!("✅ Discord token validated (length: {} chars)", token.len());
            token
        }
        Err(_) => {
            log::error!("❌ DISCORD_TOKEN not found in botconfig.txt file");
            eprintln!("❌ DISCORD_TOKEN not found in botconfig.txt file!");
            return;
        }
    };
    
    // Get command prefix from configuration
    let prefix = env::var("PREFIX").unwrap_or_else(|_| "^".to_string());
    // log::info!("🤖 Starting bot with prefix: '{}'", prefix);
    println!("🤖 Starting bot with prefix: '{}'", prefix);
    
    // Set up command framework
    // log::debug!("🔧 Setting up command framework with prefix: '{}'", prefix);
    let framework = StandardFramework::new()
        .configure(|c| {
            c.prefix(&prefix)
            .case_insensitivity(true)
            .no_dm_prefix(true)
            .with_whitespace(true)
        })
        .after(|_ctx, msg, command_name, result| Box::pin(async move {
            match result {
                Ok(()) => {
                    // log::info!("✅ Command '{}' executed successfully by user {} ({})", 
                    //           command_name, msg.author.name, msg.author.id);
                },
                Err(e) => {
                    log::error!("❌ Command '{}' failed for user {} ({}): {:?}", 
                               command_name, msg.author.name, msg.author.id, e);
                }
            }
        }))
        .unrecognised_command(|_ctx, msg, unrecognized_command_name| Box::pin(async move {
            // log::warn!("❓ Unrecognized command '{}' attempted by user {} ({})", 
            //           unrecognized_command_name, msg.author.name, msg.author.id);
        }))
        .group(&GENERAL_GROUP);

    // Configure bot intents
    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::MESSAGE_CONTENT;

    // Create and start client
    // log::info!("🔧 Creating Discord client...");
    let mut client = match Client::builder(token, intents)
        .event_handler(Handler)
        .framework(framework)
        .await
    {
        Ok(client) => {
            // log::info!("✅ Discord client created successfully");
            client
        },
        Err(e) => {
            log::error!("❌ Error creating Discord client: {:?}", e);
            eprintln!("❌ Error creating Discord client: {:?}", e);
            eprintln!("Check your token in botconfig.txt file");
            return;
        }
    };

    // Initialize per-user context maps for LM and Reason commands
    {
        // log::debug!("🔧 Initializing context maps...");
        let mut data = client.data.write().await;
        
        // Load existing contexts from disk
        match load_contexts_from_disk().await {
            Ok((lm_contexts, reason_contexts)) => {
                data.insert::<LmContextMap>(lm_contexts);
                data.insert::<ReasonContextMap>(reason_contexts);
                println!("✅ Context maps initialized with persistent data");
            }
            Err(e) => {
                println!("⚠️  Failed to load contexts from disk: {}", e);
                println!("🔧 Initializing with empty context maps");
                data.insert::<LmContextMap>(HashMap::new());
                data.insert::<ReasonContextMap>(HashMap::new());
            }
        }
        
        data.insert::<UserConversationHistoryMap>(HashMap::new());
        // log::debug!("✅ Context maps initialized");
    }

    // Set up command line interface for graceful shutdown
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<String>(1);
    
    // Start command line handler
    let cmd_shutdown_tx = shutdown_tx.clone();
    let cmd_task = tokio::spawn(async move {
        handle_command_line(cmd_shutdown_tx).await;
    });
    
    // Set up graceful shutdown on CTRL+C or command line
    println!("🚀 Bot is running...");
    println!("💡 Use 'quit' command to stop gracefully, or press Ctrl+C");
    tokio::select! {
        _ = signal::ctrl_c() => {
            // log::info!("📡 Received SIGINT, stopping bot gracefully...");
            println!("\n⏹️ Stopping bot gracefully...");
        }
        shutdown_signal = shutdown_rx.recv() => {
            if let Some(signal) = shutdown_signal {
                println!("📡 Received '{}' command, stopping bot gracefully...", signal);
            }
        }
        result = client.start() => {
            if let Err(why) = result {
                log::error!("❌ Client error: {:?}", why);
                eprintln!("❌ Client error: {:?}", why);
            }
        }
    }
    
    // Save contexts to disk before shutting down
    {
        let data = client.data.read().await;
        let lm_contexts = data.get::<LmContextMap>().cloned().unwrap_or_default();
        let reason_contexts = data.get::<ReasonContextMap>().cloned().unwrap_or_default();
        
        if let Err(e) = save_contexts_to_disk(&lm_contexts, &reason_contexts).await {
            eprintln!("⚠️  Failed to save contexts to disk: {}", e);
        }
    }
    
    // Stop the command line task
    cmd_task.abort();
    
    // log::info!("👋 Bot shutdown complete");
    
    println!("✅ Bot stopped");
}
