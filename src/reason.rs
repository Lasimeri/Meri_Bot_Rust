use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};
use std::fs;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use futures_util::StreamExt;
use crate::search::{LMConfig, ChatMessage};

// Structures for streaming API responses
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

// API Request structure
#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: i32,
    stream: bool,
}

// Structure to track streaming statistics
#[derive(Debug)]
struct StreamingStats {
    total_characters: usize,
    message_count: usize,
    filtered_characters: usize,
}

// Structure to track current message state during streaming
struct MessageState {
    current_content: String,
    current_message: Message,
    message_index: usize,
    char_limit: usize,
}

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
    let config = match load_reasoning_config().await {
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
            println!("🧠 Reasoning command: Using fallback prompt");
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

    // Log which reasoning model is being used
    println!("🧠 Reasoning command: Using model '{}' for reasoning task", config.default_reason_model);

    // Stream the response with real-time filtering and Discord post editing
    match stream_reasoning_response(messages, &config.default_reason_model, &config, ctx, &mut current_msg).await {
        Ok(final_stats) => {
            println!("🧠 Reasoning command: Streaming complete - {} total characters across {} messages", 
                final_stats.total_characters, final_stats.message_count);
        }
        Err(e) => {
            eprintln!("❌ Failed to stream response from reasoning model: {}", e);
            let _ = current_msg.edit(&ctx.http, |m| {
                m.content("❌ Failed to get response from reasoning model!")
            }).await;
        }
    }

    Ok(())
}

// Helper function to load LM Studio configuration specifically for reasoning command
async fn load_reasoning_config() -> Result<LMConfig, Box<dyn std::error::Error + Send + Sync>> {
    let config_paths = [
        "lmapiconf.txt",
        "../lmapiconf.txt", 
        "../../lmapiconf.txt",
        "src/lmapiconf.txt"
    ];
    
    let mut content = String::new();
    let mut found_file = false;
    let mut config_source = "";
    
    // Try to find the config file in multiple locations
    for config_path in &config_paths {
        match fs::read_to_string(config_path) {
            Ok(file_content) => {
                content = file_content;
                found_file = true;
                config_source = config_path;
                println!("🧠 Reasoning command: Found config file at {}", config_path);
                break;
            }
            Err(_) => {
                continue;
            }
        }
    }
    
    if !found_file {
        return Err("lmapiconf.txt file not found in any expected location (., .., ../.., src/) for reasoning command".into());
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
            return Err(format!("❌ Required setting '{}' not found in {} (reasoning command)", key, config_source).into());
        }
    }
    
    // Create config - all values must be present in lmapiconf.txt
    let config = LMConfig {
        base_url: config_map.get("LM_STUDIO_BASE_URL")
            .ok_or("LM_STUDIO_BASE_URL not found")?.clone(),
        timeout: config_map.get("LM_STUDIO_TIMEOUT")
            .ok_or("LM_STUDIO_TIMEOUT not found")?
            .parse()
            .map_err(|_| "Invalid LM_STUDIO_TIMEOUT value")?,
        default_model: config_map.get("DEFAULT_MODEL")
            .ok_or("DEFAULT_MODEL not found")?.clone(),
        default_reason_model: config_map.get("DEFAULT_REASON_MODEL")
            .ok_or("DEFAULT_REASON_MODEL not found")?.clone(),
        default_temperature: config_map.get("DEFAULT_TEMPERATURE")
            .ok_or("DEFAULT_TEMPERATURE not found")?
            .parse()
            .map_err(|_| "Invalid DEFAULT_TEMPERATURE value")?,
        default_max_tokens: config_map.get("DEFAULT_MAX_TOKENS")
            .ok_or("DEFAULT_MAX_TOKENS not found")?
            .parse()
            .map_err(|_| "Invalid DEFAULT_MAX_TOKENS value")?,
        max_discord_message_length: config_map.get("MAX_DISCORD_MESSAGE_LENGTH")
            .ok_or("MAX_DISCORD_MESSAGE_LENGTH not found")?
            .parse()
            .map_err(|_| "Invalid MAX_DISCORD_MESSAGE_LENGTH value")?,
        response_format_padding: config_map.get("RESPONSE_FORMAT_PADDING")
            .ok_or("RESPONSE_FORMAT_PADDING not found")?
            .parse()
            .map_err(|_| "Invalid RESPONSE_FORMAT_PADDING value")?,
    };

    println!("🧠 Reasoning command: Successfully loaded config from {} with reasoning model: '{}'", config_source, config.default_reason_model);
    Ok(config)
}

// Helper function to load reasoning-specific system prompt from file
async fn load_reasoning_system_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Try to load reasoning-specific prompt first, fall back to general system prompt
    let reasoning_prompt_paths = [
        "reasoning_prompt.txt",
        "../reasoning_prompt.txt",
        "../../reasoning_prompt.txt",
        "src/reasoning_prompt.txt",
        "system_prompt.txt",
        "../system_prompt.txt",
        "../../system_prompt.txt",
        "src/system_prompt.txt",
    ];
    
    for path in &reasoning_prompt_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                // Remove BOM if present
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                println!("🧠 Reasoning command: Loaded prompt from {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    Err("No reasoning prompt file found in any expected location".into())
}

