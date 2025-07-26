// search.rs - Search and Configuration Module
// This module handles LM Studio/Ollama configuration loading and search functionality

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::Duration;
use tokio::sync::OnceCell;

use log::warn;

// Global HTTP client for connection pooling and reuse
static HTTP_CLIENT: OnceCell<reqwest::Client> = OnceCell::const_new();

// Initialize shared HTTP client with optimized settings
pub async fn get_http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| async {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(120)) // Increased timeout for LM Studio
            .connect_timeout(Duration::from_secs(30)) // Connection timeout
            .pool_idle_timeout(Duration::from_secs(90)) // Keep connections alive
            .pool_max_idle_per_host(10) // Connection pool size per host
            .danger_accept_invalid_certs(true) // Accept self-signed certificates for local servers
            .tcp_keepalive(Duration::from_secs(60)) // TCP keepalive
            .http2_keep_alive_interval(Duration::from_secs(30)) // HTTP/2 keepalive
            .http2_keep_alive_timeout(Duration::from_secs(10)) // HTTP/2 keepalive timeout
            .http2_keep_alive_while_idle(true) // Keep HTTP/2 alive when idle
            .user_agent("Meri-Bot-Rust-Client/1.0") // Identify the client
            .build()
            .expect("Failed to create HTTP client")
    }).await
}

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
    pub default_summarization_model: String,
    pub default_ranking_model: String,
    pub default_temperature: f32,
    pub default_max_tokens: i32,
    pub max_discord_message_length: usize,
    pub response_format_padding: usize,
    pub default_vision_model: String,
    pub default_seed: Option<i64>, // Optional seed for reproducible responses
}

// Search result structure
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub link: String,
    pub snippet: String,
}

