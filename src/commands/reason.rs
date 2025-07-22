// reason.rs - Advanced Reasoning and Analytical AI Command Module
// This module implements the ^reason command, providing deep reasoning, step-by-step analysis, and analytical web search.
// It supports real-time streaming, thinking tag filtering, context persistence, and reasoning-enhanced search.
//
// Key Features:
// - Dedicated reasoning model
// - Real-time streaming with <think> tag filtering (removes internal thoughts)
// - Buffered chunking for long responses (reason -s)
// - Analytical web search with embedded source links
// - Multi-path config and prompt loading
// - Robust error handling and context management
//
// Used by: main.rs (command registration), search.rs (for web search)

use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};
use std::fs;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use futures_util::StreamExt;
use crate::commands::search::{LMConfig, ChatMessage};
use crate::ReasonContextMap; // TypeMap key defined in main.rs
use regex::Regex;
use once_cell::sync::Lazy;

// Compile regex once for better performance - matches <think> tags
// Used to filter out internal AI thoughts from streaming output
static THINKING_TAG_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<think>.*?</think>").unwrap()
});

// Structures for streaming API responses
// Used to parse streaming JSON chunks from the AI API
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

// API Request structure for reasoning model
#[derive(Serialize)]
struct ChatRequest {
    model: String,              // Model name
    messages: Vec<ChatMessage>, // Conversation history
    temperature: f32,           // Sampling temperature
    max_tokens: i32,            // Max tokens to generate
    stream: bool,               // Whether to stream output
}

// Structure to track streaming statistics for reasoning
#[derive(Debug)]
struct StreamingStats {
    total_characters: usize,    // Total characters streamed
    message_count: usize,       // Number of Discord messages sent
    filtered_characters: usize, // Characters filtered by <think> tag removal
}

// Structure to track current message state during streaming
struct MessageState {
    current_content: String,    // Accumulated content for current Discord message
    current_message: Message,   // Current Discord message object
    message_index: usize,       // Part number (for chunked output)
    char_limit: usize,          // Discord message length limit
}

