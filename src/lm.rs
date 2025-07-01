use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::collections::HashMap;
use futures_util::StreamExt;

// Structure to track streaming statistics
#[derive(Debug)]
struct StreamingStats {
    total_characters: usize,
    message_count: usize,
}

// Structure to track current message state during streaming
struct MessageState {
    current_content: String,
    current_message: Message,
    message_index: usize,
    char_limit: usize,
}

// LM Studio API Configuration structure
#[derive(Debug, Clone)]
pub struct LMConfig {
    pub base_url: String,
    pub timeout: u64,
    pub default_model: String,
    pub default_reason_model: String,
    pub default_temperature: f32,
    pub default_max_tokens: i32,
    pub max_discord_message_length: usize,
    pub response_format_padding: usize,
}

// API Request/Response structures
#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: i32,
    stream: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    delta: Option<Delta>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Delta {
    content: Option<String>,
}

// Removed LoadModelRequest and UnloadModelRequest - LM Studio handles model management automatically

// Function to load LM Studio configuration from file
pub async fn load_lm_config() -> Result<LMConfig, Box<dyn std::error::Error + Send + Sync>> {
    let config_paths = [
        "lmapiconf.txt",
        "../lmapiconf.txt", 
        "../../lmapiconf.txt",
        "src/lmapiconf.txt"
    ];
    
    let mut content = String::new();
    let mut found_file = false;
    
    // Try to find the config file in multiple locations
    for config_path in &config_paths {
        match fs::read_to_string(config_path) {
            Ok(file_content) => {
                content = file_content;
                found_file = true;
                // Found the config file
                break;
            }
            Err(_) => {
                continue;
            }
        }
    }
    
    if !found_file {
        return Err("lmapiconf.txt file not found in any expected location (., .., ../.., src/)".into());
    }
    
    // Remove BOM if present
    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    let mut config_map = HashMap::new();

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
            config_map.insert(key, value);
        } else {
            // Skip invalid lines silently
        }
    }
    
    // Check for required keys
    let required_keys = [
        "LM_STUDIO_BASE_URL",
        "LM_STUDIO_TIMEOUT", 
        "DEFAULT_MODEL",
        "DEFAULT_REASON_MODEL",
        "DEFAULT_TEMPERATURE",
        "DEFAULT_MAX_TOKENS",
        "MAX_DISCORD_MESSAGE_LENGTH",
        "RESPONSE_FORMAT_PADDING"
    ];
    
    for key in &required_keys {
        if !config_map.contains_key(*key) {
            return Err(format!("❌ Required setting '{}' not found in lmapiconf.txt", key).into());
        }
    }
    
    // Create config - all values must be present in lmapiconf.txt
    let config = LMConfig {
        base_url: config_map.get("LM_STUDIO_BASE_URL")
            .ok_or("LM_STUDIO_BASE_URL not found in lmapiconf.txt")?.clone(),
        timeout: config_map.get("LM_STUDIO_TIMEOUT")
            .ok_or("LM_STUDIO_TIMEOUT not found in lmapiconf.txt")?
            .parse()
            .map_err(|_| "Invalid LM_STUDIO_TIMEOUT value in lmapiconf.txt")?,
        default_model: config_map.get("DEFAULT_MODEL")
            .ok_or("DEFAULT_MODEL not found in lmapiconf.txt")?.clone(),
        default_reason_model: config_map.get("DEFAULT_REASON_MODEL")
            .ok_or("DEFAULT_REASON_MODEL not found in lmapiconf.txt")?.clone(),
        default_temperature: config_map.get("DEFAULT_TEMPERATURE")
            .ok_or("DEFAULT_TEMPERATURE not found in lmapiconf.txt")?
            .parse()
            .map_err(|_| "Invalid DEFAULT_TEMPERATURE value in lmapiconf.txt")?,
        default_max_tokens: config_map.get("DEFAULT_MAX_TOKENS")
            .ok_or("DEFAULT_MAX_TOKENS not found in lmapiconf.txt")?
            .parse()
            .map_err(|_| "Invalid DEFAULT_MAX_TOKENS value in lmapiconf.txt")?,
        max_discord_message_length: config_map.get("MAX_DISCORD_MESSAGE_LENGTH")
            .ok_or("MAX_DISCORD_MESSAGE_LENGTH not found in lmapiconf.txt")?
            .parse()
            .map_err(|_| "Invalid MAX_DISCORD_MESSAGE_LENGTH value in lmapiconf.txt")?,
        response_format_padding: config_map.get("RESPONSE_FORMAT_PADDING")
            .ok_or("RESPONSE_FORMAT_PADDING not found in lmapiconf.txt")?
            .parse()
            .map_err(|_| "Invalid RESPONSE_FORMAT_PADDING value in lmapiconf.txt")?,
    };

    // Configuration loaded successfully
    
    Ok(config)
}

