// lm.rs - Language Model (AI Chat) Command Module
// This module implements the ^lm command, providing AI chat functionality with LM Studio.
// It follows the same pattern as reason.rs for configuration and model handling.
//
// Key Features:
// - Simple AI chat with streaming responses
// - Configuration loading from lmapiconf.txt
// - System prompt loading from system_prompt.txt
// - Per-user context management
// - Clean error handling
// - Proper message chunking for Discord limits
//
// Used by: main.rs (command registration)

use serenity::{
    client::Context,
    framework::standard::{macros::command, macros::group, Args, CommandResult},
    model::channel::Message,
};
use std::fs;
use serde::{Deserialize, Serialize};
use futures_util::StreamExt;
use crate::LmContextMap; // TypeMap key defined in main.rs
use crate::commands::search::{ChatMessage, LMConfig, load_lm_config}; // Use from search module

// API structures for chat completion
#[derive(Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub max_tokens: i32,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
}

// Multimodal chat request for vision
#[derive(Serialize)]
pub struct MultimodalChatRequest {
    pub model: String,
    pub messages: Vec<MultimodalChatMessage>,
    pub temperature: f32,
    pub max_tokens: i32,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
}

#[derive(Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize)]
pub struct Choice {
    pub delta: Option<Delta>,
    pub finish_reason: Option<String>,
}

#[derive(Deserialize)]
pub struct Delta {
    pub content: Option<String>,
}

// Message state for streaming
pub struct MessageState {
    pub current_content: String,
    pub current_message: Message,
    pub message_index: usize,
    pub char_limit: usize,
    pub total_messages: Vec<Message>, // Track all messages for multi-part responses
}

// Multimodal structures for vision support
#[derive(Serialize, Deserialize, Clone)]
pub struct MultimodalChatMessage {
    pub role: String,
    pub content: Vec<MessageContent>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum MessageContent {
    Text { 
        #[serde(rename = "type")] 
        content_type: String, 
        text: String 
    },
    Image { 
        #[serde(rename = "type")] 
        content_type: String, 
        image_url: ImageUrl 
    },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ImageUrl {
    pub url: String,
}

// Streaming statistics
#[derive(Debug)]
pub struct StreamingStats {
    pub total_characters: usize,
    pub message_count: usize,
}

#[command]
#[aliases("llm", "ai", "chat")]
/// Main ^lm command handler
/// Handles user prompts for AI chat
pub async fn lm(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let input = args.message().trim();
    
    if input.is_empty() {
        msg.reply(ctx, "Please provide a prompt! Usage: `^lm <your prompt>`").await?;
        return Ok(());
    }

    // Handle special flags
    if input == "--test" || input == "-t" {
        return test_connectivity(ctx, msg).await;
    }

    if input == "--clear" || input == "-c" {
        return clear_context(ctx, msg).await;
    }

    if input == "--models" || input == "-models" {
        return list_models(ctx, msg).await;
    }

    // Handle search flag
    if input.starts_with("-s ") || input.starts_with("--search ") {
        let query = if input.starts_with("-s ") {
            &input[3..]
        } else {
            &input[9..]
        };
        
        if query.trim().is_empty() {
            msg.reply(ctx, "Please provide a search query! Usage: `^lm -s <query>`").await?;
            return Ok(());
        }

        // Delegate to search functionality
        return handle_search(ctx, msg, query).await;
    }

    // Handle vision flag
    if input.starts_with("-v ") || input.starts_with("--vision ") {
        let prompt = if input.starts_with("-v ") {
            &input[3..]
        } else {
            &input[9..]
        };
        
        if prompt.trim().is_empty() {
            msg.reply(ctx, "Please provide a prompt for vision analysis! Usage: `^lm -v <prompt>` with image attached.").await?;
            return Ok(());
        }

        // Check for image attachment
        if msg.attachments.is_empty() {
            msg.reply(ctx, "Please attach an image for vision analysis!").await?;
            return Ok(());
        }

        let attachment = &msg.attachments[0];
        if !attachment.content_type.as_deref().unwrap_or("").starts_with("image/") {
            msg.reply(ctx, "Please attach a valid image file!").await?;
            return Ok(());
        }

        // Delegate to vision functionality
        return crate::commands::vis::handle_vision_request(ctx, msg, prompt, attachment).await;
    }

    // Load configuration
    let config = match load_lm_config().await {
        Ok(cfg) => cfg,
        Err(e) => {
            msg.reply(ctx, &format!("❌ Configuration error: {}", e)).await?;
            return Ok(());
        }
    };

    // Load system prompt
    let system_prompt = match load_system_prompt().await {
        Ok(prompt) => prompt,
        Err(e) => {
            msg.reply(ctx, &format!("❌ Failed to load system prompt: {}", e)).await?;
            return Ok(());
        }
    };

    // Build messages with context
    let mut messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        }
    ];

