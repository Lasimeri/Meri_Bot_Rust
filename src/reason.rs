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
use crate::ReasonContextMap; // TypeMap key defined in main.rs

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
        reason_map.remove(&msg.author.id);
        msg.reply(ctx, "**Reasoning Context Cleared**\nYour reasoning conversation history has been reset.").await?;
        return Ok(());
    }

    // Regular reasoning functionality
    let question = input;

    // Record user question in per-user context history
    {
        let mut data_map = ctx.data.write().await;
        let reason_map = data_map.get_mut::<ReasonContextMap>().expect("Reason context map not initialized");
        let history = reason_map.entry(msg.author.id).or_insert_with(Vec::new);
        history.push(ChatMessage { role: "user".to_string(), content: question.to_string() });
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
        if let Some(history) = reason_map.get(&msg.author.id) {
            println!("Reasoning command: Including {} context messages for user {}", history.len(), msg.author.name);
            for entry in history.iter() {
                messages.push(entry.clone());
            }
        } else {
            println!("Reasoning command: No context history found for user {}", msg.author.name);
        }
    }

    // Send initial "thinking" message
    let mut current_msg = match msg.channel_id.send_message(&ctx.http, |m| {
        m.content("**Reasoning Analysis (Part 1):**\n```\n\n```")
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
            if let Some(history) = reason_map.get_mut(&msg.author.id) {
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
            eprintln!("Failed to stream reasoning response: {}", e);
            let _ = current_msg.edit(&ctx.http, |m| {
                m.content("Failed to get reasoning response!")
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
        "DEFAULT_TEMPERATURE",
        "DEFAULT_MAX_TOKENS",
        "MAX_DISCORD_MESSAGE_LENGTH",
        "RESPONSE_FORMAT_PADDING"
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

    println!("Reasoning command: Successfully loaded config from {} with reasoning model: '{}'", config_source, config.default_reason_model);
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
                println!("Reasoning command: Loaded prompt from {}", path);
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
    
    println!("All thinking tag filter tests passed!");
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
        .connect_timeout(std::time::Duration::from_secs(config.timeout * 3))
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
    let update_interval = std::time::Duration::from_millis(800);
    let mut line_buffer = String::new();

    println!("Starting real-time streaming for reasoning response...");

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                line_buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(i) = line_buffer.find('\n') {
                    let line = line_buffer.drain(..=i).collect::<String>();
                    let line = line.trim();
                    
                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str.trim() == "[DONE]" {
                            if !content_buffer.is_empty() {
                                let final_filtered_content = filter_thinking_tags(&content_buffer);
                                if !final_filtered_content.is_empty() {
                                    if let Err(e) = finalize_message_content(&mut message_state, &final_filtered_content, ctx, config).await {
                                        eprintln!("Failed to finalize message: {}", e);
                                    }
                                }
                            } else if message_state.current_content.trim().is_empty() {
                                let _ = message_state.current_message.edit(&ctx.http, |m| {
                                    m.content("**Reasoning Complete**\n\nThe AI completed its reasoning process, but the response appears to contain only thinking content.")
                                }).await;
                            }
                            
                            let stats = StreamingStats {
                                total_characters: raw_response.len(),
                                message_count: message_state.message_index,
                                filtered_characters: raw_response.len() - message_state.current_content.len(),
                            };
                            return Ok(stats);
                        }
                        
                        if let Ok(response_chunk) = serde_json::from_str::<ChatResponse>(json_str) {    
                            for choice in response_chunk.choices {
                                if let Some(finish_reason) = choice.finish_reason {
                                    if finish_reason == "stop" {
                                        if !content_buffer.is_empty() {
                                            let final_filtered_content = filter_thinking_tags(&content_buffer);
                                            if !final_filtered_content.is_empty() {
                                                if let Err(e) = finalize_message_content(&mut message_state, &final_filtered_content, ctx, config).await {
                                                    eprintln!("Failed to finalize message: {}", e);
                                                }
                                            }
                                        } else if message_state.current_content.trim().is_empty() {
                                            let _ = message_state.current_message.edit(&ctx.http, |m| {
                                                m.content("**Reasoning Complete**\n\nThe AI completed its reasoning process, but the response appears to contain only thinking content.")
                                            }).await;
                                        }
                                        
                                        let stats = StreamingStats {
                                            total_characters: raw_response.len(),
                                            message_count: message_state.message_index,
                                            filtered_characters: raw_response.len() - message_state.current_content.len(),
                                        };
                                        return Ok(stats);
                                    }
                                }
                                
                                if let Some(delta) = choice.delta {
                                    if let Some(content) = delta.content {
                                        raw_response.push_str(&content);
                                        content_buffer.push_str(&content);
                                        
                                        if last_update.elapsed() >= update_interval && !content_buffer.is_empty() {
                                            let filtered_chunk = filter_thinking_tags(&content_buffer);
                                            if !filtered_chunk.is_empty() {
                                                if let Err(e) = update_discord_message(&mut message_state, &filtered_chunk, ctx, config).await {
                                                    eprintln!("Failed to update Discord message: {}", e);
                                                    return Err(e);
                                                }
                                                content_buffer.clear();
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
                eprintln!("Stream error: {}", e);
                if !content_buffer.is_empty() {
                    let final_filtered_content = filter_thinking_tags(&content_buffer);
                    if !final_filtered_content.is_empty() {
                        let _ = finalize_message_content(&mut message_state, &final_filtered_content, ctx, config).await;
                    }
                }
                return Err(e.into());
            }
        }
    }

    if !content_buffer.is_empty() {
        let final_filtered_content = filter_thinking_tags(&content_buffer);
        if !final_filtered_content.is_empty() {
            if let Err(e) = finalize_message_content(&mut message_state, &final_filtered_content, ctx, config).await {
                eprintln!("Failed to finalize remaining content: {}", e);
            }
        }
    } else if message_state.current_content.trim().is_empty() {
        let _ = message_state.current_message.edit(&ctx.http, |m| {
            m.content("**Reasoning Complete**\n\nThe AI completed its reasoning process, but the response appears to contain only thinking content.")
        }).await;
    }

    let stats = StreamingStats {
        total_characters: raw_response.len(),
        message_count: message_state.message_index,
        filtered_characters: raw_response.len() - message_state.current_content.len(),
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
    state.current_content.push_str(new_content);

    let potential_content = format!("**Reasoning Response (Part {}):**\n```\n{}\n```", 
            state.message_index, state.current_content);

    // Check if we need to create a new message
    if potential_content.len() > state.char_limit {
        // Find a good split point for the content that is over the limit
        let mut split_at = state.current_content.len() - new_content.len();
        if let Some(pos) = state.current_content[..split_at].rfind('\n') {
            split_at = pos;
        }

        let (part1, part2) = state.current_content.split_at(split_at);
        
        let final_content = format!("**Reasoning Response (Part {}):**\n```\n{}\n```", 
            state.message_index, part1);
        
        state.current_message.edit(&ctx.http, |m| {
            m.content(final_content)
        }).await?;

        // Create new message
        state.message_index += 1;
        state.current_content = part2.trim_start().to_string();
        let new_msg_content = format!("**Reasoning Response (Part {}):**\n```\n{}\n```", 
            state.message_index, state.current_content);
        
        let new_message = state.current_message.channel_id.send_message(&ctx.http, |m| {
            m.content(new_msg_content)
        }).await?;

        state.current_message = new_message;
    } else {
        // Update current message
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
        format!("**Reasoning Complete**\n```\n{}\n```", state.current_content)
    } else {
        format!("**Reasoning Complete (Part {}/{})**\n```\n{}\n```", 
            state.message_index, state.message_index, state.current_content)
    };

    state.current_message.edit(&ctx.http, |m| {
        m.content(final_display)
    }).await?;

    Ok(())
}

/// Perform reasoning-enhanced search with direct user query and analytical summarization
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
    let results = crate::search::multi_search(user_query).await
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
async fn analyze_search_results_with_reasoning(
    results: &[crate::search::SearchResult],
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
    
    if potential_content.len() <= 2000 {
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
async fn stream_reasoning_search_response(
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
    }

    let stats = StreamingStats {
        total_characters: raw_response.len(),
        message_count,
        filtered_characters: raw_response.len() - filtered_buffer.len(),
    };

    Ok(stats)
}

// Helper function to post content in chunks with proper message creation
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
async fn chat_completion_reasoning(
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