/// Enhanced connectivity test function
pub async fn test_api_connectivity(config: &LMConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = get_http_client().await;
    
    println!("[DEBUG][CONNECTIVITY] Testing API connectivity to: {}", config.base_url);
    
    // Test 1: Basic server connectivity
    let basic_response = client
        .get(&config.base_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await;
    
    match basic_response {
        Ok(response) => {
            println!("[DEBUG][CONNECTIVITY] Basic connectivity OK - Status: {}", response.status());
        }
        Err(e) => {
            let error_msg = format!("{}", e);
            if error_msg.contains("os error 10013") || error_msg.contains("access permissions") {
                return Err(format!(
                    "🚫 **Windows Network Permission Error (10013)**\n\n\
                    Cannot connect to LM Studio at `{}`\n\n\
                    **Solutions:**\n\
                    • **Add Firewall Exception**: Windows Defender Firewall → Allow an app → Add this program\n\
                    • **Run as Administrator**: Try running the bot with administrator privileges\n\
                    • **Check LM Studio**: Ensure LM Studio is running and accessible\n\
                    • **Try localhost**: Use `http://127.0.0.1:1234` instead of `http://localhost:1234`\n\
                    • **Check Port**: Verify no other application is using the port\n\n\
                    **Original error:** {}", 
                    config.base_url, e
                ).into());
            } else if error_msg.contains("timeout") || error_msg.contains("timed out") {
                return Err(format!(
                    "⏰ **Connection Timeout**\n\n\
                    Cannot reach LM Studio server at `{}` within 10 seconds\n\n\
                    **Solutions:**\n\
                    • **Check LM Studio**: Ensure LM Studio is running and responsive\n\
                    • **Network Connection**: Verify your network connection is stable\n\
                    • **Server Load**: LM Studio might be overloaded - wait and retry\n\
                    • **Firewall**: Check if firewall is blocking the connection\n\n\
                    **Original error:** {}", 
                    config.base_url, e
                ).into());
            } else if error_msg.contains("refused") || error_msg.contains("connection refused") {
                return Err(format!(
                    "🚫 **Connection Refused**\n\n\
                    LM Studio at `{}` is not accepting connections\n\n\
                    **Solutions:**\n\
                    • **Start LM Studio**: Make sure LM Studio is running\n\
                    • **Check Port**: Verify LM Studio is listening on the correct port (usually 1234)\n\
                    • **Load Model**: Ensure a model is loaded in LM Studio\n\
                    • **Server Status**: Check LM Studio's server status indicator\n\
                    • **Alternative Port**: Try port 11434 if using Ollama instead\n\n\
                    **Original error:** {}", 
                    config.base_url, e
                ).into());
            } else if error_msg.contains("dns") || error_msg.contains("name resolution") {
                return Err(format!(
                    "🌐 **DNS Resolution Error**\n\n\
                    Cannot resolve hostname in `{}`\n\n\
                    **Solutions:**\n\
                    • **Use IP Address**: Try `http://127.0.0.1:1234` instead of `http://localhost:1234`\n\
                    • **Check Hostname**: Verify the hostname is correct\n\
                    • **DNS Settings**: Check your DNS configuration\n\n\
                    **Original error:** {}", 
                    config.base_url, e
                ).into());
            } else {
                return Err(format!(
                    "🔗 **Connection Error**\n\n\
                    Cannot connect to LM Studio at `{}`\n\n\
                    **Solutions:**\n\
                    • **Check URL**: Verify the base URL in lmapiconf.txt\n\
                    • **Start LM Studio**: Ensure LM Studio is running\n\
                    • **Network**: Check your network connection\n\
                    • **Firewall**: Verify firewall settings\n\n\
                    **Original error:** {}", 
                    config.base_url, e
                ).into());
            }
        }
    }
    
    // Test 2: API endpoint availability
    let api_url = format!("{}/v1/chat/completions", config.base_url);
    let test_payload = serde_json::json!({
        "model": config.default_model,
        "messages": [{"role": "user", "content": "test"}],
        "max_tokens": 1,
        "temperature": 0.1
    });
    
    println!("[DEBUG][CONNECTIVITY] Testing API endpoint: {}", api_url);
    
    let api_response = client
        .post(&api_url)
        .json(&test_payload)
        .timeout(Duration::from_secs(30))
        .send()
        .await;
    
    match api_response {
        Ok(response) => {
            let status = response.status();
            if status.is_success() || status == 400 || status == 422 {
                // 400/422 are acceptable - means API is responding but request format might be wrong
                println!("[DEBUG][CONNECTIVITY] API endpoint OK - Status: {}", status);
                return Ok(());
            } else if status == 404 {
                return Err(format!(
                    "🚫 **API Endpoint Not Found (404)**\n\n\
                    The endpoint `{}` was not found\n\n\
                    **Solutions:**\n\
                    • **Check LM Studio Version**: Ensure you're using a recent version that supports OpenAI API\n\
                    • **Enable API Server**: Make sure the 'Start Server' option is enabled in LM Studio\n\
                    • **Correct Port**: LM Studio usually uses port 1234, Ollama uses 11434\n\
                    • **API Path**: Verify the API path is `/v1/chat/completions`\n\n\
                    **Current URL:** {}", 
                    api_url, config.base_url
                ).into());
            } else {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                return Err(format!(
                    "🚫 **API Error (HTTP {})**\n\n\
                    LM Studio API returned an error\n\n\
                    **Response:** {}\n\n\
                    **Solutions:**\n\
                    • **Load Model**: Ensure a model is loaded in LM Studio\n\
                    • **Check Model Name**: Verify the model name in lmapiconf.txt matches loaded model\n\
                    • **Server Status**: Check LM Studio's status and logs\n\n\
                    **API URL:** {}", 
                    status, error_text, api_url
                ).into());
            }
        }
        Err(e) => {
            // API test failed, but basic connectivity worked, so this might be a model/configuration issue
            println!("[DEBUG][CONNECTIVITY] API test failed but basic connectivity OK: {}", e);
            return Err(format!(
                "⚠️ **API Configuration Issue**\n\n\
                Basic connectivity to `{}` works, but API test failed\n\n\
                **Likely Issues:**\n\
                • **Model Not Loaded**: No model is loaded in LM Studio\n\
                • **Wrong Model Name**: Model name in lmapiconf.txt doesn't match loaded model\n\
                • **API Not Enabled**: LM Studio server is not started\n\
                • **Version Issue**: LM Studio version doesn't support OpenAI API\n\n\
                **Error:** {}", 
                config.base_url, e
            ).into());
        }
    }
}

/// Load LM Studio/Ollama configuration from lmapiconf.txt file with enhanced validation
pub async fn load_lm_config() -> Result<LMConfig, Box<dyn std::error::Error + Send + Sync>> {
    let config_paths = [
        "lmapiconf.txt",
        "../lmapiconf.txt", 
        "../../lmapiconf.txt",
        "src/lmapiconf.txt"
    ];
    
    let mut config_content = String::new();
    let mut config_file_found = false;
    let mut config_file_path = "";
    
    // Try to read from multiple possible locations
    for path in &config_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                config_content = content;
                config_file_found = true;
                config_file_path = path;
                println!("✅ Configuration loaded from: {}", path);
                break;
            }
            Err(_) => continue,
        }
    }
    
    if !config_file_found {
        return Err(format!(
            "❌ **Configuration File Not Found**\n\n\
            Could not find `lmapiconf.txt` in any of these locations:\n\
            • ./lmapiconf.txt\n\
            • ../lmapiconf.txt\n\
            • ../../lmapiconf.txt\n\
            • src/lmapiconf.txt\n\n\
            **Solution:** Copy `example_lmapiconf.txt` to `lmapiconf.txt` and configure it for your setup."
        ).into());
    }
    
    // Parse configuration
    let mut config_map = HashMap::new();
    for (line_num, line) in config_content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        
        if let Some(equals_pos) = line.find('=') {
            let key = line[..equals_pos].trim().to_string();
            let value = line[equals_pos + 1..].trim().to_string();
            config_map.insert(key, value);
        } else {
            println!("⚠️ Warning: Invalid line {} in {}: {}", line_num + 1, config_file_path, line);
        }
    }
    
    // Validate required keys
    let required_keys = [
        "LM_STUDIO_BASE_URL",
        "LM_STUDIO_TIMEOUT",
        "DEFAULT_MODEL",
        "DEFAULT_REASON_MODEL",
        "DEFAULT_SUMMARIZATION_MODEL",
        "DEFAULT_RANKING_MODEL",
        "DEFAULT_TEMPERATURE",
        "DEFAULT_MAX_TOKENS",
        "MAX_DISCORD_MESSAGE_LENGTH",
        "RESPONSE_FORMAT_PADDING",
        "DEFAULT_VISION_MODEL",
    ];
    
    let mut missing_keys = Vec::new();
    for key in &required_keys {
        if !config_map.contains_key(*key) {
            missing_keys.push(*key);
        }
    }
    
    if !missing_keys.is_empty() {
        return Err(format!(
            "❌ **Missing Configuration Keys**\n\n\
            The following required keys are missing from `{}`:\n\
            {}\n\n\
            **Solution:** Add these keys to your lmapiconf.txt file. See example_lmapiconf.txt for reference.",
            config_file_path,
            missing_keys.iter().map(|k| format!("• {}", k)).collect::<Vec<_>>().join("\n")
        ).into());
    }
    
    // Extract and validate configuration values
    let base_url = config_map.get("LM_STUDIO_BASE_URL")
        .ok_or("LM_STUDIO_BASE_URL not found in lmapiconf.txt")?
        .clone();
    
    // Validate base URL format
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err(format!(
            "❌ **Invalid Base URL**\n\n\
            LM_STUDIO_BASE_URL must start with http:// or https://\n\
            Current value: `{}`\n\n\
            **Examples:**\n\
            • http://localhost:1234 (LM Studio)\n\
            • http://localhost:11434 (Ollama)\n\
            • http://127.0.0.1:1234 (Local IP)",
            base_url
        ).into());
    }
    
    let timeout = config_map.get("LM_STUDIO_TIMEOUT")
        .ok_or("LM_STUDIO_TIMEOUT not found in lmapiconf.txt")?
        .parse::<u64>()
        .map_err(|_| "LM_STUDIO_TIMEOUT must be a valid number (seconds)")?;
    
    if timeout == 0 || timeout > 600 {
        return Err(format!(
            "❌ **Invalid Timeout Value**\n\n\
            LM_STUDIO_TIMEOUT must be between 1 and 600 seconds\n\
            Current value: {} seconds\n\n\
            **Recommended:** 30-120 seconds",
            timeout
        ).into());
    }
    
    let default_model = config_map.get("DEFAULT_MODEL")
        .ok_or("DEFAULT_MODEL not found in lmapiconf.txt")?
        .clone();
        
    if default_model.trim().is_empty() {
        return Err("❌ DEFAULT_MODEL cannot be empty. Specify the model name loaded in LM Studio.".into());
    }
    
    let default_reason_model = config_map.get("DEFAULT_REASON_MODEL")
        .ok_or("DEFAULT_REASON_MODEL not found in lmapiconf.txt")?
        .clone();
        
    let default_summarization_model = config_map.get("DEFAULT_SUMMARIZATION_MODEL")
        .ok_or("DEFAULT_SUMMARIZATION_MODEL not found in lmapiconf.txt")?
        .clone();
        
    let default_ranking_model = config_map.get("DEFAULT_RANKING_MODEL")
        .ok_or("DEFAULT_RANKING_MODEL not found in lmapiconf.txt")?
        .clone();
        
    let default_vision_model = config_map.get("DEFAULT_VISION_MODEL")
        .ok_or("DEFAULT_VISION_MODEL not found in lmapiconf.txt")?
        .clone();
    
    let default_temperature = config_map.get("DEFAULT_TEMPERATURE")
        .ok_or("DEFAULT_TEMPERATURE not found in lmapiconf.txt")?
        .parse::<f32>()
        .map_err(|_| "DEFAULT_TEMPERATURE must be a valid number")?;
    
    if default_temperature < 0.0 || default_temperature > 2.0 {
        return Err(format!(
            "❌ **Invalid Temperature Value**\n\n\
            DEFAULT_TEMPERATURE must be between 0.0 and 2.0\n\
            Current value: {}\n\n\
            **Recommended:** 0.1-1.0 (0.7-0.8 is typical)",
            default_temperature
        ).into());
    }
    
    let default_max_tokens = config_map.get("DEFAULT_MAX_TOKENS")
        .ok_or("DEFAULT_MAX_TOKENS not found in lmapiconf.txt")?
        .parse::<i32>()
        .map_err(|_| "DEFAULT_MAX_TOKENS must be a valid number")?;
    
    if default_max_tokens <= 0 || default_max_tokens > 32768 {
        return Err(format!(
            "❌ **Invalid Max Tokens Value**\n\n\
            DEFAULT_MAX_TOKENS must be between 1 and 32768\n\
            Current value: {}\n\n\
            **Recommended:** 1000-8000 for most use cases",
            default_max_tokens
        ).into());
    }
    
    let max_discord_message_length = config_map.get("MAX_DISCORD_MESSAGE_LENGTH")
        .ok_or("MAX_DISCORD_MESSAGE_LENGTH not found in lmapiconf.txt")?
        .parse::<usize>()
        .map_err(|_| "MAX_DISCORD_MESSAGE_LENGTH must be a valid number")?;
    
    let response_format_padding = config_map.get("RESPONSE_FORMAT_PADDING")
        .ok_or("RESPONSE_FORMAT_PADDING not found in lmapiconf.txt")?
        .parse::<usize>()
        .map_err(|_| "RESPONSE_FORMAT_PADDING must be a valid number")?;
    
    // Optional seed configuration for reproducible responses
    let default_seed = config_map.get("DEFAULT_SEED")
        .filter(|s| !s.trim().is_empty()) // Ignore empty values
        .map(|s| s.parse::<i64>())
        .transpose()
        .map_err(|_| "DEFAULT_SEED must be a valid integer if specified")?;
    
    let config = LMConfig {
        base_url,
        timeout,
        default_model,
        default_reason_model,
        default_summarization_model,
        default_ranking_model,
        default_temperature,
        default_max_tokens,
        max_discord_message_length,
        response_format_padding,
        default_vision_model,
        default_seed,
    };
    
    // Test connectivity after loading configuration
    println!("🔍 Testing API connectivity...");
    if let Err(e) = test_api_connectivity(&config).await {
        return Err(format!(
            "❌ **Connectivity Test Failed**\n\n\
            Configuration loaded successfully from `{}`, but connectivity test failed:\n\n\
            {}\n\n\
            **Config Details:**\n\
            • Base URL: {}\n\
            • Default Model: {}\n\
            • Timeout: {}s",
            config_file_path, e, config.base_url, config.default_model, config.timeout
        ).into());
    }
    
    println!("✅ API connectivity test passed!");
    Ok(config)
}

