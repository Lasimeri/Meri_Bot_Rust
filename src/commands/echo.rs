// echo.rs - Echo Command Module
// This module implements the ^echo command, which simply repeats back user input for testing purposes.
//
// Key Features:
// - Echoes user-provided text
// - Provides usage guidance if no text is given
//
// Used by: main.rs (command registration)

use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};

#[command]
/// Main ^echo command handler
/// Echoes back the user's input text
/// Supports:
///   - ^echo <text>
pub async fn echo(ctx: &Context, msg: &Message, args: Args) -> CommandResult {

    let text = args.message();
    // If no text is provided, reply with usage guidance
    if text.is_empty() {
        msg.reply(ctx, "Please provide text to echo!").await?;
    } else {
        // Echo the provided text
        msg.reply(ctx, text).await?;
    }
    Ok(())
} 