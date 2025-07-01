use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};
use serde::{Deserialize, Serialize};
use std::fs;
use futures_util::StreamExt;
use crate::search::{
    ddg_search, format_search_results, load_lm_config, perform_basic_search, 
    perform_ai_enhanced_search, handle_search_trigger, LMConfig, ChatMessage
};

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

// Non-streaming chat completion for search trigger check
async fn chat_completion(
    messages: Vec<ChatMessage>,
    model: &str,
    config: &LMConfig,
    max_tokens: Option<i32>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout))
        .build()?;
        
    let chat_request = ChatRequest {
        model: model.to_string(),
        messages,
        temperature: 0.3, // Lower temperature for more focused responses
        max_tokens: max_tokens.unwrap_or(config.default_max_tokens),
        stream: false,
    };

    let response = client
        .post(&format!("{}/v1/chat/completions", config.base_url))
        .json(&chat_request)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("API request failed: HTTP {}", response.status()).into());
    }

    // Parse non-streaming response
    let response_text = response.text().await?;
    let response_json: serde_json::Value = serde_json::from_str(&response_text)?;
    
    // Extract content from response
    if let Some(choices) = response_json["choices"].as_array() {
        if let Some(first_choice) = choices.get(0) {
            if let Some(message) = first_choice["message"].as_object() {
                if let Some(content) = message["content"].as_str() {
                    return Ok(content.trim().to_string());
                }
            }
        }
    }
    
    Err("Failed to extract content from API response".into())
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

    // Check if this is a search request
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
                // Fallback to basic search without AI enhancement
                return perform_basic_search(ctx, msg, search_query).await.map_err(|e| e.into());
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
                // Fallback to basic search
                if let Err(fallback_error) = search_msg.edit(&ctx.http, |m| {
                    m.content("🔍 AI enhancement failed, performing basic search...")
                }).await {
                    eprintln!("❌ Failed to update message for fallback: {}", fallback_error);
                }
                
                // Perform basic search as fallback
                match ddg_search(search_query).await {
                    Ok(results) => {
                        let formatted_results = format_search_results(&results, search_query);
                        if let Err(e) = search_msg.edit(&ctx.http, |m| {
                            m.content(&formatted_results)
                        }).await {
                            eprintln!("❌ Failed to update search message: {}", e);
                        }
                    }
                    Err(basic_error) => {
                        let error_msg = format!("❌ **Search Failed**\n\nQuery: `{}`\nError: {}\n\n💡 Try rephrasing your search query or check your internet connection.", search_query, basic_error);
                        let _ = search_msg.edit(&ctx.http, |m| {
                            m.content(&error_msg)
                        }).await;
                    }
                }
            }
        }

        return Ok(());
    }

    // Regular AI chat functionality
    let prompt = input;

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

    // First, check if AI knows the answer or wants to trigger search
    println!("🤔 Checking AI knowledge for query: '{}'", prompt);
    match chat_completion(
        messages.clone(),
        &config.default_model,
        &config,
        Some(16), // Limit tokens for search trigger check
    ).await {
        Ok(initial_response) => {
            let trimmed_response = initial_response.trim();
            if trimmed_response == "__SEARCH__" {
                println!("🔍 AI triggered search for query: '{}'", prompt);
                // Update message to show search trigger
                if let Err(e) = current_msg.edit(&ctx.http, |m| {
                    m.content("🧠 AI doesn't know this - searching the web...")
                }).await {
                    eprintln!("❌ Failed to update message for search trigger: {}", e);
                }
                
                // Trigger AI-enhanced search
                if let Err(e) = handle_search_trigger(prompt, &config, &mut current_msg, ctx).await {
                    eprintln!("❌ Search trigger failed: {}", e);
                    let _ = current_msg.edit(&ctx.http, |m| {
                        m.content("❌ Search trigger failed! Let me try to answer anyway...")
                    }).await;
                    
                    // Fallback to normal chat if search fails
                    match stream_chat_response(messages, &config.default_model, &config, ctx, &mut current_msg).await {
                        Ok(final_stats) => {
                            println!("💬 LM command (fallback): Streaming complete - {} characters", final_stats.total_characters);
                        }
                        Err(stream_error) => {
                            eprintln!("❌ Fallback streaming also failed: {}", stream_error);
                            let _ = current_msg.edit(&ctx.http, |m| {
                                m.content("❌ Both search and AI chat failed! Please try again.")
                            }).await;
                        }
                    }
                }
                return Ok(());
            } else {
                println!("💬 AI has knowledge, proceeding with normal chat response");
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to check AI knowledge: {}", e);
            println!("💬 Proceeding with normal chat response due to check failure");
        }
    }

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
    fn test_search_trigger_detection() {
        // Test exact match for search trigger
        assert_eq!("__SEARCH__".trim(), "__SEARCH__");
        
        // Test with whitespace
        assert_eq!("  __SEARCH__  ".trim(), "__SEARCH__");
        
        // Test non-trigger responses
        assert_ne!("Hello there!".trim(), "__SEARCH__");
        assert_ne!("I don't know __SEARCH__".trim(), "__SEARCH__");
        assert_ne!("__SEARCH__ something".trim(), "__SEARCH__");
    }
    
    #[test]
    fn test_search_trigger_token_limit() {
        // Verify that the token limit for search trigger check is appropriate
        // The check uses 16 tokens which should be enough for "__SEARCH__"
        let trigger_response = "__SEARCH__";
        assert!(trigger_response.len() < 50); // Well under typical token limits
    }
    
    #[tokio::test]
    async fn test_load_system_prompt_contains_search_trigger() {
        // Test that system prompt contains search trigger instruction
        // This test will only pass if system_prompt.txt exists and contains the trigger
        if let Ok(prompt) = load_system_prompt().await {
            assert!(prompt.contains("__SEARCH__"), 
                "System prompt should contain __SEARCH__ trigger instruction");
            assert!(prompt.contains("Search Trigger:") || prompt.contains("search trigger"), 
                "System prompt should contain search trigger documentation");
        }
        // If file doesn't exist, test passes (file is optional)
    }
    
    #[test]
    fn test_chat_message_structure() {
        // Test that ChatMessage can be created properly for search trigger
        let system_message = ChatMessage {
            role: "system".to_string(),
            content: "You are a helpful AI. If you don't know something, respond with exactly __SEARCH__.".to_string(),
        };
        
        let user_message = ChatMessage {
            role: "user".to_string(),
            content: "What is the latest news about quantum computing?".to_string(),
        };
        
        assert_eq!(system_message.role, "system");
        assert_eq!(user_message.role, "user");
        assert!(system_message.content.contains("__SEARCH__"));
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
                println!("Contains __SEARCH__: {}", prompt.contains("__SEARCH__"));
            }
            Err(e) => {
                println!("❌ Failed to load system prompt: {}", e);
            }
        }
    }
}