#[command]
#[aliases("reasoning")]
/// Main ^reason command handler
/// Handles user questions, reasoning-enhanced search, and context management
/// Supports:
///   - ^reason <question> (step-by-step reasoning)
///   - ^reason -s <query> (analytical web search)
///   - ^reason --clear (clear context)
pub async fn reason(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let input = args.message().trim();
    
    // Start typing indicator
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    
    if input.is_empty() {
        msg.reply(ctx, "Please provide a question! Usage: `^reason <your reasoning question>`").await?;
        return Ok(());
    }

    // Check if this is a search request
    if input.starts_with("-s ") || input.starts_with("--search ") {
        // Extract search query
        let search_query = if input.starts_with("-s ") {
            input.strip_prefix("-s ").unwrap()
        } else {
            input.strip_prefix("--search ").unwrap()
        };

        if search_query.trim().is_empty() {
            msg.reply(ctx, "Please provide a search query! Usage: `^reason -s <search query>`").await?;
            return Ok(());
        }

        // Load LM Studio configuration for AI-enhanced reasoning search
        let config = match load_reasoning_config().await {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Failed to load LM Studio configuration for reasoning search: {}", e);
                msg.reply(ctx, &format!("LM Studio configuration error: {}\n\nMake sure `lmapiconf.txt` exists and contains all required settings. Check `example_lmapiconf.txt` for reference.", e)).await?;
                return Ok(());
            }
        };

        // Send initial search message
        let mut search_msg = match msg.channel_id.send_message(&ctx.http, |m| {
            m.content("Refining search query for reasoning analysis...")
        }).await {
            Ok(message) => message,
            Err(e) => {
                eprintln!("Failed to send initial search message: {}", e);
                msg.reply(ctx, "Failed to send message!").await?;
                return Ok(());
            }
        };

        // Reasoning-Enhanced Search Flow
        match perform_reasoning_enhanced_search(search_query, &config, &mut search_msg, ctx).await {
            Ok(()) => {
                println!("Reasoning-enhanced search completed successfully for query: '{}'", search_query);
            }
            Err(e) => {
                eprintln!("Reasoning-enhanced search failed: {}", e);
                let error_msg = format!("**Reasoning Search Failed**\n\nQuery: `{}`\nError: {}\n\nCheck your SerpAPI configuration in lmapiconf.txt", search_query, e);
                let _ = search_msg.edit(&ctx.http, |m| {
                    m.content(&error_msg)
                }).await;
            }
        }

        return Ok(());
    }

    // Check if this is a clear context request
    if input.starts_with("--clear") || input == "-c" {
        let mut data_map = ctx.data.write().await;
        let reason_map = data_map.get_mut::<ReasonContextMap>().expect("Reason context map not initialized");
        
        let had_context = if let Some(context) = reason_map.get_mut(&msg.author.id) {
            let message_count = context.total_messages();
            let context_info = context.get_context_info();
            println!("[reason] Clearing context via --clear flag for user {}: {}", msg.author.id, context_info);
            context.clear();
            message_count > 0
        } else {
            false
        };
        
        if had_context {
            msg.reply(ctx, "**Reasoning Context Cleared** ✅\nYour reasoning conversation history has been reset. The next reasoning question you ask will start a brand new context.").await?;
        } else {
            msg.reply(ctx, "**No Reasoning Context Found** ℹ️\nYou don't have any active reasoning conversation history to clear.").await?;
        }
        return Ok(());
    }

    // Regular reasoning functionality
    let question = input;

    // Record user question in per-user context history
    {
        let mut data_map = ctx.data.write().await;
        let reason_map = data_map.get_mut::<ReasonContextMap>().expect("Reason context map not initialized");
        let context = reason_map.entry(msg.author.id).or_insert_with(crate::UserContext::new);
        context.add_user_message(ChatMessage { role: "user".to_string(), content: question.to_string() });
        
        println!("[REASON] User context updated: {} user messages, {} assistant messages", 
            context.user_messages.len(), context.assistant_messages.len());
    }

    // Load LM Studio configuration
    let config = match load_reasoning_config().await {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load LM Studio configuration: {}", e);
            msg.reply(ctx, &format!("LM Studio configuration error: {}\n\nMake sure `lmapiconf.txt` exists and contains all required settings. Check `example_lmapiconf.txt` for reference.", e)).await?;
            return Ok(());
        }
    };

    // Load reasoning system prompt
    let system_prompt = match load_reasoning_system_prompt().await {
        Ok(prompt) => prompt,
        Err(e) => {
            eprintln!("Failed to load reasoning system prompt: {}", e);
            println!("Reasoning command: Using fallback prompt");
            // Fallback to a default reasoning prompt if file doesn't exist
            "You are an advanced AI reasoning assistant. Think step-by-step through problems and provide detailed, logical explanations. Break down complex questions into smaller parts and explain your reasoning process clearly.".to_string()
        }
    };

    // Build message list including system prompt and per-user history
    let mut messages = Vec::new();
    messages.push(ChatMessage { role: "system".to_string(), content: system_prompt });
    {
        let data_map = ctx.data.read().await;
        let reason_map = data_map.get::<ReasonContextMap>().expect("Reason context map not initialized");
        if let Some(context) = reason_map.get(&msg.author.id) {
            let conversation_messages = context.get_conversation_messages();
            println!("Reasoning command: Including {} context messages for user {}", conversation_messages.len(), msg.author.name);
            for entry in conversation_messages.iter() {
                messages.push(entry.clone());
            }
        } else {
            println!("Reasoning command: No context history found for user {}", msg.author.name);
        }
    }

    // Send initial "thinking" message
    let mut current_msg = match msg.channel_id.send_message(&ctx.http, |m| {
        m.content("**Reasoning Response (Part 1):**\n```\n\n```")
    }).await {
        Ok(message) => message,
        Err(e) => {
            eprintln!("Failed to send initial message: {}", e);
            msg.reply(ctx, "Failed to send message!").await?;
            return Ok(());
        }
    };

    // Log which reasoning model is being used
    println!("Reasoning command: Using model '{}' for reasoning task", config.default_reason_model);

    // Stream the reasoning response
    match stream_reasoning_response(messages, &config.default_reason_model, &config, ctx, &mut current_msg).await {
        Ok(final_stats) => {
            println!("Reasoning command: Streaming complete - {} total characters across {} messages", 
                final_stats.total_characters, final_stats.message_count);
            
            // Record AI response in per-user context history
            let response_content = current_msg.content.clone();
            let mut data_map = ctx.data.write().await;
            let reason_map = data_map.get_mut::<ReasonContextMap>().expect("Reason context map not initialized");
            if let Some(context) = reason_map.get_mut(&msg.author.id) {
                context.add_assistant_message(ChatMessage { 
                    role: "assistant".to_string(), 
                    content: response_content 
                });
                
                println!("[REASON] AI response recorded: {} total messages in context", 
                    context.total_messages());
            }
        }
        Err(e) => {
            eprintln!("Failed to stream reasoning response: {}", e);
            let _ = current_msg.edit(&ctx.http, |m| {
                m.content("Failed to get reasoning response!")
            }).await;
        }
    }

    Ok(())
}

#[command]
#[aliases("clearreason", "resetreason", "clearcontext")]
/// Command to clear the user's reason chat context
/// Removes all reasoning conversation history for the user
pub async fn clearreasoncontext(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    // Clear the user's reason chat context robustly
    let mut data_map = ctx.data.write().await;
    let reason_map = data_map.get_mut::<ReasonContextMap>().expect("Reason context map not initialized");
    
    let user_id = msg.author.id;
    let had_context = if let Some(context) = reason_map.get_mut(&user_id) {
        let message_count = context.total_messages();
        let context_info = context.get_context_info();
        println!("[clearcontext] Clearing reason context for user {}: {}", user_id, context_info);
        
        // Clear the context completely
        context.clear();
        
        // Verify the context was cleared
        let after_clear_info = context.get_context_info();
        println!("[clearcontext] Context after clearing: {}", after_clear_info);
        
        message_count > 0
    } else {
        false
    };
    
    println!("[clearcontext] Cleared reason context for user {} (had_context={})", user_id, had_context);
    
    if had_context {
        // Save the cleared context state to disk immediately
        {
            let data = ctx.data.read().await;
            let lm_contexts = data.get::<crate::LmContextMap>().cloned().unwrap_or_default();
            let reason_contexts = data.get::<crate::ReasonContextMap>().cloned().unwrap_or_default();
            let global_lm_context = data.get::<crate::GlobalLmContextMap>().cloned().unwrap_or_else(|| crate::UserContext::new());
            
            if let Err(e) = crate::save_contexts_to_disk(&lm_contexts, &reason_contexts, &global_lm_context).await {
                eprintln!("Failed to save cleared context to disk: {}", e);
            } else {
                println!("[clearcontext] Cleared context state saved to disk");
            }
        }
        
        msg.reply(ctx, "**Reasoning Context Cleared** ✅\nYour reasoning conversation history has been fully reset and saved. The next reasoning question you ask will start a brand new context.").await?;
    } else {
        msg.reply(ctx, "**No Reasoning Context Found** ℹ️\nYou don't have any active reasoning conversation history to clear. Start a reasoning conversation with `^reason <your question>`.\n\nIf you believe reasoning context is still being used, please report this as a bug.").await?;
    }
    
    Ok(())
}