#[command]
#[aliases("llm", "ai", "chat")]
pub async fn lm(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let prompt = args.message().trim();
    
    // Start typing indicator
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    
    if prompt.is_empty() {
        msg.reply(ctx, "❌ Please provide a prompt! Usage: `^lm <your prompt>`").await?;
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

    // Load system prompt
    let system_prompt = match load_system_prompt().await {
        Ok(prompt) => prompt,
        Err(e) => {
            eprintln!("❌ Failed to load system prompt: {}", e);
            msg.reply(ctx, "❌ Failed to load system configuration!").await?;
            return Ok(());
        }
    };

    // Send initial "thinking" message
    let mut current_msg = match msg.channel_id.send_message(&ctx.http, |m| {
        m.content("🤖 Generating response...")
    }).await {
        Ok(message) => message,
        Err(e) => {
            eprintln!("❌ Failed to send initial message: {}", e);
            msg.reply(ctx, "❌ Failed to send message!").await?;
            return Ok(());
        }
    };

    // Prepare messages for the API
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

    // Log which model is being used for LM command
    println!("💬 LM command: Using model '{}' for chat", config.default_model);

    // Stream the response with real-time Discord post editing
    match stream_chat_response(messages, &config.default_model, &config, ctx, &mut current_msg).await {
        Ok(final_stats) => {
            println!("💬 LM command: Streaming complete - {} total characters across {} messages", 
                final_stats.total_characters, final_stats.message_count);
        }
        Err(e) => {
            eprintln!("❌ Failed to stream response from AI model: {}", e);
            let _ = current_msg.edit(&ctx.http, |m| {
                m.content("❌ Failed to get response from AI model!")
            }).await;
        }
    }

        Ok(())
}

// Main streaming function that handles real-time response with Discord message editing for chat
async fn stream_chat_response(
    messages: Vec<ChatMessage>,
    model: &str,
    config: &LMConfig,
    ctx: &Context,
    initial_msg: &mut Message,
) -> Result<StreamingStats, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout * 3))
        .build()?;
        
    let chat_request = ChatRequest {
        model: model.to_string(),
        messages,
        temperature: config.default_temperature,
        max_tokens: config.default_max_tokens,
        stream: true,
    };

    let response = client
        .post(&format!("{}/v1/chat/completions", config.base_url))
        .json(&chat_request)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("API request failed: HTTP {}", response.status()).into());
    }

    let mut stream = response.bytes_stream();
    let mut message_state = MessageState {
        current_content: String::new(),
        current_message: initial_msg.clone(),
        message_index: 1,
        char_limit: config.max_discord_message_length - config.response_format_padding,
    };
    
    let mut raw_response = String::new();
    let mut content_buffer = String::new();
    let mut last_update = std::time::Instant::now();
    let update_interval = std::time::Duration::from_millis(800); // Update every 0.8 seconds

    println!("💬 Starting real-time streaming for chat response...");

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                
                for line in text.lines() {
                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str.trim() == "[DONE]" {
                            // Process any remaining content and finalize
                            if !content_buffer.trim().is_empty() {
                                if let Err(e) = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await {
                                    eprintln!("❌ Failed to finalize message: {}", e);
                                }
                            }
                            
                            let stats = StreamingStats {
                                total_characters: raw_response.len(),
                                message_count: message_state.message_index,
                            };
                            
                            println!("💬 Streaming complete - {} total characters", stats.total_characters);
                            return Ok(stats);
                        }
                        
                        if let Ok(response_chunk) = serde_json::from_str::<ChatResponse>(json_str) {    
                            for choice in response_chunk.choices {
                                if let Some(finish_reason) = choice.finish_reason {
                                    if finish_reason == "stop" {
                                        // Process final content
                                        if !content_buffer.trim().is_empty() {
                                            if let Err(e) = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await {
                                                eprintln!("❌ Failed to finalize message: {}", e);
                                            }
                                        }
                                        
                                        let stats = StreamingStats {
                                            total_characters: raw_response.len(),
                                            message_count: message_state.message_index,
                                        };
                                        
                                        return Ok(stats);
                                    }
                                }
                                
                                if let Some(delta) = choice.delta {
                                    if let Some(content) = delta.content {
                                        raw_response.push_str(&content);
                                        content_buffer.push_str(&content);
                                        
                                        // Update Discord message periodically
                                        if last_update.elapsed() >= update_interval && !content_buffer.trim().is_empty() {
                                            if let Err(e) = update_chat_message(&mut message_state, &content_buffer, ctx, config).await {
                                                eprintln!("❌ Failed to update Discord message: {}", e);
                                            } else {
                                                content_buffer.clear(); // Clear buffer after successful update
                                            }
                                            last_update = std::time::Instant::now();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ Stream error: {}", e);
                break;
            }
        }
    }

    // Final cleanup - process any remaining content
    if !content_buffer.trim().is_empty() {
        if let Err(e) = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await {
            eprintln!("❌ Failed to finalize remaining content: {}", e);
        }
    }

    let stats = StreamingStats {
        total_characters: raw_response.len(),
        message_count: message_state.message_index,
    };

    Ok(stats)
}

// Helper function to update Discord message with new content for chat
#[allow(unused_variables)]
async fn update_chat_message(
    state: &mut MessageState,
    new_content: &str,
    ctx: &Context,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let potential_content = if state.current_content.is_empty() {
        format!("🤖 **AI Response (Part {}):**\n```\n{}\n```", 
            state.message_index, new_content)
    } else {
        format!("🤖 **AI Response (Part {}):**\n```\n{}{}\n```", 
            state.message_index, state.current_content, new_content)
    };

    // Check if we need to create a new message
    if potential_content.len() > state.char_limit {
        // Finalize current message
        let final_content = format!("🤖 **AI Response (Part {}):**\n```\n{}\n```", 
            state.message_index, state.current_content);
        
        state.current_message.edit(&ctx.http, |m| {
            m.content(final_content)
        }).await?;

        // Create new message
        state.message_index += 1;
        let new_msg_content = format!("🤖 **AI Response (Part {}):**\n```\n{}\n```", 
            state.message_index, new_content);
        
        let new_message = state.current_message.channel_id.send_message(&ctx.http, |m| {
            m.content(new_msg_content)
        }).await?;

        state.current_message = new_message;
        state.current_content = new_content.to_string();
    } else {
        // Update current message
        state.current_content.push_str(new_content);
        state.current_message.edit(&ctx.http, |m| {
            m.content(&potential_content)
        }).await?;
    }

    Ok(())
}

// Helper function to finalize message content at the end of streaming for chat
#[allow(unused_variables)]
async fn finalize_chat_message(
    state: &mut MessageState,
    remaining_content: &str,
    ctx: &Context,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if remaining_content.trim().is_empty() {
        return Ok(());
    }

    // Add any remaining content and finalize
    update_chat_message(state, remaining_content, ctx, config).await?;
    
    // Mark the final message as complete
    let final_display = if state.message_index == 1 {
        format!("🤖 **AI Response Complete** ✅\n```\n{}\n```", state.current_content)
    } else {
        format!("🤖 **AI Response Complete (Part {}/{})** ✅\n```\n{}\n```", 
            state.message_index, state.message_index, state.current_content)
    };

    state.current_message.edit(&ctx.http, |m| {
        m.content(final_display)
    }).await?;

    Ok(())
} 

// Helper function to load system prompt from file
async fn load_system_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let content = fs::read_to_string("system_prompt.txt")?;
    Ok(content.trim().to_string())
}

// Note: Model loading/unloading functions removed as LM Studio handles model management automatically

