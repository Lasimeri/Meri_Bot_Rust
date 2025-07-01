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

// Command group declaration
#[group]
#[commands(ping, echo, help)]
struct General;

// Event handler implementation
struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("Bot connected as {}!", ready.user.name);
    }
}

// Command implementations
#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, "Pong! 🏓").await?;
    Ok(())
}

#[command]
async fn echo(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let text = args.message();
    msg.reply(ctx, text).await?;
    Ok(())
}

#[command]
async fn help(ctx: &Context, msg: &Message) -> CommandResult {
    let prefix = env::var("PREFIX").unwrap_or_else(|_| "!".to_string());
    msg.reply(ctx, &format!("**Commands:**\n`{0}ping` - Check latency\n`{0}echo <text>` - Repeat text\n`{0}help` - Show this help", prefix)).await?;
    Ok(())
}

// Main bot function
pub async fn run() {
    // Try to load environment variables from different locations
    if let Err(_) = dotenv::from_filename(".env") {
        if let Err(_) = dotenv::from_filename("src/.env") {
            println!("Warning: Could not load .env file from root or src directory");
        } else {
            println!("Successfully loaded .env file from src directory");
        }
    } else {
        println!("Successfully loaded .env file from root directory");
    }
    
    // Debug: Print all environment variables that start with DISCORD or PREFIX
    for (key, value) in env::vars() {
        if key.starts_with("DISCORD") || key.starts_with("PREFIX") {
            println!("Found env var: {}={}", key, if key.contains("TOKEN") { "[HIDDEN]" } else { &value });
        }
    }
    
    // Get token from environment variable
    let token = env::var("DISCORD_TOKEN")
        .expect("Expected DISCORD_TOKEN in the environment. Make sure your .env file contains DISCORD_TOKEN=your_token_here");
    
    // Get prefix from environment variable (default to "!" if not set)
    let prefix = env::var("PREFIX").unwrap_or_else(|_| "!".to_string());
    println!("Using prefix: {}", prefix);
    
    // Set up command framework
    let framework = StandardFramework::new()
        .configure(|c| c.prefix(&prefix).case_insensitivity(true))
        .group(&GENERAL_GROUP);

    // Configure bot intents
    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::MESSAGE_CONTENT;

    // Create and start client
    let mut client = Client::builder(token, intents)
        .event_handler(Handler)
        .framework(framework)
        .await
        .expect("Error creating client");

    // Start listening for events
    if let Err(why) = client.start().await {
        eprintln!("Client error: {:?}", why);
    }
} 