    // Add conversation history from context
    {
        let data_map = ctx.data.read().await;
        if let Some(lm_map) = data_map.get::<LmContextMap>() {
            if let Some(context) = lm_map.get(&msg.author.id) {
                for msg in context.get_conversation_messages() {
                    messages.push(msg.clone());
                }
            }
        }
    }

    // Add current user message
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: input.to_string(),
    });

    // Record user message in context
    {
        let mut data_map = ctx.data.write().await;
        let lm_map = data_map.get_mut::<LmContextMap>()
            .expect("LM context map not initialized");
        let context = lm_map.entry(msg.author.id)
            .or_insert_with(crate::UserContext::new);
        context.add_user_message(ChatMessage {
            role: "user".to_string(),
            content: input.to_string(),
        });
    }

    // Send initial message
    let mut response_msg = msg.channel_id.send_message(&ctx.http, |m| {
        m.content("🤔 **AI is thinking...**")
    }).await?;

    // Stream the response
    match stream_chat_response(messages, &config, ctx, &mut response_msg).await {
        Ok(full_response_content) => {
            // Record assistant response in context with the full content
            let mut data_map = ctx.data.write().await;
            let lm_map = data_map.get_mut::<LmContextMap>()
                .expect("LM context map not initialized");
            if let Some(context) = lm_map.get_mut(&msg.author.id) {
                context.add_assistant_message(ChatMessage {
                    role: "assistant".to_string(),
                    content: full_response_content,
                });
            }
        }
        Err(e) => {
            let _ = response_msg.edit(&ctx.http, |m| {
                m.content(&format!("❌ Error: {}", e))
            }).await;
        }
    }

    Ok(())
}



// Load system prompt from file
async fn load_system_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let prompt_paths = [
        "system_prompt.txt",
        "../system_prompt.txt",
        "../../system_prompt.txt",
        "src/system_prompt.txt",
    ];
    
    for path in &prompt_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    Err("system_prompt.txt not found in any expected location".into())
}

// Stream chat response
async fn stream_chat_response(
    messages: Vec<ChatMessage>,
    config: &LMConfig,
    ctx: &Context,
    initial_msg: &mut Message,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout))
        .build()?;

    let chat_request = ChatRequest {
        model: config.default_model.clone(),
        messages,
        temperature: config.default_temperature,
        max_tokens: config.default_max_tokens,
        stream: true,
        seed: config.default_seed,
    };

    let api_url = format!("{}/v1/chat/completions", config.base_url);
    
    let response = client
        .post(&api_url)
        .json(&chat_request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        
        // Check for specific errors
        if error_text.contains("No models loaded") || error_text.contains("model_not_found") {
            return Err(format!(
                "Model '{}' is not loaded in LM Studio. Please load the model and try again.",
                config.default_model
            ).into());
        }
        
        return Err(format!("API error: {} - {}", status, error_text).into());
    }

    // Stream the response
    let mut stream = response.bytes_stream();
    let mut accumulated_content = String::new();
    let mut line_buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        line_buffer.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(i) = line_buffer.find('\n') {
            let line = line_buffer.drain(..=i).collect::<String>();
            let line = line.trim();

            if let Some(json_str) = line.strip_prefix("data: ") {
                if json_str.trim() == "[DONE]" {
                    break;
                }

                if let Ok(response) = serde_json::from_str::<ChatResponse>(json_str) {
                    for choice in response.choices {
                        if let Some(delta) = choice.delta {
                            if let Some(content) = delta.content {
                                accumulated_content.push_str(&content);
                            }
                        }
                    }
                }
            }
        }
    }

    // Split content into Discord-friendly chunks
    let chunks = split_message(&accumulated_content, config.max_discord_message_length - config.response_format_padding);
    
    // Handle multiple messages if content is too long
    if chunks.len() == 1 {
        // Single message - update the initial message
        let formatted_content = format!(
            "**AI Response:**\n```\n{}\n```",
            chunks[0]
        );
        
        initial_msg.edit(&ctx.http, |m| {
            m.content(&formatted_content)
        }).await?;
    } else {
        // Multiple messages - update first message and send additional ones
        for (i, chunk) in chunks.iter().enumerate() {
            let formatted_content = if chunks.len() == 1 {
                format!("**AI Response:**\n```\n{}\n```", chunk)
            } else {
                format!("**AI Response (Part {}/{})**\n```\n{}\n```", i + 1, chunks.len(), chunk)
            };
            
            if i == 0 {
                // Update the first message
                initial_msg.edit(&ctx.http, |m| {
                    m.content(&formatted_content)
                }).await?;
            } else {
                // Send additional messages for remaining chunks
                initial_msg.channel_id.send_message(&ctx.http, |m| {
                    m.content(&formatted_content)
                }).await?;
            }
        }
    }

    // Return the full accumulated content for context storage
    Ok(accumulated_content)
}