// Helper function to load LM Studio configuration specifically for reasoning command
// Loads all required settings from lmapiconf.txt using multi-path fallback
// Returns LMConfig or error
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
                println!("Reasoning command: Found config file at {}", config_path);
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
        "DEFAULT_RANKING_MODEL",
        "DEFAULT_TEMPERATURE",
        "DEFAULT_MAX_TOKENS",
        "MAX_DISCORD_MESSAGE_LENGTH",
        "RESPONSE_FORMAT_PADDING",
        "DEFAULT_VISION_MODEL",
    ];
    
    for key in &required_keys {
        if !config_map.contains_key(*key) {
            return Err(format!("Required setting '{}' not found in {} (reasoning command)", key, config_source).into());
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
        default_ranking_model: config_map.get("DEFAULT_RANKING_MODEL")
            .ok_or("DEFAULT_RANKING_MODEL not found")?.clone(),
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
        default_vision_model: config_map.get("DEFAULT_VISION_MODEL")
            .ok_or("DEFAULT_VISION_MODEL not found")?.clone(),
    };

    println!("Reasoning command: Successfully loaded config from {} with reasoning model: '{}'", config_source, config.default_reason_model);
    Ok(config)
}

// Helper function to load reasoning-specific system prompt from file
// Tries reasoning_prompt.txt, falls back to system_prompt.txt
// Returns prompt string or error
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
                println!("Reasoning command: Loaded prompt from {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    Err("No reasoning prompt file found in any expected location".into())
}

// Simple and reliable thinking tag filter
// Removes all <think>...</think> blocks from the content
fn filter_thinking_tags(content: &str) -> String {
    // Use pre-compiled regex to remove thinking tags and their content
    let filtered = THINKING_TAG_REGEX.replace_all(content, "");
    
    // Clean up whitespace and empty lines
    let lines: Vec<&str> = filtered
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    
    lines.join("\n").trim().to_string()
}

// Simple processing function that just filters thinking tags
// Returns filtered content or a message if only thinking content remains
fn process_reasoning_content(content: &str) -> String {
    let filtered = filter_thinking_tags(content);
    
    // If we have filtered content, return it
    if !filtered.trim().is_empty() {
        return filtered;
    }
    
    // If no content after filtering, return a message
    "The AI response appears to contain only thinking content.".to_string()
}

// Test function for the thinking tag filter (for debugging)
// Can be run to verify <think> tag removal logic
#[allow(dead_code)]
fn test_filter_thinking_tags() {
    // Test basic filtering with <think> tags
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
    
    println!("All thinking tag filter tests passed!");
}

