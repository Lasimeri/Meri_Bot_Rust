// search.rs - Search and Configuration Module
// This module handles LM Studio/Ollama configuration loading and search functionality

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use log::warn;

// Chat message structure for context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

// LM configuration structure
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
    pub default_vision_model: String,
}

// Search result structure
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub link: String,
    pub snippet: String,
}

/// Load LM Studio/Ollama configuration from lmapiconf.txt file
/// This reads the configuration file and returns a properly configured LMConfig
pub async fn load_lm_config() -> Result<LMConfig, Box<dyn std::error::Error + Send + Sync>> {
    let config_paths = [
        "lmapiconf.txt",
        "../lmapiconf.txt", 
        "../../lmapiconf.txt",
        "src/lmapiconf.txt"
    ];
    
    let mut config_content = String::new();
    let mut config_file_found = false;
    
    // Try to read from multiple possible locations
    for path in &config_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                config_content = content;
                config_file_found = true;
                println!("Configuration loaded from: {}", path);
                break;
            }
            Err(_) => continue,
        }
    }
    
    if !config_file_found {
        return Err("lmapiconf.txt file not found in any expected location (., .., ../.., src/)".into());
    }
    
    // Parse configuration
    let mut config_map = HashMap::new();
    for line in config_content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        
        if let Some(equals_pos) = line.find('=') {
            let key = line[..equals_pos].trim().to_string();
            let value = line[equals_pos + 1..].trim().to_string();
            config_map.insert(key, value);
        }
    }
    
    // Extract configuration values with validation
    let base_url = config_map.get("LM_STUDIO_BASE_URL")
        .ok_or("LM_STUDIO_BASE_URL not found in lmapiconf.txt")?
        .clone();
    
    let timeout = config_map.get("LM_STUDIO_TIMEOUT")
        .ok_or("LM_STUDIO_TIMEOUT not found in lmapiconf.txt")?
        .parse::<u64>()
        .map_err(|_| "LM_STUDIO_TIMEOUT must be a valid number")?;
    
    let default_model = config_map.get("DEFAULT_MODEL")
        .ok_or("DEFAULT_MODEL not found in lmapiconf.txt")?
        .clone();
    
    let default_reason_model = config_map.get("DEFAULT_REASON_MODEL")
        .ok_or("DEFAULT_REASON_MODEL not found in lmapiconf.txt")?
        .clone();
    
    let default_vision_model = config_map.get("DEFAULT_VISION_MODEL")
        .ok_or("DEFAULT_VISION_MODEL not found in lmapiconf.txt")?
        .clone();
    
    let default_temperature = config_map.get("DEFAULT_TEMPERATURE")
        .ok_or("DEFAULT_TEMPERATURE not found in lmapiconf.txt")?
        .parse::<f32>()
        .map_err(|_| "DEFAULT_TEMPERATURE must be a valid number")?;
    
    let default_max_tokens = config_map.get("DEFAULT_MAX_TOKENS")
        .ok_or("DEFAULT_MAX_TOKENS not found in lmapiconf.txt")?
        .parse::<i32>()
        .map_err(|_| "DEFAULT_MAX_TOKENS must be a valid number")?;
    
    let max_discord_message_length = config_map.get("MAX_DISCORD_MESSAGE_LENGTH")
        .ok_or("MAX_DISCORD_MESSAGE_LENGTH not found in lmapiconf.txt")?
        .parse::<usize>()
        .map_err(|_| "MAX_DISCORD_MESSAGE_LENGTH must be a valid number")?;
    
    let response_format_padding = config_map.get("RESPONSE_FORMAT_PADDING")
        .ok_or("RESPONSE_FORMAT_PADDING not found in lmapiconf.txt")?
        .parse::<usize>()
        .map_err(|_| "RESPONSE_FORMAT_PADDING must be a valid number")?;
    
    Ok(LMConfig {
        base_url,
        timeout,
        default_model,
        default_reason_model,
        default_temperature,
        default_max_tokens,
        max_discord_message_length,
        response_format_padding,
        default_vision_model,
    })
}

/// Test API connectivity to the configured LM Studio/Ollama server
pub async fn test_api_connectivity(config: &LMConfig) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout))
        .build()?;
    
    println!("Testing connectivity to: {}", config.base_url);
    
    // Test basic connectivity
    let response = client.get(&config.base_url).send().await?;
    
    if response.status().is_success() {
        Ok(format!("✅ Successfully connected to {} (Status: {})", config.base_url, response.status()))
    } else {
        Err(format!("❌ Server responded with status: {}", response.status()).into())
    }
}

