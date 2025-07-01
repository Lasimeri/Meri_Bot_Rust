use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};
use std::fs;
use std::collections::HashMap;
use crate::lm::{LMConfig, buffer_chat_response, send_buffered_response, ChatMessage};

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

    // Buffer the complete response from streaming using the reasoning model
    match buffer_chat_response(messages, &config.default_reason_model, &config, ctx, &mut current_msg).await {
        Ok(response_content) => {
            // Filter out thinking tags and content before sending to Discord
            let filtered_content = filter_thinking_tags(&response_content);
            
            // Log the filtering results for debugging
            println!("🧠 Reasoning command: Raw response length: {} characters", response_content.len());
            println!("🧠 Reasoning command: Filtered response length: {} characters", filtered_content.len());
            
            if filtered_content.len() != response_content.len() {
                println!("🧠 Reasoning command: Successfully filtered thinking content");
            }
            
            // Check if we have any content left after filtering
            if filtered_content.trim().is_empty() {
                let _ = current_msg.edit(&ctx.http, |m| {
                    m.content("🧠 **Reasoning Complete** ✅\n\nThe AI completed its reasoning process, but the response appears to contain only thinking content. The model may have used `<think>` tags for the entire response.")
                }).await;
                return Ok(());
            }
            
            // Send the filtered response in Discord-sized chunks
            if let Err(e) = send_buffered_response(ctx, &mut current_msg, &filtered_content, &config).await {
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
    
    // Try to find the config file in multiple locations
    for config_path in &config_paths {
        match fs::read_to_string(config_path) {
            Ok(file_content) => {
                content = file_content;
                found_file = true;
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
            return Err(format!("❌ Required setting '{}' not found in lmapiconf.txt (reasoning command)", key).into());
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

    println!("🧠 Reasoning command: Successfully loaded config with reasoning model: '{}'", config.default_reason_model);
    Ok(config)
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
            println!("🧠 Reasoning command: Loaded prompt from {}", path);
            return Ok(content.trim().to_string());
        }
    }
    
    Err("No reasoning prompt file found".into())
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