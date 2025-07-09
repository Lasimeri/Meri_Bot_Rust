use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};
use serde::{Deserialize, Serialize};
use std::fs;
use futures_util::StreamExt;
use crate::search::{
    load_lm_config, perform_ai_enhanced_search, LMConfig, ChatMessage
};
use crate::LmContextMap; // TypeMap key defined in main.rs

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

// API Request/Response structures for streaming
#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: i32,
    stream: bool,
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

#[command]
#[aliases("llm", "ai", "chat")]
pub async fn lm(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let input = args.message().trim();
    
    // Start typing indicator
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    
    if input.is_empty() {
        msg.reply(ctx, "❌ Please provide a prompt! Usage: `^lm <your prompt>` or `^lm -s <search query>`").await?;
        return Ok(());
    }

    // Skip context logic for search and test flags
    if input.starts_with("-s ") || input.starts_with("--search ") {
        // Extract search query
        let search_query = if input.starts_with("-s ") {
            &input[3..]
        } else {
            &input[9..] // "--search "
        };

        if search_query.trim().is_empty() {
            msg.reply(ctx, "❌ Please provide a search query! Usage: `^lm -s <search query>`").await?;
            return Ok(());
        }

        // Load LM Studio configuration for AI-enhanced search
        let config = match load_lm_config().await {
            Ok(config) => config,
            Err(e) => {
                eprintln!("❌ Failed to load LM Studio configuration for search: {}", e);
                msg.reply(ctx, &format!("❌ LM Studio configuration error: {}\n\nMake sure `lmapiconf.txt` exists and contains all required settings. Check `example_lmapiconf.txt` for reference.", e)).await?;
                return Ok(());
            }
        };

        // Send initial search message
        let mut search_msg = match msg.channel_id.send_message(&ctx.http, |m| {
            m.content("🧠 Refining search query...")
        }).await {
            Ok(message) => message,
            Err(e) => {
                eprintln!("❌ Failed to send initial search message: {}", e);
                msg.reply(ctx, "❌ Failed to send message!").await?;
                return Ok(());
            }
        };

        // AI-Enhanced Search Flow
        match perform_ai_enhanced_search(search_query, &config, &mut search_msg, ctx).await {
            Ok(()) => {
                println!("🔍 AI-enhanced search completed successfully for query: '{}'", search_query);
            }
            Err(e) => {
                eprintln!("❌ AI-enhanced search failed: {}", e);
                let error_msg = format!("❌ **Search Failed**\n\nQuery: `{}`\nError: {}\n\n💡 Check your SerpAPI configuration in lmapiconf.txt", search_query, e);
                let _ = search_msg.edit(&ctx.http, |m| {
                    m.content(&error_msg)
                }).await;
            }
        }

        return Ok(());
    }

    if input.starts_with("--test") || input == "-t" {
        // Load LM Studio configuration for connectivity test
        let config = match load_lm_config().await {
            Ok(config) => config,
            Err(e) => {
                eprintln!("❌ Failed to load LM Studio configuration for test: {}", e);
                msg.reply(ctx, &format!("❌ LM Studio configuration error: {}\n\nMake sure `lmapiconf.txt` exists and contains all required settings. Check `example_lmapiconf.txt` for reference.", e)).await?;
                return Ok(());
            }
        };

        // Send initial test message
        let mut test_msg = match msg.channel_id.send_message(&ctx.http, |m| {
            m.content("🔗 Testing API connectivity to remote server...")
        }).await {
            Ok(message) => message,
            Err(e) => {
                eprintln!("❌ Failed to send initial test message: {}", e);
                msg.reply(ctx, "❌ Failed to send message!").await?;
                return Ok(());
            }
        };

        // Perform connectivity test
        match crate::search::test_api_connectivity(&config).await {
            Ok(success_message) => {
                let final_message = format!("✅ **Connectivity Test Results**\n\n{}\n\n**Configuration:**\n• Server: `{}`\n• Default Model: `{}`\n• Reasoning Model: `{}`\n• Timeout: `{}s`", 
                    success_message, config.base_url, config.default_model, config.default_reason_model, config.timeout);
                
                if let Err(e) = test_msg.edit(&ctx.http, |m| {
                    m.content(&final_message)
                }).await {
                    eprintln!("❌ Failed to update test message: {}", e);
                }
            }
            Err(e) => {
                let error_message = format!("❌ **Connectivity Test Failed**\n\n**Error:** {}\n\n**Troubleshooting:**\n• Check if LM Studio/Ollama is running on `{}`\n• Verify the model `{}` is loaded\n• Check firewall settings\n• Ensure the server is accessible from this network\n\n**Configuration:**\n• Server: `{}`\n• Default Model: `{}`\n• Timeout: `{}s`", 
                    e, config.base_url, config.default_model, config.base_url, config.default_model, config.timeout);
                
                if let Err(edit_error) = test_msg.edit(&ctx.http, |m| {
                    m.content(&error_message)
                }).await {
                    eprintln!("❌ Failed to update test message with error: {}", edit_error);
                }
            }
        }

        return Ok(());
    }

    // Check if this is a clear context request
    if input.starts_with("--clear") || input == "-c" {
        let mut data_map = ctx.data.write().await;
        let lm_map = data_map.get_mut::<LmContextMap>().expect("LM context map not initialized");
        lm_map.remove(&msg.author.id);
        msg.reply(ctx, "🧹 **LM Chat Context Cleared**\nYour conversation history has been reset.").await?;
        return Ok(());
    }

    // Regular AI chat functionality
    let prompt = input;

    // Record user prompt in per-user context history
    {
        let mut data_map = ctx.data.write().await;
        let lm_map = data_map.get_mut::<LmContextMap>().expect("LM context map not initialized");
        let history = lm_map.entry(msg.author.id).or_insert_with(Vec::new);
        history.push(ChatMessage { role: "user".to_string(), content: prompt.to_string() });
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

    // Build message list including system prompt and per-user history
    let mut messages = Vec::new();
    messages.push(ChatMessage { role: "system".to_string(), content: system_prompt });
    {
        let data_map = ctx.data.read().await;
        let lm_map = data_map.get::<LmContextMap>().expect("LM context map not initialized");
        if let Some(history) = lm_map.get(&msg.author.id) {
            println!("💬 LM command: Including {} context messages for user {}", history.len(), msg.author.name);
            for entry in history.iter() {
                messages.push(entry.clone());
            }
        } else {
            println!("💬 LM command: No context history found for user {}", msg.author.name);
        }
    }

    // Log which model is being used for LM command
    println!("💬 LM command: Using model '{}' for chat", config.default_model);

    // Stream the response with real-time Discord post editing
    let mut current_msg = msg.channel_id.send_message(&ctx.http, |m| {
        m.content("🤖 **AI Response (Part 1):**\n```\n\n```")
    }).await?;

    // Ensure current_msg is in scope for this match
    match stream_chat_response(messages, &config.default_model, &config, ctx, &mut current_msg).await {
        Ok(final_stats) => {
            println!("💬 LM command: Streaming complete - {} total characters across {} messages", 
                final_stats.total_characters, final_stats.message_count);
            
            // Record AI response in per-user context history
            let response_content = current_msg.content.clone();
            let mut data_map = ctx.data.write().await;
            let lm_map = data_map.get_mut::<LmContextMap>().expect("LM context map not initialized");
            if let Some(history) = lm_map.get_mut(&msg.author.id) {
                history.push(ChatMessage { 
                    role: "assistant".to_string(), 
                    content: response_content 
                });
                
                // Limit history to last 10 messages to prevent token overflow
                if history.len() > 10 {
                    history.drain(0..history.len()-10);
                }
            }
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
    println!("🔗 [STREAMING] Attempting connection to API server: {}", config.base_url);
    println!("🔗 [STREAMING] Using model: {}", model);
    println!("🔗 [STREAMING] Connect timeout: {} seconds", config.timeout);
    
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(config.timeout))
        .build()?;
        
    let chat_request = ChatRequest {
        model: model.to_string(),
        messages,
        temperature: config.default_temperature,
        max_tokens: config.default_max_tokens,
        stream: true,
    };

    let api_url = format!("{}/v1/chat/completions", config.base_url);
    println!("🔗 [STREAMING] Full API URL: {}", api_url);
    println!("🔗 [STREAMING] Request payload: model={}, max_tokens={}, temperature={}, stream=true", 
        chat_request.model, chat_request.max_tokens, chat_request.temperature);

    // First, test basic connectivity to the server
    println!("🔗 [STREAMING] Testing basic connectivity to {}...", config.base_url);
    match client.get(&config.base_url).send().await {
        Ok(response) => {
            println!("✅ [STREAMING] Basic connectivity test successful - Status: {}", response.status());
        }
        Err(e) => {
            println!("❌ [STREAMING] Basic connectivity test failed: {}", e);
            return Err(format!("Cannot reach remote server {}: {}", config.base_url, e).into());
        }
    }

    // Now attempt the actual streaming API call
    println!("🔗 [STREAMING] Making streaming API request to chat completions endpoint...");
    let response = match client
        .post(&api_url)
        .json(&chat_request)
        .send()
        .await
    {
        Ok(resp) => {
            println!("✅ [STREAMING] API request sent successfully - Status: {}", resp.status());
            resp
        }
        Err(e) => {
            println!("❌ [STREAMING] API request failed: {}", e);
            return Err(format!("Streaming API request to {} failed: {}", api_url, e).into());
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
        println!("❌ [STREAMING] API returned error status {}: {}", status, error_text);
        return Err(format!("Streaming API request failed: HTTP {} - {}", status, error_text).into());
    }

    println!("✅ [STREAMING] Starting to process response stream...");
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
    let mut chunk_count = 0;
    let mut line_buffer = String::new();

    println!("💬 Starting real-time streaming for chat response...");

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                chunk_count += 1;
                if chunk_count == 1 {
                    println!("✅ [STREAMING] Received first chunk ({} bytes)", bytes.len());
                } else if chunk_count % 10 == 0 {
                    println!("📊 [STREAMING] Processed {} chunks, total response: {} chars", chunk_count, raw_response.len());
                }
                
                line_buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(i) = line_buffer.find('\n') {
                    let line = line_buffer.drain(..=i).collect::<String>();
                    let line = line.trim();

                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str.trim() == "[DONE]" {
                            println!("✅ [STREAMING] Received [DONE] signal, finalizing response");
                            if !content_buffer.is_empty() {
                                if let Err(e) = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await {
                                    eprintln!("❌ Failed to finalize message: {}", e);
                                }
                            }
                            return Ok(StreamingStats { total_characters: raw_response.len(), message_count: message_state.message_index });
                        }

                        if let Ok(response_chunk) = serde_json::from_str::<ChatResponse>(json_str) {
                            for choice in response_chunk.choices {
                                if let Some(finish_reason) = choice.finish_reason {
                                    if finish_reason == "stop" {
                                        println!("✅ [STREAMING] Received finish_reason=stop, finalizing response");
                                        if !content_buffer.is_empty() {
                                            if let Err(e) = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await {
                                                eprintln!("❌ Failed to finalize message: {}", e);
                                            }
                                        }
                                        return Ok(StreamingStats { total_characters: raw_response.len(), message_count: message_state.message_index });
                                    }
                                }

                                if let Some(delta) = choice.delta {
                                    if let Some(content) = delta.content {
                                        raw_response.push_str(&content);
                                        content_buffer.push_str(&content);

                                        if last_update.elapsed() >= update_interval && !content_buffer.is_empty() {
                                            if let Err(e) = update_chat_message(&mut message_state, &content_buffer, ctx, config).await {
                                                eprintln!("❌ Failed to update Discord message: {}", e);
                                                return Err(e);
                                            } else {
                                                content_buffer.clear();
                                            }
                                            last_update = std::time::Instant::now();
                                        }
                                    }
                                }
                            }
                        } else {
                            if !json_str.trim().is_empty() {
                                println!("⚠️ [STREAMING] Failed to parse JSON chunk: {}", json_str);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ [STREAMING] Stream error: {}", e);
                if !content_buffer.is_empty() {
                    let _ = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await;
                }
                return Err(e.into());
            }
        }
    }

    println!("📊 [STREAMING] Stream ended, processed {} chunks total", chunk_count);
    
    // Final cleanup - process any remaining content
    if !content_buffer.is_empty() {
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

// Helper function to load system prompt from file using multi-path fallback
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
                // Remove BOM if present
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                println!("💬 LM command: Loaded system prompt from {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    Err("system_prompt.txt file not found in any expected location (., .., ../.., src/)".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_chat_message_structure() {
        // Test that ChatMessage can be created properly
        let system_message = ChatMessage {
            role: "system".to_string(),
            content: "You are a helpful AI assistant.".to_string(),
        };
        
        let user_message = ChatMessage {
            role: "user".to_string(),
            content: "What is machine learning?".to_string(),
        };
        
        assert_eq!(system_message.role, "system");
        assert_eq!(user_message.role, "user");
    }
    
    #[tokio::test]
    async fn debug_prompt_loading() {
        println!("=== DEBUG: Testing prompt loading ===");
        
        // Test system prompt loading
        match load_system_prompt().await {
            Ok(prompt) => {
                println!("✅ System prompt loaded successfully:");
                println!("Length: {} characters", prompt.len());
                println!("Content preview: {}", &prompt[..prompt.len().min(200)]);
            }
            Err(e) => {
                println!("❌ Failed to load system prompt: {}", e);
            }
        }
    }
}