/// Placeholder for multi-search functionality
pub async fn multi_search(_query: &str) -> Result<Vec<SearchResult>, Box<dyn std::error::Error + Send + Sync>> {
    // Placeholder - return empty results
    Ok(Vec::new())
}

/// Chat completion functionality for AI-enhanced features
/// Handles both regular chat and reasoning tasks with model capability detection
pub async fn chat_completion(
    messages: Vec<ChatMessage>,
    model: &str,
    config: &LMConfig,
    max_tokens: Option<i32>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
        
    let chat_request = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": config.default_temperature,
        "max_tokens": max_tokens.unwrap_or(config.default_max_tokens),
        "stream": false
    });

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

/// AI-enhanced search functionality
/// Uses the default model to enhance search queries and summarize results
pub async fn perform_ai_enhanced_search(
    query: &str,
    config: &LMConfig,
    search_msg: &mut serenity::model::channel::Message,
    ctx: &serenity::prelude::Context,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Step 1: Use the user's exact query for searching (no refinement for now)
    println!("Using exact user query for AI-enhanced search: '{}'", query);
    
    // Update message to show search progress
    search_msg.edit(&ctx.http, |m| {
        m.content("Searching with your exact query...")
    }).await.map_err(|e| format!("Failed to update message: {}", e))?;

    // Step 2: Perform the web search with user's exact query
    let results = multi_search(query).await
        .map_err(|e| format!("Search failed: {}", e))?;
    
    // Update message to show AI analysis progress
    search_msg.edit(&ctx.http, |m| {
        m.content("Analyzing search results with AI...")
    }).await.map_err(|e| format!("Failed to update message: {}", e))?;

    // Step 3: Analyze the search results using the default model
    analyze_search_results_with_ai(&results, query, config, ctx, search_msg).await?;

    Ok(())
}

/// Analyze search results using the default AI model
/// Formats search results, builds prompt, and provides AI-enhanced summary
async fn analyze_search_results_with_ai(
    results: &[SearchResult],
    user_query: &str,
    config: &LMConfig,
    ctx: &serenity::prelude::Context,
    search_msg: &mut serenity::model::channel::Message,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("AI analysis: Generating summary for {} results", results.len());
    
    let analysis_prompt = "You are an expert research analyst. Analyze these web search results to provide a concise, informative summary. Use Discord formatting and embed relevant links using [title](URL) format. CRITICAL: Your ENTIRE response must be under 1200 characters including all formatting. Be extremely concise.";

    // Format the results for the AI model
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
        "User's search query: {}\n\nSources to analyze:\n{}\n\nProvide a VERY CONCISE summary that:\n1. Addresses the user's question directly\n2. Cites 2-3 sources with [title](URL) format\n3. Provides key insights\n\nCRITICAL: Keep your ENTIRE response under 1200 characters. Be extremely concise and direct.",
        user_query, formatted_results
    );

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: analysis_prompt.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_prompt,
        },
    ];

    // Use the default model for AI-enhanced search (not the reasoning model)
    let ai_response = chat_completion(messages, &config.default_model, config, Some(800)).await?;
    
    // Check if the model returned a fallback message
    if ai_response.contains("Search functionality is not available") || ai_response.contains("fallback") {
        warn!("⚠️ Model {} appears to be a search model, not suitable for AI-enhanced search", config.default_model);
        
        // Provide a user-friendly error message
        let error_message = format!(
            "❌ **AI-Enhanced Search Failed**\n\n**Issue:** The AI model `{}` appears to be a search/retrieval model, not suitable for content analysis.\n\n**Solution:** Please update your `lmapiconf.txt` to use a chat/completion model for `DEFAULT_MODEL`.\n\n**Recommended models:**\n• `llama3.2:3b`\n• `llama3.2:7b`\n• `qwen2.5:4b`\n• `qwen2.5:7b`\n• `mistral:7b`\n\n*Search query: {}*",
            config.default_model, user_query
        );
        
        search_msg.edit(&ctx.http, |m| m.content(&error_message)).await?;
        return Ok(());
    }
    
    // Format the final response
    let final_message = format!(
        "**AI-Enhanced Search Results**\n\n{}\n\n---\n*Search query: {}*",
        ai_response, user_query
    );
    
    search_msg.edit(&ctx.http, |m| m.content(&final_message)).await?;
    
    Ok(())
} 