// Helper function to filter out <think>...</think> tags and their content
fn filter_thinking_tags(content: &str) -> String {
    let mut result = String::new();
    let mut remaining = content;
    
    loop {
        // Find the start of a thinking block
        if let Some(think_start) = remaining.find("<think>") {
            // Add everything before the thinking block
            result.push_str(&remaining[..think_start]);
            
            // Look for the end of the thinking block
            let after_start_tag = &remaining[think_start + 7..]; // Skip "<think>"
            
            if let Some(think_end) = after_start_tag.find("</think>") {
                // Found matching closing tag, skip everything between tags
                remaining = &after_start_tag[think_end + 8..]; // Skip "</think>"
            } else {
                // No closing tag found, discard everything after the opening tag
                break;
            }
        } else {
            // No more thinking blocks, add the rest of the content
            result.push_str(remaining);
            break;
        }
    }
    
    // Clean up the result - remove extra whitespace and normalize spacing
    let cleaned = result
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join("\n");
    
    cleaned.trim().to_string()
}

// Test function for the thinking tag filter (for debugging)
#[allow(dead_code)]
fn test_filter_thinking_tags() {
    // Test basic filtering
    let test1 = "Before <think>This is thinking</think> After";
    let result1 = filter_thinking_tags(test1);
    println!("Test 1: '{}' -> '{}'", test1, result1);
    assert_eq!(result1, "Before After");
    
    // Test multiple thinking blocks
    let test2 = "Start <think>Think 1</think> Middle <think>Think 2</think> End";
    let result2 = filter_thinking_tags(test2);
    println!("Test 2: '{}' -> '{}'", test2, result2);
    assert_eq!(result2, "Start Middle End");
    
    // Test unclosed thinking tag
    let test3 = "Before <think>Unclosed thinking content...";
    let result3 = filter_thinking_tags(test3);
    println!("Test 3: '{}' -> '{}'", test3, result3);
    assert_eq!(result3, "Before");
    
    // Test no thinking tags
    let test4 = "Just normal content here";
    let result4 = filter_thinking_tags(test4);
    println!("Test 4: '{}' -> '{}'", test4, result4);
    assert_eq!(result4, "Just normal content here");
    
    println!("✅ All thinking tag filter tests passed!");
}