// Main streaming function that handles real-time response with Discord message editing
// Streams the AI's reasoning response, filtering <think> tags in real time
// Handles chunking, message updates, and finalization
async fn stream_reasoning_response(
    messages: Vec<ChatMessage>,
    model: &str,
    config: &LMConfig,
    ctx: &Context,
    initial_msg: &mut Message,
) -> Result<StreamingStats, Box<dyn std::error::Error + Send + Sync>> {
    println!("[DEBUG][REASONING] === STARTING REASONING STREAM RESPONSE ===");
    println!("[DEBUG][REASONING] Model: {}", model);
    println!("[DEBUG][REASONING] Messages count: {}", messages.len());
    println!("[DEBUG][REASONING] Base URL: {}", config.base_url);
    println!("[DEBUG][REASONING] Config timeout: {} seconds, Using: 180 seconds for reasoning operations", config.timeout);
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180)) // 3 minutes for reasoning operations
        .build()?;
    println!("[DEBUG][REASONING] HTTP client created");
        
    let chat_request = ChatRequest {
        model: model.to_string(),
        messages,
        temperature: config.default_temperature,
        max_tokens: config.default_max_tokens,
        stream: true,
    };
    println!("[DEBUG][REASONING] Chat request created - Temperature: {}, Max tokens: {}, Stream: {}", 
        chat_request.temperature, chat_request.max_tokens, chat_request.stream);

    let api_url = format!("{}/v1/chat/completions", config.base_url);
    println!("[DEBUG][REASONING] API URL: {}", api_url);

    // First, test basic connectivity to the server with enhanced error handling
    println!("[DEBUG][REASONING] === TESTING BASIC CONNECTIVITY ===");
    match client.get(&config.base_url).send().await {
        Ok(response) => {
            println!("[DEBUG][REASONING] Basic connectivity test successful - Status: {}", response.status());
        }
        Err(e) => {
            println!("[DEBUG][REASONING] Basic connectivity test failed: {}", e);
            
            // Check if this is a Windows permission error
            let error_msg = format!("{}", e);
            if error_msg.contains("os error 10013") || error_msg.contains("access permissions") {
                return Err(format!(
                    "Windows Network Permission Error (10013): Cannot connect to {}.\n\n**Common Solutions:**\n\
                    1. **Windows Firewall:** Allow the application through Windows Defender Firewall\n\
                    2. **Network Access:** Ensure the AI server at {} is running and accessible\n\
                    3. **Port Access:** Check if port 11434 is blocked by antivirus or firewall\n\
                    4. **Local Network:** Try using localhost (127.0.0.1) instead of {} if running locally\n\
                    5. **Administrator:** Try running as administrator if needed\n\n\
                    **Original error:** {}", 
                    config.base_url, config.base_url, config.base_url.replace("http://", ""), e
                ).into());
            } else if error_msg.contains("timeout") || error_msg.contains("timed out") {
                return Err(format!(
                    "Connection Timeout: Cannot reach AI server at {}.\n\n**Check:**\n\
                    1. AI server is running and accessible\n\
                    2. Network connection is stable\n\
                    3. Server is not overloaded\n\n\
                    **Original error:** {}", 
                    config.base_url, e
                ).into());
            } else if error_msg.contains("refused") || error_msg.contains("connection refused") {
                return Err(format!(
                    "Connection Refused: AI server at {} is not accepting connections.\n\n**Check:**\n\
                    1. AI server (LM Studio/Ollama) is running\n\
                    2. Server is listening on the correct port\n\
                    3. No other application is using the port\n\n\
                    **Original error:** {}", 
                    config.base_url, e
                ).into());
            } else {
                return Err(format!("Cannot reach remote server {}: {}", config.base_url, e).into());
            }
        }
    }

    // Now attempt the actual streaming API call
    println!("[DEBUG][REASONING] === MAKING STREAMING API REQUEST ===");
    let response = match client
        .post(&api_url)
        .json(&chat_request)
        .send()
        .await
    {
        Ok(resp) => {
            println!("[DEBUG][REASONING] API request sent successfully - Status: {}", resp.status());
            resp
        }
        Err(e) => {
            println!("[DEBUG][REASONING] API request failed: {}", e);
            
            // Enhanced error handling for API requests
            let error_msg = format!("{}", e);
            if error_msg.contains("os error 10013") || error_msg.contains("access permissions") {
                return Err(format!(
                    "Windows Network Permission Error (10013): Cannot connect to AI API at {}.\n\n**Solutions:**\n\
                    1. **Windows Firewall:** Add firewall exception for this application\n\
                    2. **Run as Administrator:** Try running the bot as administrator\n\
                    3. **Check AI Server:** Ensure LM Studio/Ollama is running at {}\n\
                    4. **Port Access:** Verify port 11434 isn't blocked\n\
                    5. **Network Config:** Try localhost (127.0.0.1) if running locally\n\n\
                    **Original error:** {}", 
                    api_url, config.base_url, e
                ).into());
            } else {
                return Err(format!("Streaming API request to {} failed: {}", api_url, e).into());
            }
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
        println!("[DEBUG][REASONING] API returned error status {}: {}", status, error_text);
        return Err(format!("Streaming API request failed: HTTP {} - {}", status, error_text).into());
    }

    println!("[DEBUG][REASONING] === BUFFERING COMPLETE RESPONSE ===");
    let mut stream = response.bytes_stream();
    
    let mut raw_response = String::new();
    let mut chunk_count = 0;
    let mut line_buffer = String::new();
    let mut received_any_content = false;
    let mut stream_complete = false;

    println!("[DEBUG][REASONING] Starting to buffer response from API...");

    // STEP 1: Buffer the complete response from the API
    while let Some(chunk) = stream.next().await {
        if stream_complete {
            println!("[DEBUG][REASONING] Stream marked as complete, stopping buffering");
            break;
        }
        
        match chunk {
            Ok(bytes) => {
                chunk_count += 1;
                if chunk_count == 1 {
                    println!("[DEBUG][REASONING] Received first chunk ({} bytes)", bytes.len());
                } else if chunk_count % 10 == 0 {
                    println!("[DEBUG][REASONING] Buffered {} chunks, total response: {} chars", chunk_count, raw_response.len());
                }
                
                line_buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(i) = line_buffer.find('\n') {
                    let line = line_buffer.drain(..=i).collect::<String>();
                    let line = line.trim();

                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str.trim() == "[DONE]" {
                            println!("[DEBUG][REASONING] Received [DONE] signal, marking stream complete");
                            stream_complete = true;
                            break;
                        }

                        if let Ok(response_chunk) = serde_json::from_str::<ChatResponse>(json_str) {
                            for choice in response_chunk.choices {
                                if let Some(finish_reason) = choice.finish_reason {
                                    if finish_reason == "stop" {
                                        println!("[DEBUG][REASONING] Received finish_reason=stop, marking stream complete");
                                        stream_complete = true;
                                        break;
                                    }
                                }

                                if let Some(delta) = choice.delta {
                                    if let Some(content) = delta.content {
                                        received_any_content = true;
                                        raw_response.push_str(&content);
                                        println!("[DEBUG][REASONING] Added content chunk: '{}' (total: {} chars)", 
                                            content, raw_response.len());
                                    } else {
                                        println!("[DEBUG][REASONING] Delta has no content field");
                                    }
                                } else {
                                    println!("[DEBUG][REASONING] Choice has no delta field");
                                }
                            }
                        } else {
                            if !json_str.trim().is_empty() {
                                println!("[DEBUG][REASONING] Failed to parse JSON chunk: {}", json_str);
                            }
                        }
                    }
                }
                
                // Break out of the outer loop if stream is complete
                if stream_complete {
                    println!("[DEBUG][REASONING] Breaking out of chunk processing loop");
                    break;
                }
            }
            Err(e) => {
                eprintln!("[DEBUG][REASONING] Stream error: {}", e);
                return Err(e.into());
            }
        }
    }

    println!("[DEBUG][REASONING] === BUFFERING COMPLETE ===");
    println!("[DEBUG][REASONING] Buffered {} chunks, total response: {} chars", chunk_count, raw_response.len());
    println!("[DEBUG][REASONING] Raw response content: '{}'", raw_response);
    
    if !received_any_content {
        eprintln!("[DEBUG][REASONING] No content received from API stream");
        return Err("No content received from API stream".into());
    }
    
    // Check for zero character response
    if raw_response.is_empty() {
        eprintln!("[DEBUG][REASONING] ERROR: API returned 0 characters in response");
        eprintln!("[DEBUG][REASONING] Chunk count: {}", chunk_count);
        eprintln!("[DEBUG][REASONING] Received any content flag: {}", received_any_content);
        return Err("API returned 0 characters in response - this indicates a problem with the API or model".into());
    }

    // STEP 2: Process the buffered content and stream to Discord
    println!("[DEBUG][REASONING] === PROCESSING AND STREAMING TO DISCORD ===");
    
    // Apply thinking tag filtering to the complete response
    let filtered_response = filter_thinking_tags(&raw_response);
    println!("[DEBUG][REASONING] Filtered response length: {} chars", filtered_response.len());
    println!("[DEBUG][REASONING] Filtered response content: '{}'", filtered_response);
    
    // Apply reasoning content processing
    let processed_response = process_reasoning_content(&filtered_response);
    println!("[DEBUG][REASONING] Processed response length: {} chars", processed_response.len());
    println!("[DEBUG][REASONING] Processed response content: '{}'", processed_response);
    
    if processed_response.is_empty() {
        println!("[DEBUG][REASONING] Processed response is empty, sending fallback message");
        let _ = initial_msg.edit(&ctx.http, |m| {
            m.content("**Reasoning Response Complete**\n\nThe AI completed its reasoning process, but the response appears to contain only thinking content.")
        }).await;
        
        let stats = StreamingStats {
            total_characters: raw_response.len(),
            message_count: 1,
            filtered_characters: raw_response.len() - filtered_response.len(),
        };
        return Ok(stats);
    }
    
    let mut message_state = MessageState {
        current_content: String::new(),
        current_message: initial_msg.clone(),
        message_index: 1,
        char_limit: config.max_discord_message_length - config.response_format_padding,
    };
    println!("[DEBUG][REASONING] Message state initialized - Char limit: {}", message_state.char_limit);
    
    // Split the processed response into chunks for Discord streaming
    let chunk_size = 100; // Characters per Discord update
    let mut chars_processed = 0;
    
    while chars_processed < processed_response.len() {
        let end_pos = std::cmp::min(chars_processed + chunk_size, processed_response.len());
        let chunk = &processed_response[chars_processed..end_pos];
        
        println!("[DEBUG][REASONING] Streaming chunk {} chars to Discord", chunk.len());
        println!("[DEBUG][REASONING] Chunk content: '{}'", chunk);
        println!("[DEBUG][REASONING] Current state content before update: {} chars", message_state.current_content.len());
        println!("[DEBUG][REASONING] Current state content before update: '{}'", message_state.current_content);
        
        if let Err(e) = update_discord_message(&mut message_state, chunk, ctx, config).await {
            eprintln!("[DEBUG][REASONING] Failed to update Discord message: {}", e);
            return Err(e);
        }
        
        println!("[DEBUG][REASONING] Current state content after update: {} chars", message_state.current_content.len());
        println!("[DEBUG][REASONING] Current state content after update: '{}'", message_state.current_content);
        
        chars_processed = end_pos;
        
        // Small delay to make streaming visible
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Finalize the message
    println!("[DEBUG][REASONING] === FINALIZING DISCORD MESSAGE ===");
    println!("[DEBUG][REASONING] About to finalize with processed content length: {} chars", processed_response.len());
    println!("[DEBUG][REASONING] Final message state content: {} chars", message_state.current_content.len());
    println!("[DEBUG][REASONING] Final message state content: '{}'", message_state.current_content);
    
    // Check if we have content to finalize
    if processed_response.is_empty() {
        eprintln!("[DEBUG][REASONING] ERROR: Cannot finalize - no content was processed from API");
        return Err("No content was processed from API - cannot finalize empty message".into());
    }
    
    // Check if the message state has content (this should catch streaming issues)
    if message_state.current_content.trim().is_empty() {
        eprintln!("[DEBUG][REASONING] ERROR: Message state has no content despite processed response");
        eprintln!("[DEBUG][REASONING] Processed response length: {} chars", processed_response.len());
        eprintln!("[DEBUG][REASONING] Message state content length: {} chars", message_state.current_content.len());
        return Err("Message state has no content despite processed response - streaming to Discord failed".into());
    }
    
    if let Err(e) = finalize_message_content(&mut message_state, "", ctx, config).await {
        eprintln!("[DEBUG][REASONING] Failed to finalize Discord message: {}", e);
        return Err(e);
    }

    let stats = StreamingStats {
        total_characters: raw_response.len(),
        message_count: message_state.message_index,
        filtered_characters: raw_response.len() - filtered_response.len(),
    };

    println!("[DEBUG][REASONING] === REASONING STREAMING COMPLETED ===");
    println!("[DEBUG][REASONING] Final stats - Total chars: {}, Messages: {}, Filtered chars: {}", 
        stats.total_characters, stats.message_count, stats.filtered_characters);
    Ok(stats)
}