/// Enhanced chat completion with retry logic and better error handling
pub async fn chat_completion(
    messages: Vec<ChatMessage>,
    model: &str,
    config: &LMConfig,
    max_tokens: Option<i32>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    chat_completion_with_retries(messages, model, config, max_tokens, 3).await
}

/// Chat completion with configurable retry attempts
async fn chat_completion_with_retries(
    messages: Vec<ChatMessage>,
    model: &str,
    config: &LMConfig,
    max_tokens: Option<i32>,
    max_retries: u32,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = get_http_client().await;
    let api_url = format!("{}/v1/chat/completions", config.base_url);
    
    let chat_request = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": config.default_temperature,
        "max_tokens": max_tokens.unwrap_or(config.default_max_tokens),
        "stream": false,
        "seed": config.default_seed
    });

    let mut last_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    
    for attempt in 1..=max_retries {
        println!("[DEBUG][CHAT] Attempt {}/{} - Sending request to: {}", attempt, max_retries, api_url);
        
        let start_time = std::time::Instant::now();
        
        let response = match client
            .post(&api_url)
            .json(&chat_request)
            .timeout(Duration::from_secs(config.timeout))
            .send()
            .await
        {
            Ok(resp) => {
                let elapsed = start_time.elapsed();
                println!("[DEBUG][CHAT] Request completed in {:.2}s - Status: {}", elapsed.as_secs_f32(), resp.status());
                resp
            }
            Err(e) => {
                let elapsed = start_time.elapsed();
                println!("[DEBUG][CHAT] Request failed after {:.2}s: {}", elapsed.as_secs_f32(), e);
                
                let error_msg = format!("{}", e);
                
                // Check for specific error types that might benefit from retry
                let should_retry = attempt < max_retries && (
                    error_msg.contains("timeout") ||
                    error_msg.contains("connection reset") ||
                    error_msg.contains("connection aborted") ||
                    error_msg.contains("broken pipe") ||
                    error_msg.contains("connection closed")
                );
                
                                 if should_retry {
                     let delay = Duration::from_millis(1000 * attempt as u64); // Exponential backoff
                     println!("[DEBUG][CHAT] Retrying in {:.1}s...", delay.as_secs_f32());
                     tokio::time::sleep(delay).await;
                     last_error = Some(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("{}", e))));
                    continue;
                } else {
                    // Don't retry for these errors - they're likely configuration issues
                    if error_msg.contains("os error 10013") || error_msg.contains("access permissions") {
                        return Err(format!(
                            "🚫 **Windows Network Permission Error**\n\n\
                            Cannot connect to LM Studio API\n\n\
                            **Quick Fixes:**\n\
                            • **Run as Administrator**: Right-click and 'Run as administrator'\n\
                            • **Windows Firewall**: Add firewall exception for this program\n\
                            • **Try localhost**: Use `http://127.0.0.1:1234` in lmapiconf.txt\n\n\
                            **Current URL:** {}\n\
                            **Error:** {}", 
                            config.base_url, e
                        ).into());
                    } else if error_msg.contains("refused") || error_msg.contains("connection refused") {
                        return Err(format!(
                            "🚫 **Connection Refused**\n\n\
                            LM Studio is not accepting connections\n\n\
                            **Solutions:**\n\
                            • **Start LM Studio**: Make sure LM Studio is running\n\
                            • **Load Model**: Ensure a model is loaded\n\
                            • **Enable Server**: Click 'Start Server' in LM Studio\n\
                            • **Check Port**: Verify port 1234 is available\n\n\
                            **Current URL:** {}\n\
                            **Error:** {}", 
                            config.base_url, e
                        ).into());
                    } else {
                        return Err(format!("API request failed: {}", e).into());
                    }
                }
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
            
            // Check if this is a retryable server error
            let is_server_error = status.is_server_error();
            let should_retry = attempt < max_retries && is_server_error;
            
            if should_retry {
                                 let delay = Duration::from_millis(1000 * attempt as u64);
                 println!("[DEBUG][CHAT] Server error ({}), retrying in {:.1}s...", status, delay.as_secs_f32());
                 tokio::time::sleep(delay).await;
                 last_error = Some(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("HTTP {} - {}", status, error_text))));
                continue;
            } else {
                return Err(format!(
                    "🚫 **API Error (HTTP {})**\n\n\
                    **Response:** {}\n\n\
                    **Solutions:**\n\
                    • **Model Loaded**: Ensure model '{}' is loaded in LM Studio\n\
                    • **Model Name**: Verify model name matches exactly\n\
                    • **Server Status**: Check LM Studio server logs\n\
                    • **Memory**: Ensure sufficient RAM for the model\n\n\
                    **API URL:** {}", 
                    status, error_text, model, api_url
                ).into());
            }
        }

        // Parse successful response
        let response_text = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                                 if attempt < max_retries {
                     let delay = Duration::from_millis(1000 * attempt as u64);
                     println!("[DEBUG][CHAT] Failed to read response, retrying in {:.1}s...", delay.as_secs_f32());
                     tokio::time::sleep(delay).await;
                     last_error = Some(Box::new(e));
                    continue;
                } else {
                    return Err(format!("Failed to read response: {}", e).into());
                }
            }
        };
        
        let response_json: serde_json::Value = match serde_json::from_str(&response_text) {
            Ok(json) => json,
            Err(e) => {
                return Err(format!(
                    "🚫 **Invalid API Response**\n\n\
                    Failed to parse JSON response from LM Studio\n\n\
                    **Response:** {}\n\
                    **Parse Error:** {}\n\n\
                    **Solutions:**\n\
                    • **Update LM Studio**: Ensure you're using a recent version\n\
                    • **Check Model**: Verify the model supports chat completions\n\
                    • **Server Logs**: Check LM Studio logs for errors",
                    response_text.chars().take(500).collect::<String>(), e
                ).into());
            }
        };
        
        // Extract content from response
        if let Some(choices) = response_json["choices"].as_array() {
            if let Some(first_choice) = choices.get(0) {
                if let Some(message) = first_choice["message"].as_object() {
                    if let Some(content) = message["content"].as_str() {
                        let result = content.trim().to_string();
                        println!("[DEBUG][CHAT] Success! Generated {} characters", result.len());
                        return Ok(result);
                    }
                }
            }
        }
        
        // If we reach here, the JSON structure was unexpected
        return Err(format!(
            "🚫 **Unexpected API Response Format**\n\n\
            LM Studio returned a valid JSON response, but the structure was unexpected\n\n\
            **Response:** {}\n\n\
            **Solutions:**\n\
            • **Update LM Studio**: Ensure compatibility with OpenAI API format\n\
            • **Check Model**: Verify the model supports chat completions\n\
            • **API Version**: Ensure you're using a compatible API version",
            serde_json::to_string_pretty(&response_json).unwrap_or_else(|_| "Unable to format response".to_string())
        ).into());
    }
    
    // All retries exhausted
    Err(format!("Request failed after {} attempts. Last error: {}", 
                max_retries, 
                last_error.map(|e| format!("{}", e)).unwrap_or_else(|| "Unknown error".to_string())
    ).into())
}



/// Placeholder for multi-search functionality
pub async fn multi_search(_query: &str) -> Result<Vec<SearchResult>, Box<dyn std::error::Error + Send + Sync>> {
    // Placeholder - return empty results
    Ok(Vec::new())
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