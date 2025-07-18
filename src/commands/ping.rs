// ping.rs - Ping Command Module
// This module implements the ^ping command, which measures and displays the bot's response time.
//
// Key Features:
// - Measures round-trip latency for Discord message handling
// - Provides immediate feedback to users
//
// Used by: main.rs (command registration)

// ============================================================================
// IMPORTS
// ============================================================================

use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};

// ============================================================================
// COMMAND IMPLEMENTATION
// ============================================================================

#[command]
/// Main ^ping command handler
/// Measures and displays the bot's response time in milliseconds
/// Supports:
///   - ^ping
pub async fn ping(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let start_time = std::time::Instant::now();
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    
    // Send the initial response and measure the time
    let response_result = msg.reply(ctx, "Pong! Calculating delay...").await;
    let elapsed = start_time.elapsed();
    
    // Update the message with the actual ping time
    if let Ok(mut response_msg) = response_result {
        let ping_ms = elapsed.as_millis();
        let updated_content = format!("Pong! Response time: {}ms", ping_ms);
        
        if let Err(e) = response_msg.edit(&ctx.http, |m| m.content(updated_content)).await {
            eprintln!("[PING] Failed to update ping message with delay: {}", e);
            // If edit fails, at least we sent the initial response
        }
    }
    
    Ok(())
} 
// ============================================================================
// TODO: Add more diagnostics or latency breakdown in future versions
// ============================================================================ 