// Helper function to update Discord message with new content for reasoning
// Handles chunking and message creation if content exceeds Discord's limit
#[allow(unused_variables)]
async fn update_discord_message(
    state: &mut MessageState,
    new_content: &str,
    ctx: &Context,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("[DEBUG][REASONING_UPDATE] Updating Discord message with {} chars", new_content.len());
    
    println!("[DEBUG][REASONING_UPDATE] New content to add: '{}' ({} chars)", new_content, new_content.len());
    
    // First, add the new content to the state
    if state.current_content.is_empty() {
        println!("[DEBUG][REASONING_UPDATE] State content was empty, setting to new content");
        state.current_content = new_content.to_string();
    } else {
        println!("[DEBUG][REASONING_UPDATE] State content was not empty, appending new content");
        state.current_content.push_str(new_content);
    }
    
    println!("[DEBUG][REASONING_UPDATE] State content after adding: '{}' ({} chars)", state.current_content, state.current_content.len());
    
    // Then create the formatted content for Discord
    let potential_content = format!("**Reasoning Response (Part {}):**\n```\n{}\n```", 
        state.message_index, state.current_content);
    
    println!("[DEBUG][REASONING_UPDATE] Formatted content for Discord: '{}' ({} chars)", potential_content, potential_content.len());

    // Check if we need to create a new message
    if potential_content.len() > state.char_limit {
        println!("[DEBUG][REASONING_UPDATE] Content exceeds limit ({} > {}), creating new message", 
            potential_content.len(), state.char_limit);
        
        // Finalize current message
        let final_content = format!("**Reasoning Response (Part {}):**\n```\n{}\n```", 
            state.message_index, state.current_content);
        let edit_result = state.current_message.edit(&ctx.http, |m| {
            m.content(final_content)
        }).await;
        if let Err(e) = edit_result {
            eprintln!("[ERROR][REASONING_UPDATE] Failed to finalize message part {}: {}", state.message_index, e);
        } else {
            println!("[DEBUG][REASONING_UPDATE] Finalized message part {}", state.message_index);
        }

        // Create new message
        state.message_index += 1;
        // Reset current_content for the new message
        state.current_content = new_content.to_string();
        let new_msg_content = format!("**Reasoning Response (Part {}):**\n```\n{}\n```", 
            state.message_index, state.current_content);
        let send_result = state.current_message.channel_id.send_message(&ctx.http, |m| {
            m.content(new_msg_content)
        }).await;
        match send_result {
            Ok(new_message) => {
                println!("[DEBUG][REASONING_UPDATE] Created new message part {}", state.message_index);
                state.current_message = new_message;
            }
            Err(e) => {
                eprintln!("[ERROR][REASONING_UPDATE] Failed to create new message part {}: {}", state.message_index, e);
            }
        }
    } else {
        // Update current message
        println!("[DEBUG][REASONING_UPDATE] Updating existing message part {}", state.message_index);
        let edit_result = state.current_message.edit(&ctx.http, |m| {
            m.content(&potential_content)
        }).await;
        if let Err(e) = edit_result {
            eprintln!("[ERROR][REASONING_UPDATE] Failed to update existing message part {}: {}", state.message_index, e);
        }
    }

    Ok(())
}