/// Split message content into Discord-friendly chunks
fn split_message(content: &str, max_len: usize) -> Vec<String> {
    let lines: Vec<&str> = content.lines().collect();
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    
    for line in lines {
        let potential_chunk = if current_chunk.is_empty() {
            line.to_string()
        } else {
            format!("{}\n{}", current_chunk, line)
        };
        
        if potential_chunk.len() <= max_len {
            current_chunk = potential_chunk;
        } else {
            if !current_chunk.is_empty() {
                chunks.push(current_chunk.clone());
            }
            current_chunk = line.to_string();
        }
    }
    
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }
    
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_message_short_content() {
        let short_content = "This is a short message that should fit in one chunk.";
        let chunks = split_message(short_content, 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], short_content);
    }

    #[test]
    fn test_split_message_long_content() {
        let long_content = "This is the first line.\nThis is the second line.\nThis is the third line that should be split.\nThis is the fourth line to test chunking.";
        let chunks = split_message(&long_content, 50);
        assert!(chunks.len() > 1, "Long content should be split into multiple chunks");
        
        // Test that each chunk is within the limit
        for chunk in &chunks {
            assert!(chunk.len() <= 50, "Chunk exceeds maximum length: {}", chunk.len());
        }
    }

    #[test]
    fn test_split_message_single_long_line() {
        let single_long_line = "This is a very long line that should not be split because it exceeds the maximum length but we want to keep it as one chunk for testing purposes.";
        let chunks = split_message(single_long_line, 50);
        assert_eq!(chunks.len(), 1, "Single long line should remain as one chunk");
    }

    #[test]
    fn test_split_message_empty_content() {
        let chunks = split_message("", 100);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_split_message_exact_limit() {
        let content = "This is exactly 25 characters";
        let chunks = split_message(content, 25);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], content);
    }
}

// Update Discord message (simplified for single message updates)
async fn update_message(
    state: &mut MessageState,
    ctx: &Context,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let formatted_content = format!(
        "**Part {}:**\n```\n{}\n```",
        state.message_index,
        state.current_content
    );

    state.current_message.edit(&ctx.http, |m| {
        m.content(&formatted_content)
    }).await?;

    Ok(())
}

// Public wrapper for update_message (for vis.rs compatibility)
pub async fn update_chat_message(
    state: &mut MessageState,
    new_content: &str,
    ctx: &Context,
    _config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    state.current_content.push_str(new_content);
    update_message(state, ctx).await
}

// Public wrapper for finalize_message (for vis.rs compatibility)
pub async fn finalize_chat_message(
    state: &mut MessageState,
    remaining_content: &str,
    ctx: &Context,
    _config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !remaining_content.is_empty() {
        state.current_content.push_str(remaining_content);
    }
    update_message(state, ctx).await
}

// Test connectivity command
async fn test_connectivity(ctx: &Context, msg: &Message) -> CommandResult {
    let config = match load_lm_config().await {
        Ok(cfg) => cfg,
        Err(e) => {
            msg.reply(ctx, &format!("❌ Configuration error: {}", e)).await?;
            return Ok(());
        }
    };

    let mut test_msg = msg.channel_id.send_message(&ctx.http, |m| {
        m.content("🔍 Testing API connectivity...")
    }).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    match client.get(&config.base_url).send().await {
        Ok(_) => {
            let _ = test_msg.edit(&ctx.http, |m| {
                m.content(&format!(
                    "✅ **Connection Successful**\n\n\
                    **Server:** {}\n\
                    **Model:** {}\n\
                    **Temperature:** {}\n\
                    **Max Tokens:** {}",
                    config.base_url,
                    config.default_model,
                    config.default_temperature,
                    config.default_max_tokens
                ))
            }).await;
        }
        Err(e) => {
            let _ = test_msg.edit(&ctx.http, |m| {
                m.content(&format!("❌ **Connection Failed**\n\nError: {}", e))
            }).await;
        }
    }

    Ok(())
}