// Main streaming function that handles real-time response with Discord message editing
async fn stream_reasoning_response(
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
    let mut last_update = std::time::Instant::now();
    let update_interval = std::time::Duration::from_millis(800); // Update every 0.8 seconds
    let mut total_filtered_chars = 0; // Track total filtered content length

    println!("🧠 Starting real-time streaming for reasoning response...");

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                
                for line in text.lines() {
                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str.trim() == "[DONE]" {
                            // Apply final thinking tag filtering and finalize
                            let final_filtered_content = filter_thinking_tags(&raw_response);
                            
                            // Check if there's any remaining content to display
                            if final_filtered_content.len() > total_filtered_chars {
                                let remaining_content = &final_filtered_content[total_filtered_chars..];
                                if !remaining_content.trim().is_empty() {
                                    if let Err(e) = finalize_message_content(&mut message_state, remaining_content, ctx, config).await {
                                        eprintln!("❌ Failed to finalize message: {}", e);
                                    }
                                }
                            } else if final_filtered_content.trim().is_empty() {
                                // Handle case where entire response was thinking content
                                let _ = message_state.current_message.edit(&ctx.http, |m| {
                                    m.content("🧠 **Reasoning Complete** ✅\n\nThe AI completed its reasoning process, but the response appears to contain only thinking content. The model may have used `<think>` tags for the entire response.")
                                }).await;
                            }
                            
                            let stats = StreamingStats {
                                total_characters: raw_response.len(),
                                message_count: message_state.message_index,
                                filtered_characters: raw_response.len() - final_filtered_content.len(),
                            };
                            
                            println!("🧠 Streaming complete - filtered {} characters of thinking content", 
                                stats.filtered_characters);
                            return Ok(stats);
                        }
                        
                        if let Ok(response_chunk) = serde_json::from_str::<ChatResponse>(json_str) {    
                            for choice in response_chunk.choices {
                                if let Some(finish_reason) = choice.finish_reason {
                                    if finish_reason == "stop" {
                                        // Apply final thinking tag filtering and finalize
                                        let final_filtered_content = filter_thinking_tags(&raw_response);
                                        
                                        // Check if there's any remaining content to display
                                        if final_filtered_content.len() > total_filtered_chars {
                                            let remaining_content = &final_filtered_content[total_filtered_chars..];
                                            if !remaining_content.trim().is_empty() {
                                                if let Err(e) = finalize_message_content(&mut message_state, remaining_content, ctx, config).await {
                                                    eprintln!("❌ Failed to finalize message: {}", e);
                                                }
                                            }
                                        } else if final_filtered_content.trim().is_empty() {
                                            // Handle case where entire response was thinking content
                                            let _ = message_state.current_message.edit(&ctx.http, |m| {
                                                m.content("🧠 **Reasoning Complete** ✅\n\nThe AI completed its reasoning process, but the response appears to contain only thinking content. The model may have used `<think>` tags for the entire response.")
                                            }).await;
                                        }
                                        
                                        let stats = StreamingStats {
                                            total_characters: raw_response.len(),
                                            message_count: message_state.message_index,
                                            filtered_characters: raw_response.len() - final_filtered_content.len(),
                                        };
                                        
                                        return Ok(stats);
                                    }
                                }
                                
                                if let Some(delta) = choice.delta {
                                    if let Some(content) = delta.content {
                                        raw_response.push_str(&content);
                                        
                                        // Update Discord message periodically with filtered content
                                        if last_update.elapsed() >= update_interval && raw_response.len() > 50 {
                                            // Apply thinking tag filtering to the accumulated response
                                            let filtered_content = filter_thinking_tags(&raw_response);
                                            
                                            // Only update if we have meaningful filtered content
                                            if !filtered_content.trim().is_empty() {
                                                // Calculate what new content to show (difference from what's already displayed)
                                                let new_content = if filtered_content.len() > total_filtered_chars {
                                                    &filtered_content[total_filtered_chars..]
                                                } else {
                                                    ""
                                                };
                                                
                                                if !new_content.trim().is_empty() {
                                                    if let Err(e) = update_discord_message(&mut message_state, new_content, ctx, config).await {
                                                        eprintln!("❌ Failed to update Discord message: {}", e);
                                                    } else {
                                                        total_filtered_chars = filtered_content.len();
                                                    }
                                                }
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

    // Final cleanup - apply thinking tag filtering and process any remaining content
    let final_filtered_content = filter_thinking_tags(&raw_response);
    
    if final_filtered_content.len() > total_filtered_chars {
        let remaining_content = &final_filtered_content[total_filtered_chars..];
        if !remaining_content.trim().is_empty() {
            if let Err(e) = finalize_message_content(&mut message_state, remaining_content, ctx, config).await {
                eprintln!("❌ Failed to finalize remaining content: {}", e);
            }
        }
    } else if final_filtered_content.trim().is_empty() {
        // Handle case where entire response was thinking content
        let _ = message_state.current_message.edit(&ctx.http, |m| {
            m.content("🧠 **Reasoning Complete** ✅\n\nThe AI completed its reasoning process, but the response appears to contain only thinking content. The model may have used `<think>` tags for the entire response.")
        }).await;
    }

    let stats = StreamingStats {
        total_characters: raw_response.len(),
        message_count: message_state.message_index,
        filtered_characters: raw_response.len() - final_filtered_content.len(),
    };

    Ok(stats)
}

// Helper function to update Discord message with new content
#[allow(unused_variables)]
async fn update_discord_message(
    state: &mut MessageState,
    new_content: &str,
    ctx: &Context,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let potential_content = if state.current_content.is_empty() {
        format!("🧠 **Reasoning Response (Part {}):**\n```\n{}\n```", 
            state.message_index, new_content)
    } else {
        format!("🧠 **Reasoning Response (Part {}):**\n```\n{}{}\n```", 
            state.message_index, state.current_content, new_content)
    };

    // Check if we need to create a new message
    if potential_content.len() > state.char_limit {
        // Finalize current message
        let final_content = format!("🧠 **Reasoning Response (Part {}):**\n```\n{}\n```", 
            state.message_index, state.current_content);
        
        state.current_message.edit(&ctx.http, |m| {
            m.content(final_content)
        }).await?;

        // Create new message
        state.message_index += 1;
        let new_msg_content = format!("🧠 **Reasoning Response (Part {}):**\n```\n{}\n```", 
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

// Helper function to finalize message content at the end of streaming
#[allow(unused_variables)]
async fn finalize_message_content(
    state: &mut MessageState,
    remaining_content: &str,
    ctx: &Context,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if remaining_content.trim().is_empty() {
        return Ok(());
    }

    // Add any remaining content and finalize
    update_discord_message(state, remaining_content, ctx, config).await?;
    
    // Mark the final message as complete
    let final_display = if state.message_index == 1 {
        format!("🧠 **Reasoning Complete** ✅\n```\n{}\n```", state.current_content)
    } else {
        format!("🧠 **Reasoning Complete (Part {}/{})** ✅\n```\n{}\n```", 
            state.message_index, state.message_index, state.current_content)
    };

    state.current_message.edit(&ctx.http, |m| {
        m.content(final_display)
    }).await?;

    Ok(())
} 