// Helper function to finalize message content at the end of streaming for reasoning
// Ensures all remaining content is posted and marks the message as complete
#[allow(unused_variables)]
async fn finalize_message_content(
    state: &mut MessageState,
    remaining_content: &str,
    ctx: &Context,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("[DEBUG][REASONING_FINALIZE] Finalizing message with {} chars", remaining_content.len());
    println!("[DEBUG][REASONING_FINALIZE] Current state content: {} chars", state.current_content.len());
    
    // Check for zero content error condition - this should catch cases where API returned content but it wasn't streamed properly
    if remaining_content.is_empty() && state.current_content.trim().is_empty() {
        eprintln!("[DEBUG][REASONING_FINALIZE] ERROR: Attempting to finalize message with 0 total characters");
        eprintln!("[DEBUG][REASONING_FINALIZE] Remaining content: '{}' ({} chars)", remaining_content, remaining_content.len());
        eprintln!("[DEBUG][REASONING_FINALIZE] State content: '{}' ({} chars)", state.current_content, state.current_content.len());
        eprintln!("[DEBUG][REASONING_FINALIZE] This indicates either:");
        eprintln!("[DEBUG][REASONING_FINALIZE] 1. No content was received from the API");
        eprintln!("[DEBUG][REASONING_FINALIZE] 2. Content was received but not properly streamed to Discord");
        eprintln!("[DEBUG][REASONING_FINALIZE] 3. The update_discord_message function failed to populate current_content");
        return Err("Cannot finalize message with 0 characters - this indicates no content was received from the API or streaming failed".into());
    }
    
    // Add any remaining content if provided
    if !remaining_content.trim().is_empty() {
        update_discord_message(state, remaining_content, ctx, config).await?;
    }
    
    // Check if we have any content to finalize (either from remaining_content or existing state)
    if state.current_content.trim().is_empty() {
        println!("[DEBUG][REASONING_FINALIZE] No content to finalize");
        return Ok(());
    }
    
    // Mark the final message as complete
    let final_display = if state.message_index == 1 {
        format!("**Reasoning Response Complete**\n```\n{}\n```", state.current_content)
    } else {
        format!("**Reasoning Response Complete (Part {}/{})**\n```\n{}\n```", 
            state.message_index, state.message_index, state.current_content)
    };

    println!("[DEBUG][REASONING_FINALIZE] Marking message as complete - Part {}", state.message_index);
    let edit_result = state.current_message.edit(&ctx.http, |m| {
        m.content(final_display)
    }).await;
    if let Err(e) = edit_result {
        eprintln!("[ERROR][REASONING_FINALIZE] Failed to finalize Discord message part {}: {}", state.message_index, e);
    }

    Ok(())
}