// Clear context command
async fn clear_context(ctx: &Context, msg: &Message) -> CommandResult {
    let mut data_map = ctx.data.write().await;
    let lm_map = data_map.get_mut::<LmContextMap>()
        .expect("LM context map not initialized");
    
    if let Some(context) = lm_map.get_mut(&msg.author.id) {
        context.clear();
        msg.reply(ctx, "✅ Your conversation history has been cleared.").await?;
    } else {
        msg.reply(ctx, "ℹ️ You don't have any conversation history to clear.").await?;
    }

    Ok(())
}

// List available models
async fn list_models(ctx: &Context, msg: &Message) -> CommandResult {
    let config = match load_lm_config().await {
        Ok(cfg) => cfg,
        Err(e) => {
            msg.reply(ctx, &format!("❌ Configuration error: {}", e)).await?;
            return Ok(());
        }
    };

    let mut models_msg = msg.channel_id.send_message(&ctx.http, |m| {
        m.content("🔍 Checking available models...")
    }).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    
    let models_url = format!("{}/v1/models", config.base_url);
    
    match client.get(&models_url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                let models_json: serde_json::Value = response.json().await?;
                
                if let Some(data) = models_json.get("data").and_then(|d| d.as_array()) {
                    let mut models_list = String::new();
                    models_list.push_str("**Available Models** 📋\n\n");
                    
                    for (i, model) in data.iter().enumerate() {
                        if let Some(id) = model.get("id").and_then(|id| id.as_str()) {
                            let status = if id == config.default_model {
                                "✅ **CONFIGURED**"
                            } else {
                                "⚪"
                            };
                            models_list.push_str(&format!("{}. {} {}\n", i + 1, status, id));
                        }
                    }
                    
                    models_list.push_str(&format!("\n**Current Configuration:**\n• Default Model: `{}`\n• Server: `{}`", 
                        config.default_model, config.base_url));
                    
                    let _ = models_msg.edit(&ctx.http, |m| {
                        m.content(&models_list)
                    }).await;
                } else {
                    let _ = models_msg.edit(&ctx.http, |m| {
                        m.content("❌ Invalid response format from server")
                    }).await;
                }
            } else {
                let _ = models_msg.edit(&ctx.http, |m| {
                    m.content(&format!("❌ Failed to get models: HTTP {}", response.status()))
                }).await;
            }
        }
        Err(e) => {
            let _ = models_msg.edit(&ctx.http, |m| {
                m.content(&format!("❌ Connection failed: {}", e))
            }).await;
        }
    }

    Ok(())
}

// Handle search functionality
async fn handle_search(ctx: &Context, msg: &Message, query: &str) -> CommandResult {
    let config = match load_lm_config().await {
        Ok(cfg) => cfg,
        Err(e) => {
            msg.reply(ctx, &format!("❌ Configuration error: {}", e)).await?;
            return Ok(());
        }
    };

    let mut search_msg = msg.channel_id.send_message(&ctx.http, |m| {
        m.content("🔍 Searching...")
    }).await?;

    // Use the search module's functionality
    match crate::commands::search::perform_ai_enhanced_search(query, &config, &mut search_msg, ctx).await {
        Ok(()) => {},
        Err(e) => {
            let _ = search_msg.edit(&ctx.http, |m| {
                m.content(&format!("❌ Search failed: {}", e))
            }).await;
        }
    }

    Ok(())
}

// Handle global LM request (when bot is mentioned)
pub async fn handle_lm_request_global(
    ctx: &Context,
    msg: &Message,
    input: &str,
    _original_prompt: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // For global context, we use a simplified approach
    // Just delegate to the regular lm command
    let args = Args::new(input, &[]);
    lm(ctx, msg, args).await?;
    Ok(())
}

// Logging initialization (stub for compatibility)
pub fn init_logging() -> Result<(), std::io::Error> {
    // Logging is handled by the main module
    Ok(())
}

// Command group exports
#[group]
#[commands(lm, clearcontext)]
pub struct Lm;

impl Lm {
    pub const fn new() -> Self {
        Lm
    }
}

#[command]
#[aliases("clearlm", "resetlm")]
/// Clear LM context command
pub async fn clearcontext(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    clear_context(ctx, msg).await
}

// Command definitions are automatically exported by the #[command] macro