use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::collections::HashMap;
use futures_util::StreamExt;

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

    // Buffer the complete response from streaming
    match buffer_chat_response(messages, &config.default_model, &config, ctx, &mut current_msg).await {
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
            eprintln!("❌ Failed to get response from AI model: {}", e);
            let _ = current_msg.edit(&ctx.http, |m| {
                m.content("❌ Failed to get response from AI model!")
            }).await;
        }
    }

    Ok(())
}

// Helper function to load system prompt from file
async fn load_system_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let content = fs::read_to_string("system_prompt.txt")?;
    Ok(content.trim().to_string())
}

// Note: Model loading/unloading functions removed as LM Studio handles model management automatically

// Helper function to buffer chat response from LM Studio with live updates
pub async fn buffer_chat_response(
    messages: Vec<ChatMessage>, 
    model: &str,
    config: &LMConfig, 
    ctx: &Context, 
    current_msg: &mut Message
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout * 3)) // Use longer timeout for streaming
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

    let mut response_buffer = String::new();
    let mut stream = response.bytes_stream();
    let mut last_update = std::time::Instant::now();
    let update_interval = std::time::Duration::from_millis(1500); // Update every 1.5 seconds

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                
                // Process each line in the chunk
                for line in text.lines() {
                    // Handle SSE format: "data: {json}"
                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str.trim() == "[DONE]" {
                            return Ok(response_buffer);
                        }
                        
                        if let Ok(response_chunk) = serde_json::from_str::<ChatResponse>(json_str) {
                            for choice in response_chunk.choices {
                                if let Some(finish_reason) = choice.finish_reason {
                                    if finish_reason == "stop" {
                                        return Ok(response_buffer);
                                    }
                                }
                                
                                if let Some(delta) = choice.delta {
                                    if let Some(content) = delta.content {
                                        response_buffer.push_str(&content);
                                        
                                        // Update Discord message periodically to show progress
                                        if last_update.elapsed() >= update_interval {
                                            let preview = if response_buffer.len() > 100 {
                                                format!("🤖 Generating response... ({} characters so far)\n\n📝 Preview:\n```\n{}...\n```", 
                                                    response_buffer.len(), 
                                                    &response_buffer[..97])
                                            } else {
                                                format!("🤖 Generating response... ({} characters so far)", response_buffer.len())
                                            };
                                            
                                            let _ = current_msg.edit(&ctx.http, |m| {
                                                m.content(preview)
                                            }).await;
                                            
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
                eprintln!("Stream error: {}", e);
                break;
            }
        }
    }

    Ok(response_buffer)
}

// Helper function to send buffered response in Discord-sized chunks
pub async fn send_buffered_response(
    ctx: &Context,
    current_msg: &mut Message,
    content: &str,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if content.is_empty() {
        current_msg.edit(&ctx.http, |m| {
            m.content("🤖 *No response generated*")
        }).await?;
        return Ok(());
    }

    // Calculate chunk size accounting for formatting overhead
    let chunk_size = config.max_discord_message_length - config.response_format_padding;
    
    // Split content into word-boundary chunks to avoid breaking words
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    
    for word in content.split_whitespace() {
        // Check if adding this word would exceed the chunk size
        if current_chunk.len() + word.len() + 1 > chunk_size && !current_chunk.is_empty() {
            chunks.push(current_chunk.clone());
            current_chunk.clear();
        }
        
        if !current_chunk.is_empty() {
            current_chunk.push(' ');
        }
        current_chunk.push_str(word);
    }
    
    // Add the last chunk if it's not empty
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    println!("📊 Buffered response: {} characters split into {} chunks", content.len(), chunks.len());

    // Edit the first message with the first chunk
    if let Some(first_chunk) = chunks.first() {
        let formatted_content = if chunks.len() > 1 {
            format!("🤖 **AI Response (Part 1/{}) - {} total characters:**\n```\n{}\n```", 
                chunks.len(), content.len(), first_chunk)
        } else {
            format!("🤖 **AI Response ({} characters):**\n```\n{}\n```", 
                content.len(), first_chunk)
        };

        current_msg.edit(&ctx.http, |m| {
            m.content(formatted_content)
        }).await?;
    }

    // Send additional messages for remaining chunks
    for (i, chunk) in chunks.iter().skip(1).enumerate() {
        let formatted_content = format!("🤖 **AI Response (Part {}/{}):**\n```\n{}\n```", 
            i + 2, chunks.len(), chunk);
        
        current_msg.channel_id.send_message(&ctx.http, |m| {
            m.content(formatted_content)
        }).await?;
        
        // Small delay between messages to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    Ok(())
} 