/// Perform reasoning-enhanced search with direct user query and analytical summarization
/// Steps:
///   1. Use user's query for web search
///   2. Perform web search (SerpAPI)
///   3. Analyze results with reasoning model (streamed)
///   4. Post summary to Discord
async fn perform_reasoning_enhanced_search(
    user_query: &str,
    config: &LMConfig,
    search_msg: &mut Message,
    ctx: &Context,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Step 1: Use the user's exact query for searching (no refinement)
    println!("Using exact user query for reasoning search: '{}'", user_query);
    
    // Update message to show search progress
    search_msg.edit(&ctx.http, |m| {
        m.content("Searching with your exact query...")
    }).await.map_err(|e| format!("Failed to update message: {}", e))?;

    // Step 2: Perform the web search with user's exact query
    let results = crate::commands::search::multi_search(user_query).await
        .map_err(|e| format!("Search failed: {}", e))?;
    
    // Update message to show reasoning analysis progress
    search_msg.edit(&ctx.http, |m| {
        m.content("Analyzing search results with reasoning model...")
    }).await.map_err(|e| format!("Failed to update message: {}", e))?;

    // Step 3: Analyze the search results using reasoning model with embedded links
    // This function now handles its own streaming and metadata
    analyze_search_results_with_reasoning(&results, user_query, user_query, config, ctx, search_msg).await?;

    Ok(())
}

/// Analyze search results using reasoning model with embedded links, streaming the response
/// Formats search results, builds prompt, and streams analytical summary to Discord
async fn analyze_search_results_with_reasoning(
    results: &[crate::commands::search::SearchResult],
    _search_query: &str,
    user_query: &str,
    config: &LMConfig,
    ctx: &Context,
    search_msg: &mut Message,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Reasoning analysis: Generating analytical summary for {} results", results.len());
    
    let analysis_prompt = load_reasoning_search_analysis_prompt().await.unwrap_or_else(|_| {
        "You are an expert analytical reasoner. Analyze these web search results to provide a concise, logical analysis. Use Discord formatting and embed relevant links using [title](URL) format. CRITICAL: Your ENTIRE response must be under 1200 characters including all formatting. Be extremely concise.".to_string()
    });

    // Format the results for the reasoning model with both text and links
    let mut formatted_results = String::new();
    for (index, result) in results.iter().enumerate() {
        formatted_results.push_str(&format!(
            "Source {}: {}\nURL: {}\nContent: {}\n\n",
            index + 1,
            result.title,
            result.link,
            if result.snippet.is_empty() { "No content preview available" } else { &result.snippet }
        ));
    }

    let user_prompt = format!(
        "User's search query: {}\n\nSources to analyze:\n{}\n\nProvide a VERY CONCISE analytical response that:\n1. Addresses the user's question directly\n2. Cites 2-3 sources with [title](URL) format\n3. Uses clear reasoning\n\nCRITICAL: Keep your ENTIRE response under 1200 characters. Be extremely concise and direct.",
        user_query, formatted_results
    );

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: analysis_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_prompt,
        },
    ];

    // Create a new initial message for the streaming response
    let mut analysis_msg = match search_msg.channel_id.send_message(&ctx.http, |m| {
        m.content("Analyzing search results with reasoning model...")
    }).await {
        Ok(message) => message,
        Err(e) => {
            eprintln!("Failed to send initial analysis message: {}", e);
            return Err(format!("Failed to send analysis message: {}", e).into());
        }
    };

    // Use dedicated streaming function for reason -s with proper chunking
    let _stats = stream_reasoning_search_response(messages, &config.default_reason_model, config, ctx, &mut analysis_msg).await?;
    
    // Add search metadata to the final message
    let final_msg = format!(
        "\n\n---\n*Reasoning Search: {}*",
        user_query
    );
    
    // Try to append the metadata to the last message, or create a new one if it's too long
    let current_content = &analysis_msg.content;
    let potential_content = format!("{}{}", current_content, final_msg);
    
    if potential_content.len() <= 2000 { // Adjusted limit for better performance
        // Can fit in current message
        analysis_msg.edit(&ctx.http, |m| {
            m.content(&potential_content)
        }).await.map_err(|e| format!("Failed to update final message: {}", e))?;
    } else {
        // Need to create a new message for metadata
        analysis_msg.channel_id.send_message(&ctx.http, |m| {
            m.content(&final_msg)
        }).await.map_err(|e| format!("Failed to send metadata message: {}", e))?;
    }
    
    Ok(())
}

