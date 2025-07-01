use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};
use std::fs;
use crate::lm::{load_lm_config, buffer_chat_response, send_buffered_response, ChatMessage};

#[command]
#[aliases("reasoning")]
pub async fn reason(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let prompt = args.message().trim();
    
    // Start typing indicator
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    
    if prompt.is_empty() {
        msg.reply(ctx, "❌ Please provide a reasoning prompt! Usage: `^reason <your reasoning question>`").await?;
        return Ok(());
    }

    // Load LM Studio configuration
    let config = match load_lm_config().await {
        Ok(config) => config,
        Err(e) => {
            eprintln!("❌ Failed to load LM Studio configuration: {}", e);
            msg.reply(ctx, &format!("❌ LM Studio configuration error: {}\n\nMake sure `lmapiconf.txt` exists and contains all required settings. Check `example_lmapiconf.txt` for reference.", e)).await?;
            return Ok(());
        }
    };

    // Load reasoning system prompt
    let system_prompt = match load_reasoning_system_prompt().await {
        Ok(prompt) => prompt,
        Err(e) => {
            eprintln!("❌ Failed to load reasoning system prompt: {}", e);
            // Fallback to a default reasoning prompt if file doesn't exist
            "You are an advanced AI reasoning assistant. Think step-by-step through problems and provide detailed, logical explanations. Break down complex questions into smaller parts and explain your reasoning process clearly.".to_string()
        }
    };

    // Send initial "thinking" message
    let mut current_msg = match msg.channel_id.send_message(&ctx.http, |m| {
        m.content("🧠 Reasoning through your question...")
    }).await {
        Ok(message) => message,
        Err(e) => {
            eprintln!("❌ Failed to send initial message: {}", e);
            msg.reply(ctx, "❌ Failed to send message!").await?;
            return Ok(());
        }
    };

    // Prepare messages for the API with reasoning-specific system prompt
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        },
    ];

    // Buffer the complete response from streaming using the reasoning model
    match buffer_chat_response(messages, &config.default_reason_model, &config, ctx, &mut current_msg).await {
        Ok(response_content) => {
            // Send the complete buffered response in Discord-sized chunks
            if let Err(e) = send_buffered_response(ctx, &mut current_msg, &response_content, &config).await {
                eprintln!("❌ Failed to send response chunks: {}", e);
                let _ = current_msg.edit(&ctx.http, |m| {
                    m.content("❌ Failed to send complete response!")
                }).await;
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to get response from reasoning model: {}", e);
            let _ = current_msg.edit(&ctx.http, |m| {
                m.content("❌ Failed to get response from reasoning model!")
            }).await;
        }
    }

    Ok(())
}

// Helper function to load reasoning-specific system prompt from file
async fn load_reasoning_system_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Try to load reasoning-specific prompt first, fall back to general system prompt
    let reasoning_prompt_paths = [
        "reasoning_prompt.txt",
        "system_prompt.txt",
    ];
    
    for path in &reasoning_prompt_paths {
        if let Ok(content) = fs::read_to_string(path) {
            return Ok(content.trim().to_string());
        }
    }
    
    Err("No reasoning prompt file found".into())
} 