// Dedicated streaming function for reason -s with proper message chunking
// Buffers content and posts in 2000-character chunks for Discord
async fn stream_reasoning_search_response(
    messages: Vec<ChatMessage>,
    model: &str,
    config: &LMConfig,
    ctx: &Context,
    initial_msg: &mut Message,
) -> Result<StreamingStats, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180)) // 3 minutes for reasoning search operations
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
    let mut raw_response = String::new();
    let mut filtered_buffer = String::new();
    let mut message_count = 1;
    let mut current_message = initial_msg.clone();
    let char_limit = config.max_discord_message_length - config.response_format_padding;

    println!("Starting streaming for reasoning search response (buffered chunks)...");

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                
                for line in text.lines() {
                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str.trim() == "[DONE]" {
                            // Post any remaining content in the buffer
                            if !filtered_buffer.trim().is_empty() {
                                if let Err(e) = post_chunked_message(&filtered_buffer, &mut current_message, &mut message_count, ctx, char_limit).await {
                                    eprintln!("Failed to post final chunk: {}", e);
                                }
                            }
                            
                            let stats = StreamingStats {
                                total_characters: raw_response.len(),
                                message_count,
                                filtered_characters: raw_response.len() - filtered_buffer.len(),
                            };
                            
                            return Ok(stats);
                        }
                        
                        if let Ok(response_chunk) = serde_json::from_str::<ChatResponse>(json_str) {    
                            for choice in response_chunk.choices {
                                if let Some(finish_reason) = choice.finish_reason {
                                    if finish_reason == "stop" {
                                        // Post any remaining content in the buffer
                                        if !filtered_buffer.trim().is_empty() {
                                            if let Err(e) = post_chunked_message(&filtered_buffer, &mut current_message, &mut message_count, ctx, char_limit).await {
                                                eprintln!("Failed to post final chunk: {}", e);
                                            }
                                        }
                                        
                                        let stats = StreamingStats {
                                            total_characters: raw_response.len(),
                                            message_count,
                                            filtered_characters: raw_response.len() - filtered_buffer.len(),
                                        };
                                        
                                        return Ok(stats);
                                    }
                                }
                                
                                if let Some(delta) = choice.delta {
                                    if let Some(content) = delta.content {
                                        raw_response.push_str(&content);
                                        
                                        // Apply thinking tag filtering to accumulated content
                                        let new_filtered = filter_thinking_tags(&raw_response);
                                        
                                        // Only update if we have new filtered content
                                        if new_filtered.len() > filtered_buffer.len() {
                                            let new_content = &new_filtered[filtered_buffer.len()..];
                                            filtered_buffer.push_str(new_content);
                                            
                                            // Check if we have enough content to post a chunk
                                            if filtered_buffer.len() >= char_limit {
                                                if let Err(e) = post_chunked_message(&filtered_buffer, &mut current_message, &mut message_count, ctx, char_limit).await {
                                                    eprintln!("Failed to post chunked message: {}", e);
                                                }
                                                // Clear the buffer after posting
                                                filtered_buffer.clear();
                                            }
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

    // Final cleanup - post any remaining content
    if !filtered_buffer.trim().is_empty() {
        if let Err(e) = post_chunked_message(&filtered_buffer, &mut current_message, &mut message_count, ctx, char_limit).await {
            eprintln!("Failed to post final chunk: {}", e);
        }
    } else if !raw_response.trim().is_empty() {
        // Process the raw response
        let processed_content = process_reasoning_content(&raw_response);
        if !processed_content.is_empty() {
            if let Err(e) = post_chunked_message(&processed_content, &mut current_message, &mut message_count, ctx, char_limit).await {
                eprintln!("Failed to post processed content chunk: {}", e);
            }
        }
    }

    let stats = StreamingStats {
        total_characters: raw_response.len(),
        message_count,
        filtered_characters: raw_response.len() - filtered_buffer.len(),
    };

    Ok(stats)
}

// Helper function to post content in chunks with proper message creation
// Used for buffered chunking in reason -s
async fn post_chunked_message(
    content: &str,
    current_message: &mut Message,
    message_count: &mut usize,
    ctx: &Context,
    char_limit: usize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if content.trim().is_empty() {
        return Ok(());
    }

    // Split content into chunks that fit within Discord's limit
    let mut remaining_content = content;

    while !remaining_content.trim().is_empty() {
        let chunk_size = if remaining_content.len() > char_limit {
            // Find a good breaking point (end of sentence or word boundary)
            let mut break_point = char_limit;
            if let Some(last_period) = remaining_content[..char_limit].rfind('.') {
                break_point = last_period + 1;
            } else if let Some(last_space) = remaining_content[..char_limit].rfind(' ') {
                break_point = last_space;
            }
            break_point
        } else {
            remaining_content.len()
        };

        let chunk = &remaining_content[..chunk_size];
        remaining_content = &remaining_content[chunk_size..];

        // Create a new message for this chunk
        let message_content = format!("**Analytical Summary (Part {}):**\n\n{}", *message_count, chunk);
        let new_message = current_message.channel_id.send_message(&ctx.http, |m| {
            m.content(message_content)
        }).await?;

        *current_message = new_message;
        *message_count += 1;
    }

    Ok(())
}

/// Load reasoning-specific search analysis prompt
/// Loads prompt for analytical search summarization (multi-path fallback)
async fn load_reasoning_search_analysis_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Try to load reasoning-specific search analysis prompt first, fall back to general prompts
    let prompt_paths = [
        "reasoning_search_analysis_prompt.txt",
        "../reasoning_search_analysis_prompt.txt",
        "../../reasoning_search_analysis_prompt.txt",
        "src/reasoning_search_analysis_prompt.txt",
        "summarize_search_prompt.txt",
        "../summarize_search_prompt.txt", 
        "../../summarize_search_prompt.txt",
        "src/summarize_search_prompt.txt",
        "example_summarize_search_prompt.txt",
        "../example_summarize_search_prompt.txt",
        "../../example_summarize_search_prompt.txt", 
        "src/example_summarize_search_prompt.txt",
    ];
    
    for path in &prompt_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                // Remove BOM if present  
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                println!("Reasoning analysis: Loaded prompt from {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    // Final fallback prompt
    Ok("You are an expert analytical reasoner. Analyze these web search results to provide a comprehensive, logical analysis. Focus on reasoning through the information, identifying patterns, and providing insights. Use Discord formatting and embed relevant links naturally using [title](URL) format.".to_string())
}

/// Non-streaming chat completion specifically for reasoning tasks
/// Used for short, non-streamed completions (e.g., query refinement)
async fn chat_completion_reasoning(
    messages: Vec<ChatMessage>,
    model: &str,
    config: &LMConfig,
    max_tokens: Option<i32>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180)) // 3 minutes for reasoning completion operations
        .build()?;
        
    let chat_request = ChatRequest {
        model: model.to_string(),
        messages,
        temperature: 0.5, // Slightly higher temperature for reasoning tasks
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
    
    Err("Failed to extract content from reasoning API response".into())
} 

 