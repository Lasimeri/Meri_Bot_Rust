// sum.rs - Self-Contained Webpage and YouTube Summarization Command Module
// This module implements the ^sum command, providing AI-powered summarization for webpages and YouTube videos.
// It supports robust content fetching, VTT/HTML cleaning, RAG chunking, and real-time streaming to Discord.
//
// Key Features:
// - Summarizes arbitrary webpages and YouTube videos
// - Uses yt-dlp for YouTube transcript extraction
// - Cleans and processes VTT/HTML content
// - RAG (map-reduce) chunking for long content
// - Real-time streaming of summary to Discord
// - Multi-path config and prompt loading
// - Robust error handling and logging
// - Self-contained with no external module dependencies
//
// Self-contained includes:
// - LM Studio configuration loading and management
// - HTTP client setup with connection pooling
// - Chat completion functionality with retry logic
// - All necessary structures and functions from search.rs and reason.rs

use serenity::{
    client::Context,
    framework::standard::{macros::command, macros::group, Args, CommandResult},
    model::channel::Message,
};
use std::time::Duration;
use std::fs;
use std::process::Command;
use uuid::Uuid;
use log::{info, warn, error, debug, trace};
use serde::{Deserialize, Serialize};
use regex::Regex;
use std::time::Instant;
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use tokio::sync::OnceCell;

// ============================================================================
// SELF-CONTAINED COMPONENTS FROM SEARCH.RS AND REASON.RS
// ============================================================================

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
        .timeout(Duration::from_secs(60)) // 1 minute for API endpoint test
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
    // Trace-level function entry
    trace!("[TRACE][SUM][load_lm_config] === FUNCTION ENTRY ===");
    trace!("[TRACE][SUM][load_lm_config] Function: load_lm_config()");
    trace!("[TRACE][SUM][load_lm_config] Current working dir: {:?}", std::env::current_dir());
    
    let config_paths = [
        "lmapiconf.txt",
        "../lmapiconf.txt", 
        "../../lmapiconf.txt",
        "src/lmapiconf.txt"
    ];
    
    trace!("[TRACE][SUM][load_lm_config] Config paths to try: {:?}", config_paths);
    
    let mut config_content = String::new();
    let mut config_file_found = false;
    let mut config_file_path = "";
    
    // Try to read from multiple possible locations
    trace!("[TRACE][SUM][load_lm_config] === FILE SEARCH LOOP ===");
    for (index, path) in config_paths.iter().enumerate() {
        trace!("[TRACE][SUM][load_lm_config] Attempt {}: trying path '{}'", index + 1, path);
        trace!("[TRACE][SUM][load_lm_config] Path exists: {}", std::path::Path::new(path).exists());
        
        match fs::read_to_string(path) {
            Ok(content) => {
                trace!("[TRACE][SUM][load_lm_config] SUCCESS: File read from '{}'", path);
                trace!("[TRACE][SUM][load_lm_config] Content length: {} bytes", content.len());
                trace!("[TRACE][SUM][load_lm_config] Content preview: {}", &content[..std::cmp::min(200, content.len())]);
                
                config_content = content;
                config_file_found = true;
                config_file_path = path;
                println!("✅ Configuration loaded from: {}", path);
                break;
            }
            Err(e) => {
                trace!("[TRACE][SUM][load_lm_config] FAILED: Could not read '{}': {}", path, e);
                continue;
            }
        }
    }
    
    trace!("[TRACE][SUM][load_lm_config] File search complete. Found: {}", config_file_found);
    
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
    // Trace-level function entry for chat completion
    trace!("[TRACE][SUM][chat_completion] === FUNCTION ENTRY ===");
    trace!("[TRACE][SUM][chat_completion] Model: '{}'", model);
    trace!("[TRACE][SUM][chat_completion] Messages count: {}", messages.len());
    trace!("[TRACE][SUM][chat_completion] Max tokens: {:?}", max_tokens);
    trace!("[TRACE][SUM][chat_completion] Config base URL: {}", config.base_url);
    trace!("[TRACE][SUM][chat_completion] Config temperature: {}", config.default_temperature);
    
    let result = chat_completion_with_retries(messages, model, config, max_tokens, 3).await;
    
    // Trace-level function exit
    match &result {
        Ok(response) => {
            trace!("[TRACE][SUM][chat_completion] === FUNCTION EXIT (SUCCESS) ===");
            trace!("[TRACE][SUM][chat_completion] Response length: {} chars", response.len());
        }
        Err(e) => {
            trace!("[TRACE][SUM][chat_completion] === FUNCTION EXIT (ERROR) ===");
            trace!("[TRACE][SUM][chat_completion] Error: {}", e);
        }
    }
    
    result
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

// ============================================================================
// ORIGINAL SUM.RS FUNCTIONALITY
// ============================================================================

// SSE response structures for streaming summary
// Used to parse streaming JSON chunks from the AI API
#[derive(Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>, // Streaming choices (delta content)
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,         // Streaming delta (content chunk)
    finish_reason: Option<String>, // Reason for stream completion
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,    // Content chunk
}

#[command]
#[aliases("summarize", "webpage")]
/// Main ^sum command handler
/// Handles summarization of webpages and YouTube videos
/// Supports:
///   - ^sum <url> (webpage or YouTube)
pub async fn sum(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let start_time = std::time::Instant::now();
    let command_uuid = Uuid::new_v4();
    
    // Trace-level function entry logging
    trace!("[TRACE][SUM] === FUNCTION ENTRY: sum() ===");
    trace!("[TRACE][SUM] Function: sum(), UUID: {}", command_uuid);
    trace!("[TRACE][SUM] Entry timestamp: {:?}", start_time);
    trace!("[TRACE][SUM] Context data lock acquired successfully");
    trace!("[TRACE][SUM] Message author: {} (ID: {})", msg.author.name, msg.author.id);
    trace!("[TRACE][SUM] Channel: {} (ID: {})", msg.channel_id, msg.channel_id.0);
    trace!("[TRACE][SUM] Guild: {:?}", msg.guild_id);
    trace!("[TRACE][SUM] Raw arguments: '{}'", args.message());
    
    info!("📺 === SUM COMMAND STARTED ===");
    info!("🆔 Command UUID: {}", command_uuid);
    info!("👤 User: {} ({})", msg.author.name, msg.author.id);
    info!("📺 Channel: {} ({})", msg.channel_id, msg.channel_id.0);
    info!("📺 Guild: {:?}", msg.guild_id);
    info!("📺 Message ID: {}", msg.id);
    info!("📺 Timestamp: {:?}", msg.timestamp);
    
    // Enhanced logging for web page summarization debugging
    log::info!("🔍 === SUM COMMAND DEBUG INFO ===");
    log::info!("🔍 Command UUID: {}", command_uuid);
    log::info!("🔍 User ID: {}", msg.author.id);
    log::info!("🔍 Channel ID: {}", msg.channel_id);
    log::info!("🔍 Message ID: {}", msg.id);
    log::info!("🔍 Arguments: '{}'", args.message());
    log::info!("🔍 Arguments length: {} characters", args.message().len());
    log::info!("🔍 Start time: {:?}", start_time);
    
    debug!("🔧 === COMMAND INITIALIZATION ===");
    debug!("🔧 Command arguments: '{}'", args.message());
    debug!("🔧 Arguments length: {} characters", args.message().len());
    debug!("🔧 Arguments trimmed: '{}'", args.message().trim());
    debug!("🔧 Arguments trimmed length: {} characters", args.message().trim().len());
    trace!("🔍 Command initialization details: uuid={}, author_id={}, channel_id={}, message_id={}", 
           command_uuid, msg.author.id, msg.channel_id, msg.id);
    
    let url = args.message().trim();
    
    // Trace-level URL processing
    trace!("[TRACE][SUM] === URL PROCESSING ENTRY ===");
    trace!("[TRACE][SUM] Raw args message: '{}'", args.message());
    trace!("[TRACE][SUM] Raw args length: {} chars", args.message().len());
    trace!("[TRACE][SUM] After trim: '{}'", url);
    trace!("[TRACE][SUM] After trim length: {} chars", url.len());
    trace!("[TRACE][SUM] Is empty after trim: {}", url.is_empty());
    trace!("[TRACE][SUM] Contains http://: {}", url.contains("http://"));
    trace!("[TRACE][SUM] Contains https://: {}", url.contains("https://"));
    trace!("[TRACE][SUM] First 50 chars: '{}'", &url[..std::cmp::min(50, url.len())]);
    
    debug!("🔗 === URL PROCESSING ===");
    debug!("🔗 Raw URL: '{}'", url);
    debug!("🔗 URL length: {} characters", url.len());
    debug!("🔗 URL is empty: {}", url.is_empty());
    trace!("🔍 URL processing: raw_length={}, trimmed_length={}, is_empty={}", 
           args.message().len(), url.len(), url.is_empty());
    
    // Logging is now configured globally in main.rs to show all levels
    debug!("🔧 Logging configured for maximum debugging detail");
    trace!("🔍 TRACE logging enabled - will show all function calls and data flow");
    
    if url.is_empty() {
        warn!("❌ === EMPTY URL ERROR ===");
        warn!("❌ Empty URL provided by user {} ({})", msg.author.name, msg.author.id);
        debug!("🔍 URL validation failed: empty string");
        debug!("🔍 Sending error message to user");
        trace!("🔍 Empty URL error: user_id={}, channel_id={}, command_uuid={}", 
               msg.author.id, msg.channel_id, command_uuid);
        
        // Trace-level error exit
        trace!("[TRACE][SUM] === FUNCTION EXIT: sum() (EMPTY URL ERROR) ===");
        trace!("[TRACE][SUM] Function: sum(), UUID: {}", command_uuid);
        trace!("[TRACE][SUM] Exit status: ERROR - Empty URL");
        trace!("[TRACE][SUM] Exit timestamp: {:?}", std::time::Instant::now());
        
        msg.reply(ctx, "Please provide a URL to summarize!\n\n**Usage:** `^sum <url>`").await?;
        debug!("✅ Error message sent successfully");
        return Ok(());
    }
    
    debug!("🔍 === URL VALIDATION ===");
    debug!("🔍 Validating URL format: {}", url);
    debug!("🔍 URL starts with http://: {}", url.starts_with("http://"));
    debug!("🔍 URL starts with https://: {}", url.starts_with("https://"));
    debug!("🔍 URL contains youtube.com: {}", url.contains("youtube.com"));
    debug!("🔍 URL contains youtu.be: {}", url.contains("youtu.be"));
    trace!("🔍 URL validation details: starts_with_http={}, starts_with_https={}, contains_youtube_com={}, contains_youtu_be={}", 
           url.starts_with("http://"), url.starts_with("https://"), url.contains("youtube.com"), url.contains("youtu.be"));
    
    if !url.starts_with("http://") && !url.starts_with("https://") {
        warn!("❌ === INVALID URL FORMAT ERROR ===");
        warn!("❌ Invalid URL format provided: {}", url);
        debug!("🔍 URL validation failed: missing http/https prefix");
        debug!("🔍 URL first 10 characters: '{}'", url.chars().take(10).collect::<String>());
        trace!("🔍 URL validation failure details: length={}, first_chars={}, command_uuid={}", 
               url.len(), url.chars().take(10).collect::<String>(), command_uuid);
        
        // Trace-level error exit
        trace!("[TRACE][SUM] === FUNCTION EXIT: sum() (INVALID URL FORMAT ERROR) ===");
        trace!("[TRACE][SUM] Function: sum(), UUID: {}", command_uuid);
        trace!("[TRACE][SUM] Exit status: ERROR - Invalid URL format");
        trace!("[TRACE][SUM] Invalid URL: '{}'", url);
        trace!("[TRACE][SUM] Exit timestamp: {:?}", std::time::Instant::now());
        
        msg.reply(ctx, "Please provide a valid URL starting with `http://` or `https://`").await?;
        debug!("✅ Invalid URL error message sent");
        return Ok(());
    }
    debug!("✅ URL format validation passed");
    trace!("🔍 URL validation success: protocol={}, command_uuid={}", 
           if url.starts_with("https://") { "https" } else { "http" }, command_uuid);
    
    // Load LM configuration from lmapiconf.txt
    trace!("[TRACE][SUM] === CONFIGURATION LOADING ENTRY ===");
    trace!("[TRACE][SUM] About to call load_lm_config()");
    trace!("[TRACE][SUM] Current working directory: {:?}", std::env::current_dir());
    trace!("[TRACE][SUM] Command UUID: {}", command_uuid);
    
    debug!("🔧 === CONFIGURATION LOADING ===");
    debug!("🔧 Loading LM configuration from lmapiconf.txt...");
    trace!("🔍 Configuration loading phase started: command_uuid={}", command_uuid);
    
    let config = match load_lm_config().await {
        Ok(cfg) => {
            info!("✅ === CONFIGURATION LOADED SUCCESSFULLY ===");
            info!("✅ LM configuration loaded successfully");
            debug!("🧠 Using default model: {}", cfg.default_model);
            debug!("🧠 Using reasoning model: {}", cfg.default_reason_model);
            debug!("🧠 Using summarization model: {}", cfg.default_summarization_model);
            debug!("🌐 API endpoint: {}", cfg.base_url);
            debug!("⏱️ Timeout setting: {} seconds", cfg.timeout);
            debug!("🔥 Temperature setting: {}", cfg.default_temperature);
            debug!("📝 Max tokens setting: {}", cfg.default_max_tokens);
            debug!("📏 Max Discord message length: {}", cfg.max_discord_message_length);
            debug!("📏 Response format padding: {}", cfg.response_format_padding);
            trace!("🔍 Configuration details: max_discord_length={}, response_format_padding={}, command_uuid={}", 
                   cfg.max_discord_message_length, cfg.response_format_padding, command_uuid);
            cfg
        },
        Err(e) => {
            error!("❌ === CONFIGURATION LOADING ERROR ===");
            error!("❌ Failed to load LM configuration: {}", e);
            debug!("🔍 Configuration loading error details: {:?}", e);
            debug!("🔍 Configuration error type: {:?}", std::any::type_name_of_val(&e));
            trace!("🔍 Configuration error: error_type={}, command_uuid={}", 
                   std::any::type_name_of_val(&e), command_uuid);
            msg.reply(ctx, &format!("❌ **Configuration Error**\n\n{}\n\n**Setup required:** Ensure `lmapiconf.txt` is properly configured with your LM Studio settings.", e)).await?;
            debug!("✅ Configuration error message sent");
            return Ok(());
        }
    };
    
    debug!("🔧 Configuration loaded successfully, proceeding with next steps");
    trace!("🔍 Configuration phase completed: command_uuid={}", command_uuid);
    

    
    // Trace-level URL type detection
    trace!("[TRACE][SUM] === URL TYPE DETECTION ENTRY ===");
    trace!("[TRACE][SUM] URL to analyze: '{}'", url);
    trace!("[TRACE][SUM] URL length: {} chars", url.len());
    trace!("[TRACE][SUM] Checking youtube.com/...");
    let contains_youtube_com = url.contains("youtube.com/");
    trace!("[TRACE][SUM] Contains youtube.com/: {}", contains_youtube_com);
    trace!("[TRACE][SUM] Checking youtu.be/...");
    let contains_youtu_be = url.contains("youtu.be/");
    trace!("[TRACE][SUM] Contains youtu.be/: {}", contains_youtu_be);
    let is_youtube = contains_youtube_com || contains_youtu_be;
    trace!("[TRACE][SUM] Final determination - is_youtube: {}", is_youtube);
    trace!("[TRACE][SUM] Content type will be: {}", if is_youtube { "YouTube video" } else { "Webpage" });
    
    debug!("🔍 === URL TYPE DETECTION ===");
    debug!("🔍 Detecting URL type...");
    debug!("🔍 URL contains youtube.com/: {}", url.contains("youtube.com/"));
    debug!("🔍 URL contains youtu.be/: {}", url.contains("youtu.be/"));
    debug!("🔍 Final YouTube detection: {}", is_youtube);
    trace!("🔍 URL type detection details: contains_youtube_com={}, contains_youtu_be={}, is_youtube={}, command_uuid={}", 
           url.contains("youtube.com/"), url.contains("youtu.be/"), is_youtube, command_uuid);
    info!("🎯 === CONTENT TYPE DETECTED ===");
    info!("🎯 Processing {} URL: {}", if is_youtube { "YouTube" } else { "webpage" }, url);
    debug!("📊 URL type detection: YouTube = {}", is_youtube);
    
    // Always use the summarization model for all content types due to 32K context window
    let selected_model = &config.default_summarization_model;
    
    // Log the model selection
    info!("🎯 === SUMMARIZATION MODEL SELECTION ===");
    info!("🎯 Using summarization model for all content types: {}", selected_model);
    info!("🎯 Reason: 32K context window for optimal summarization performance");
    debug!("🎯 Model selection: summarization_model={}, content_type={}", selected_model, if is_youtube { "YouTube" } else { "webpage" });
    trace!("🔍 Model selection: model={}, content_type={}, command_uuid={}", selected_model, if is_youtube { "youtube" } else { "webpage" }, command_uuid);
    
    // Create response message
    debug!("💬 === DISCORD MESSAGE CREATION ===");
    debug!("💬 Creating initial Discord response message...");
    trace!("🔍 Discord message creation: author={}, channel={}, command_uuid={}", msg.author.name, msg.channel_id, command_uuid);
    let mut response_msg = msg.reply(ctx, "🔄 Fetching content...").await?;
    debug!("✅ Initial Discord message sent successfully");
    debug!("📝 Response message ID: {}", response_msg.id);
    debug!("📝 Response message channel ID: {}", response_msg.channel_id);
    debug!("📝 Response message content: '{}'", response_msg.content);
    trace!("🔍 Discord message details: id={}, channel_id={}, content_length={}, command_uuid={}", 
           response_msg.id, response_msg.channel_id, response_msg.content.len(), command_uuid);
    
    // Add a small delay to avoid rate limiting if multiple requests are made quickly
    debug!("⏳ === RATE LIMITING DELAY ===");
    debug!("⏳ Adding 1-second delay to prevent rate limiting...");
    trace!("🔍 Rate limiting delay: 1000ms, command_uuid={}", command_uuid);
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    debug!("✅ Delay completed");
    trace!("🔍 Rate limiting delay completed: command_uuid={}", command_uuid);
    
    // Fetch content
    info!("🌐 === CONTENT FETCHING PHASE ===");
    info!("🌐 Starting content fetching process...");
    debug!("🚀 Content fetching phase initiated");
    trace!("🔍 Content fetching phase: url_type={}, url={}, command_uuid={}", 
           if is_youtube { "youtube" } else { "webpage" }, url, command_uuid);

    let (subtitle_file_path, content) = if is_youtube {
        debug!("🎥 === YOUTUBE CONTENT FETCHING ===");
        debug!("🎥 YouTube URL detected, starting transcript extraction...");
        trace!("🔍 YouTube transcript extraction started: command_uuid={}", command_uuid);
        match fetch_youtube_transcript(url).await {
            Ok(path) => {
                info!("✅ === YOUTUBE TRANSCRIPT SUCCESS ===");
                info!("✅ YouTube subtitle file created successfully: {}", path);
                debug!("📁 Subtitle file path: {}", path);
                debug!("📁 Subtitle file exists: {}", std::path::Path::new(&path).exists());
                trace!("🔍 YouTube subtitle file success: path={}, command_uuid={}", path, command_uuid);
                
                // Read the subtitle file content for statistics only (RAG will handle the actual processing)
                debug!("📖 === SUBTITLE FILE READING FOR STATISTICS ===");
                debug!("📖 Reading subtitle file for statistics only...");
                match fs::read_to_string(&path) {
                    Ok(file_content) => {
                        debug!("📖 Subtitle file read successfully: {} characters", file_content.len());
                        debug!("📖 File content preview: {}", &file_content[..std::cmp::min(200, file_content.len())]);
                        trace!("🔍 Subtitle file read: path={}, length={}, command_uuid={}", path, file_content.len(), command_uuid);
                        
                        let cleaned_content = clean_vtt_content(&file_content);
                        debug!("🧹 === VTT CLEANING FOR STATISTICS ===");
                        debug!("🧹 Cleaning VTT content for statistics only...");
                        debug!("📝 Original subtitle content: {} characters", file_content.len());
                        debug!("📝 Cleaned subtitle content: {} characters", cleaned_content.len());
                        debug!("📝 Content preview: {}", &cleaned_content[..std::cmp::min(200, cleaned_content.len())]);
                        debug!("📊 Subtitle statistics: {} characters, {} words", cleaned_content.len(), cleaned_content.split_whitespace().count());
                        debug!("📁 RAG will process the original file: {}", path);
                        trace!("🔍 VTT cleaning for statistics: original_length={}, cleaned_length={}, word_count={}, command_uuid={}", 
                               file_content.len(), cleaned_content.len(), cleaned_content.split_whitespace().count(), command_uuid);
                        
                        // Store statistics for logging purposes only
                        debug!("📊 YouTube subtitle statistics: {} characters, {} words", 
                               cleaned_content.len(), cleaned_content.split_whitespace().count());
                    },
                    Err(e) => {
                        warn!("⚠️ === SUBTITLE FILE READ ERROR ===");
                        warn!("⚠️ Could not read subtitle file for statistics: {}", e);
                        debug!("🔍 Subtitle file read error: path={}, error={}", path, e);
                        trace!("🔍 Subtitle file read error: path={}, error_type={}, command_uuid={}", 
                               path, std::any::type_name_of_val(&e), command_uuid);
                    }
                }
                (Some(path), String::new()) // Empty content for YouTube since RAG handles the file
            },
            Err(e) => {
                error!("❌ === YOUTUBE TRANSCRIPT ERROR ===");
                error!("❌ Failed to fetch YouTube transcript: {}", e);
                debug!("🔍 YouTube transcript error details: {:?}", e);
                debug!("🔍 YouTube transcript error type: {:?}", std::any::type_name_of_val(&e));
                trace!("🔍 YouTube transcript error: error_type={}, command_uuid={}", 
                       std::any::type_name_of_val(&e), command_uuid);
                response_msg.edit(ctx, |m| {
                    m.content(format!("❌ Failed to fetch YouTube transcript: {}", e))
                }).await?;
                debug!("✅ YouTube transcript error message sent to Discord");
                return Ok(());
            }
        }
    } else {
        // Content will be assigned from webpage processing
        debug!("🌐 === WEBPAGE CONTENT FETCHING ===");
        debug!("🌐 Webpage URL detected, starting content extraction...");
        trace!("🔍 Webpage content extraction started: command_uuid={}", command_uuid);
        
        // Enhanced logging for web page processing
        log::info!("🌐 === WEBPAGE PROCESSING STARTED ===");
        log::info!("🌐 URL: {}", url);
        log::info!("🌐 Command UUID: {}", command_uuid);
        log::info!("🌐 Processing type: HTML file download and RAG processing");
        
        match fetch_webpage_content(url).await {
            Ok((page_content, html_file_path)) => {
                info!("✅ === WEBPAGE CONTENT SUCCESS ===");
                info!("✅ Webpage content fetched successfully: {} characters", page_content.len());
                info!("💾 HTML file saved for RAG processing: {}", html_file_path);
                debug!("📄 Content preview: {}", &page_content[..std::cmp::min(200, page_content.len())]);
                debug!("📊 Webpage statistics: {} characters, {} words", page_content.len(), page_content.split_whitespace().count());
                debug!("💾 HTML file path: {}", html_file_path);
                trace!("🔍 Webpage content success: length={}, word_count={}, preview_chars={}, file_path={}, command_uuid={}", 
                       page_content.len(), page_content.split_whitespace().count(), std::cmp::min(200, page_content.len()), html_file_path, command_uuid);
                
                // Enhanced logging for successful web page processing
                log::info!("✅ === WEBPAGE CONTENT SUCCESS DETAILS ===");
                log::info!("✅ Content length: {} characters", page_content.len());
                log::info!("✅ Word count: {} words", page_content.split_whitespace().count());
                log::info!("✅ HTML file path: {}", html_file_path);
                log::info!("✅ File exists: {}", std::path::Path::new(&html_file_path).exists());
                log::info!("✅ Content preview: {}", &page_content[..std::cmp::min(300, page_content.len())]);
                log::info!("✅ Processing will use RAG with file: {}", html_file_path);
                
                (Some(html_file_path), page_content)
            },
            Err(e) => {
                error!("❌ === WEBPAGE CONTENT ERROR ===");
                error!("❌ Failed to fetch webpage content: {}", e);
                debug!("🔍 Webpage content error details: {:?}", e);
                debug!("🔍 Webpage content error type: {:?}", std::any::type_name_of_val(&e));
                trace!("🔍 Webpage content error: error_type={}, command_uuid={}", 
                       std::any::type_name_of_val(&e), command_uuid);
                response_msg.edit(ctx, |m| {
                    m.content(format!("❌ Failed to fetch webpage: {}", e))
                }).await?;
                debug!("✅ Webpage content error message sent to Discord");
                return Ok(());
            }
        }
    };
    
    // Update status
    debug!("📝 === DISCORD MESSAGE UPDATE ===");
    debug!("📝 Updating Discord message to show AI processing...");
    trace!("🔍 Discord message update: changing content to '🤖 Generating summary...', command_uuid={}", command_uuid);
    response_msg.edit(ctx, |m| {
        m.content("🤖 Generating summary...")
    }).await?;
    debug!("✅ Discord message updated to show AI processing");
    trace!("🔍 Discord message update completed: command_uuid={}", command_uuid);
    
    // Stream the summary
    info!("🧠 === AI SUMMARIZATION PHASE ===");
    info!("🧠 Starting AI summarization process with streaming...");
    debug!("🚀 AI summarization phase initiated");
    
    let content_length = if is_youtube {
        // For YouTube videos, calculate length from subtitle file
        if let Some(ref path) = subtitle_file_path {
            debug!("📏 === CONTENT LENGTH CALCULATION ===");
            debug!("📏 Calculating content length from subtitle file...");
            match fs::read_to_string(path) {
                Ok(content) => {
                    let cleaned_length = clean_vtt_content(&content).len();
                    debug!("📏 Content length from subtitle file: {} characters", cleaned_length);
                    trace!("🔍 Content length calculation: path={}, length={}, command_uuid={}", path, cleaned_length, command_uuid);
                    cleaned_length
                },
                Err(e) => {
                    warn!("⚠️ Could not read subtitle file for length calculation: {}", e);
                    debug!("🔍 Content length calculation error: path={}, error={}", path, e);
                    trace!("🔍 Content length calculation error: path={}, error_type={}, command_uuid={}", 
                           path, std::any::type_name_of_val(&e), command_uuid);
                    0
                }
            }
        } else {
            0
        }
    } else {
        // For webpages, calculate length from content
        debug!("📏 Content length from direct content: {} characters", content.len());
        trace!("🔍 Content length calculation: direct_length={}, command_uuid={}", content.len(), command_uuid);
        content.len()
    };
    
    trace!("🔍 AI summarization phase: content_length={}, url={}, is_youtube={}, command_uuid={}", 
           content_length, url, is_youtube, command_uuid);
    let processing_start = std::time::Instant::now();
    debug!("⏱️ AI processing start time: {:?}", processing_start);
    
    // For YouTube videos, pass empty content since RAG will handle the file processing
    // For webpages, pass the content directly
    let content_for_summary = if is_youtube { 
        debug!("🔧 YouTube video detected - passing empty content for RAG processing");
        debug!("🔧 File path for RAG: {:?}", subtitle_file_path);
        debug!("🔧 File path exists: {}", subtitle_file_path.as_ref().map(|p| std::path::Path::new(p).exists()).unwrap_or(false));
        
        // Verify the subtitle file exists before proceeding
        if let Some(ref path) = subtitle_file_path {
            if !std::path::Path::new(path).exists() {
                error!("❌ === SUBTITLE FILE MISSING ERROR ===");
                error!("❌ Subtitle file does not exist: {}", path);
                response_msg.edit(ctx, |m| {
                    m.content(format!("❌ Subtitle file missing: {}", path))
                }).await?;
                return Ok(()); // Exit early if subtitle file is missing
            }
        } else {
            error!("❌ === NO SUBTITLE FILE PATH ERROR ===");
            error!("❌ No subtitle file path provided for YouTube video");
            response_msg.edit(ctx, |m| {
                m.content("❌ No subtitle file path provided for YouTube video")
            }).await?;
            return Ok(()); // Exit early if no subtitle file path
        }
        
        "" 
    } else { 
        debug!("🔧 Webpage detected - passing content directly");
        &content 
    };
    match stream_summary(content_for_summary, url, &config, selected_model, &mut response_msg, ctx, is_youtube, subtitle_file_path.as_deref()).await {
        Ok(_) => {
            let processing_time = processing_start.elapsed();
            info!("✅ === AI SUMMARIZATION SUCCESS ===");
            info!("✅ Summary streaming completed successfully in {:.2}s", processing_time.as_secs_f64());
            debug!("📊 AI processing statistics: {:.2}s processing time", processing_time.as_secs_f64());
            debug!("📊 Processing time in milliseconds: {} ms", processing_time.as_millis());
            trace!("🔍 AI summarization success: processing_time_ms={}, content_length={}, command_uuid={}", 
                   processing_time.as_millis(), content_length, command_uuid);
        },
        Err(e) => {
            error!("❌ === AI SUMMARIZATION ERROR ===");
            error!("❌ Summary generation failed: {}", e);
            debug!("🔍 AI summarization error details: {:?}", e);
            debug!("🔍 AI summarization error type: {:?}", std::any::type_name_of_val(&e));
            trace!("🔍 AI summarization error: error_type={}, command_uuid={}", 
                   std::any::type_name_of_val(&e), command_uuid);
            response_msg.edit(ctx, |m| {
                m.content(format!("❌ Failed to generate summary: {}", e))
            }).await?;
            debug!("✅ AI summarization error message sent to Discord");
        }
    }
    
    let total_time = start_time.elapsed();
    info!("⏱️ === COMMAND COMPLETION ===");
    info!("⏱️ Sum command completed in {:.2}s for user {} ({})", 
          total_time.as_secs_f64(), msg.author.name, msg.author.id);
    debug!("📊 === FINAL COMMAND STATISTICS ===");
    debug!("📊 Total execution time: {:.2}s", total_time.as_secs_f64());
    debug!("📊 Total execution time in milliseconds: {} ms", total_time.as_millis());
    debug!("📊 Content length: {} characters", content_length);
    debug!("📊 URL type: {}", if is_youtube { "YouTube" } else { "Webpage" });
    debug!("📊 User: {} ({})", msg.author.name, msg.author.id);
    debug!("📊 Channel: {} ({})", msg.channel_id, msg.channel_id.0);
    debug!("📊 Command UUID: {}", command_uuid);
    trace!("🔍 Final command trace: total_time_ms={}, content_length={}, url_type={}, user_id={}, channel_id={}, command_uuid={}", 
           total_time.as_millis(), content_length, if is_youtube { "youtube" } else { "webpage" }, 
           msg.author.id, msg.channel_id, command_uuid);
    
    // Final comprehensive logging summary
    log::info!("🎯 === SUM COMMAND COMPLETION SUMMARY ===");
    log::info!("🎯 Command UUID: {}", command_uuid);
    log::info!("🎯 Total execution time: {:.2}s", total_time.as_secs_f64());
    log::info!("🎯 Content type: {}", if is_youtube { "YouTube" } else { "Webpage" });
    log::info!("🎯 Content length: {} characters", content_length);
    log::info!("🎯 User: {} ({})", msg.author.name, msg.author.id);
    log::info!("🎯 Channel: {} ({})", msg.channel_id, msg.channel_id.0);
    log::info!("🎯 URL: {}", url);
    log::info!("🎯 Processing method: {}", if subtitle_file_path.is_some() { "RAG with file" } else { "Direct processing" });
    
    // TEMPORARILY BYPASS CLEANUP - Keep temporary files for debugging
    if let Some(ref file_path) = subtitle_file_path {
        // Log file path
        log::info!("🎯 File path used: {}", file_path);
        log::info!("🔄 === CLEANUP BYPASSED ===");
        log::info!("🔄 Temporary file preserved for debugging: {}", file_path);
        log::info!("🔄 File exists: {}", std::path::Path::new(&file_path).exists());
        log::info!("🔄 Command UUID: {}", command_uuid);
        log::info!("🔄 Status: Temporary file preserved (cleanup bypassed)");
        
        debug!("🔄 === CLEANUP BYPASSED ===");
        debug!("🔄 Preserving temporary file for debugging: {}", file_path);
        trace!("🔍 Cleanup bypassed: path={}, command_uuid={}", file_path, command_uuid);
    }
    
    log::info!("🎯 Status: SUCCESS");
    
    // Trace-level function exit
    trace!("[TRACE][SUM] === FUNCTION EXIT: sum() ===");
    trace!("[TRACE][SUM] Function: sum(), UUID: {}", command_uuid);
    trace!("[TRACE][SUM] Exit status: SUCCESS");
    trace!("[TRACE][SUM] Total execution time: {:.3}s", total_time.as_secs_f64());
    trace!("[TRACE][SUM] Final content length: {} chars", content_length);
    trace!("[TRACE][SUM] Processing method used: {}", 
           if let Some(ref _path) = subtitle_file_path { "RAG with file" } else { "Direct processing" });
    trace!("[TRACE][SUM] Exit timestamp: {:?}", std::time::Instant::now());

    
    Ok(())
}

// Load summarization system prompt with multi-path fallback (like lm command)
// Loads summarization_prompt.txt from multiple locations, returns prompt string or fallback
async fn load_summarization_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let prompt_paths = [
        "summarization_prompt.txt",
        "../summarization_prompt.txt",
        "../../summarization_prompt.txt",
        "src/summarization_prompt.txt",
        "example_summarization_prompt.txt",
        "../example_summarization_prompt.txt",
        "../../example_summarization_prompt.txt",
        "src/example_summarization_prompt.txt",
    ];
    
    for path in &prompt_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                // Remove BOM if present
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                debug!("📄 Summarization prompt loaded from: {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    // Fallback prompt if no file found
    debug!("📄 Using built-in fallback summarization prompt");
    Ok("You are an expert content summarizer. Create a comprehensive, well-structured summary of the provided content. Use clear formatting and highlight key points. Keep the summary informative but concise.".to_string())
}

// Load YouTube summarization system prompt with multi-path fallback
// Loads youtube_summarization_prompt.txt from multiple locations, returns prompt string or fallback
async fn load_youtube_summarization_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let prompt_paths = [
        "youtube_summarization_prompt.txt",
        "../youtube_summarization_prompt.txt",
        "../../youtube_summarization_prompt.txt",
        "src/youtube_summarization_prompt.txt",
        "example_youtube_summarization_prompt.txt",
        "../example_youtube_summarization_prompt.txt",
        "../../example_youtube_summarization_prompt.txt",
        "src/example_youtube_summarization_prompt.txt",
    ];
    
    for path in &prompt_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                // Remove BOM if present
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                debug!("📺 YouTube summarization prompt loaded from: {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    // Fallback prompt if no file found
    debug!("📺 Using built-in fallback YouTube summarization prompt");
    Ok("You are an expert at summarizing YouTube video content. Focus on key points, main themes, and important takeaways. Structure your summary with clear sections and highlight the most valuable information for viewers.".to_string())
}

// Enhanced YouTube transcript fetcher using yt-dlp with detailed logging
// Generate a hash from YouTube URL for caching
fn generate_youtube_cache_key(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

// Downloads and cleans VTT subtitles for a given YouTube URL with caching
async fn fetch_youtube_transcript(url: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let process_uuid = Uuid::new_v4();
    
    // Trace-level function entry
    trace!("[TRACE][SUM][fetch_youtube_transcript] === FUNCTION ENTRY ===");
    trace!("[TRACE][SUM][fetch_youtube_transcript] Function: fetch_youtube_transcript()");
    trace!("[TRACE][SUM][fetch_youtube_transcript] Process UUID: {}", process_uuid);
    trace!("[TRACE][SUM][fetch_youtube_transcript] Input URL: '{}'", url);
    trace!("[TRACE][SUM][fetch_youtube_transcript] URL length: {} chars", url.len());
    trace!("[TRACE][SUM][fetch_youtube_transcript] Current working dir: {:?}", std::env::current_dir());
    
    info!("🎥 === YOUTUBE TRANSCRIPT EXTRACTION STARTED ===");
    info!("🆔 Process UUID: {}", process_uuid);
    info!("📍 Target URL: {}", url);
    
    // TEMPORARILY BYPASS CACHING - Use direct yt_transcript files
    info!("🔄 === CACHING BYPASSED ===");
    info!("🔄 Using direct yt_transcript files for RAG processing");
    debug!("🔄 Cache system temporarily disabled");
    trace!("🔍 Cache bypass: process_uuid={}", process_uuid);
    
    let temp_file = format!("yt_transcript_{}", Uuid::new_v4());
    info!("📁 Temp file base: {}", temp_file);
    
    debug!("🔧 === YOUTUBE TRANSCRIPT INITIALIZATION ===");
    debug!("🔧 URL length: {} characters", url.len());
    debug!("🔧 Temp file length: {} characters", temp_file.len());
    debug!("🔧 Process UUID: {}", process_uuid);
    trace!("🔍 YouTube transcript extraction details: url_length={}, temp_file_length={}, uuid={}", 
           url.len(), temp_file.len(), process_uuid);
    
    // Create subtitles directory if it doesn't exist
    debug!("📁 === DIRECTORY SETUP ===");
    let subtitles_dir = "subtitles";
    debug!("📁 Checking subtitles directory: {}", subtitles_dir);
    debug!("📁 Directory exists: {}", std::path::Path::new(subtitles_dir).exists());
    trace!("🔍 Directory check: path={}, exists={}, process_uuid={}", subtitles_dir, std::path::Path::new(subtitles_dir).exists(), process_uuid);
    
    if !std::path::Path::new(subtitles_dir).exists() {
        debug!("📁 Creating subtitles directory: {}", subtitles_dir);
        trace!("🔍 Directory creation started: path={}, process_uuid={}", subtitles_dir, process_uuid);
        std::fs::create_dir(subtitles_dir)?;
        debug!("✅ Subtitles directory created successfully");
        trace!("🔍 Directory creation completed: path={}, process_uuid={}", subtitles_dir, process_uuid);
    } else {
        debug!("📁 Subtitles directory already exists: {}", subtitles_dir);
        trace!("🔍 Directory already exists: path={}, process_uuid={}", subtitles_dir, process_uuid);
    }
    
    // Check if yt-dlp is available and get version
    debug!("🔍 === YT-DLP VERSION CHECK ===");
    debug!("🔍 Checking yt-dlp availability and version...");
    trace!("🔍 yt-dlp version check started: process_uuid={}", process_uuid);
    
    let version_output = Command::new("yt-dlp")
        .arg("--version")
        .output()
        .map_err(|e| {
            error!("❌ === YT-DLP NOT FOUND ERROR ===");
            error!("❌ yt-dlp is not installed or not in PATH: {}", e);
            debug!("🔍 yt-dlp PATH error details: {:?}", e);
            debug!("🔍 yt-dlp PATH error type: {:?}", std::any::type_name_of_val(&e));
            trace!("🔍 yt-dlp PATH error: error_type={}, process_uuid={}", 
                   std::any::type_name_of_val(&e), process_uuid);
            "yt-dlp is not installed. Please install yt-dlp to use YouTube summarization."
        })?;
    
    debug!("📊 === YT-DLP VERSION CHECK RESULTS ===");
    debug!("📊 yt-dlp version check exit status: {}", version_output.status);
    debug!("📊 yt-dlp version check success: {}", version_output.status.success());
    debug!("📊 yt-dlp stdout length: {} bytes", version_output.stdout.len());
    debug!("📊 yt-dlp stderr length: {} bytes", version_output.stderr.len());
    trace!("🔍 yt-dlp version check details: success={}, stdout_len={}, stderr_len={}, process_uuid={}", 
           version_output.status.success(), version_output.stdout.len(), version_output.stderr.len(), process_uuid);
    
    if !version_output.status.success() {
        error!("❌ === YT-DLP VERSION CHECK FAILED ===");
        error!("❌ yt-dlp version check failed");
        debug!("🔍 yt-dlp version check stderr: {}", String::from_utf8_lossy(&version_output.stderr));
        debug!("🔍 yt-dlp version check exit code: {:?}", version_output.status.code());
        trace!("🔍 yt-dlp version check failure: exit_code={:?}, process_uuid={}", version_output.status.code(), process_uuid);
        return Err("yt-dlp is not working properly".into());
    }
    
    let version_str = String::from_utf8_lossy(&version_output.stdout);
    info!("✅ === YT-DLP VERSION CHECK SUCCESS ===");
    info!("✅ yt-dlp version: {}", version_str.trim());
    debug!("🔧 yt-dlp version check completed successfully");
    debug!("🔧 Version string length: {} characters", version_str.trim().len());
    trace!("🔍 yt-dlp version check success: version={}, version_length={}, process_uuid={}", 
           version_str.trim(), version_str.trim().len(), process_uuid);
    
    // Try multiple subtitle extraction methods with retry logic
    info!("🔄 === SUBTITLE EXTRACTION PHASE ===");
    debug!("🔄 Starting subtitle extraction with retry logic...");
    trace!("🔍 Subtitle extraction phase started: process_uuid={}", process_uuid);
    
    let mut success = false;
    let mut last_error = String::new();
    let max_retries = 3;
    
    debug!("📊 === EXTRACTION CONFIGURATION ===");
    debug!("📊 Max retries: {}", max_retries);
    debug!("📊 Sleep interval: 2 seconds");
    debug!("📊 Max sleep interval: 5 seconds");
    debug!("📊 Temp file: {}", temp_file);
    debug!("📊 Subtitles directory: {}", subtitles_dir);
    trace!("🔍 Extraction configuration details: max_retries={}, temp_file={}, subtitles_dir={}, process_uuid={}", 
           max_retries, temp_file, subtitles_dir, process_uuid);
    
    for attempt in 1..=max_retries {
        info!("🔄 === ATTEMPT {}/{} STARTED ===", attempt, max_retries);
        debug!("🔄 Attempt {} of {} started", attempt, max_retries);
        trace!("🔍 Attempt {} started: attempt_number={}, max_retries={}, process_uuid={}", 
               attempt, attempt, max_retries, process_uuid);
        
        // Method 1: Try automatic subtitles first
        debug!("🔄 === METHOD 1: AUTOMATIC SUBTITLES ===");
        debug!("🔄 Method 1: Trying automatic subtitles...");
        trace!("🔍 Method 1 (automatic subtitles) started: attempt={}, process_uuid={}", attempt, process_uuid);
        
        let mut command = Command::new("yt-dlp");
        command
            .arg("--write-auto-sub")
            .arg("--write-sub")
            .arg("--sub-langs").arg("en")
            .arg("--sub-format").arg("vtt")
            .arg("--skip-download")
            .arg("--no-warnings")
            .arg("--no-playlist")
            .arg("--sleep-interval").arg("2")  // Add 2 second delay between requests
            .arg("--max-sleep-interval").arg("5")  // Max 5 second delay
            .arg("--retries").arg("3")  // Retry failed downloads
            .arg("--fragment-retries").arg("3")  // Retry failed fragments
            .arg("--user-agent").arg("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")  // Use realistic user agent
            .arg("--output").arg(&format!("{}/{}", subtitles_dir, temp_file))
            .arg(url);
        
        debug!("📋 === YT-DLP COMMAND ARGUMENTS ===");
        debug!("📋 yt-dlp command arguments:");
        debug!("📋   - --write-auto-sub");
        debug!("📋   - --write-sub");
        debug!("📋   - --sub-langs en");
        debug!("📋   - --sub-format vtt");
        debug!("📋   - --skip-download");
        debug!("📋   - --no-warnings");
        debug!("📋   - --no-playlist");
        debug!("📋   - --sleep-interval 2");
        debug!("📋   - --max-sleep-interval 5");
        debug!("📋   - --retries 3");
        debug!("📋   - --fragment-retries 3");
        debug!("📋   - --user-agent [Chrome 120]");
        debug!("📋   - --output {}/{}", subtitles_dir, temp_file);
        debug!("📋   - URL: {}", url);
        trace!("🔍 yt-dlp command details: attempt={}, output_path={}/{}, url_length={}, process_uuid={}", 
               attempt, subtitles_dir, temp_file, url.len(), process_uuid);
        
        debug!("🚀 === YT-DLP COMMAND EXECUTION ===");
        debug!("🚀 Executing yt-dlp command...");
        trace!("🔍 yt-dlp command execution started: attempt={}, process_uuid={}", attempt, process_uuid);
        
        let output = command.output()?;
        
        debug!("📊 === YT-DLP COMMAND RESULTS ===");
        debug!("📊 yt-dlp command completed with exit status: {}", output.status);
        debug!("📊 yt-dlp command success: {}", output.status.success());
        debug!("📊 yt-dlp stdout length: {} bytes", output.stdout.len());
        debug!("📊 yt-dlp stderr length: {} bytes", output.stderr.len());
        trace!("🔍 yt-dlp command execution completed: success={}, stdout_len={}, stderr_len={}, attempt={}, process_uuid={}", 
               output.status.success(), output.stdout.len(), output.stderr.len(), attempt, process_uuid);
        
        if output.status.success() {
            success = true;
            info!("✅ === METHOD 1 SUCCESS ===");
            info!("✅ Method 1 (automatic subtitles) succeeded on attempt {}", attempt);
            debug!("📄 yt-dlp stdout: {}", String::from_utf8_lossy(&output.stdout));
            trace!("🔍 Method 1 success details: attempt={}, stdout_length={}, process_uuid={}", 
                   attempt, output.stdout.len(), process_uuid);
            break;
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            last_error = stderr.to_string();
            
            warn!("❌ === METHOD 1 FAILED ===");
            warn!("❌ Method 1 failed on attempt {}", attempt);
            debug!("📄 yt-dlp stdout: {}", stdout);
            debug!("❌ yt-dlp stderr: {}", stderr);
            debug!("❌ stderr length: {} characters", stderr.len());
            debug!("❌ stdout length: {} characters", stdout.len());
            trace!("🔍 Method 1 failure details: attempt={}, stderr_length={}, stdout_length={}, process_uuid={}", 
                   attempt, stderr.len(), stdout.len(), process_uuid);
            
            // Check if it's a rate limit error
            debug!("🔍 === RATE LIMIT CHECK ===");
            debug!("🔍 Checking for rate limit errors...");
            debug!("🔍 stderr contains '429': {}", stderr.contains("429"));
            debug!("🔍 stderr contains 'Too Many Requests': {}", stderr.contains("Too Many Requests"));
            trace!("🔍 Rate limit detection: stderr_contains_429={}, stderr_contains_too_many_requests={}, attempt={}, process_uuid={}", 
                   stderr.contains("429"), stderr.contains("Too Many Requests"), attempt, process_uuid);
            
            if stderr.contains("429") || stderr.contains("Too Many Requests") {
                warn!("🚨 === RATE LIMIT DETECTED ===");
                warn!("🚨 Rate limit detected (429/Too Many Requests)");
                trace!("🔍 Rate limit detection: stderr_contains_429={}, stderr_contains_too_many_requests={}, attempt={}, process_uuid={}", 
                       stderr.contains("429"), stderr.contains("Too Many Requests"), attempt, process_uuid);
                
                if attempt < max_retries {
                    let delay = attempt * 5; // Exponential backoff: 5s, 10s, 15s
                    warn!("⏳ === RATE LIMIT DELAY ===");
                    warn!("⏳ Rate limited. Waiting {} seconds before retry...", delay);
                    debug!("⏳ Delay calculation: attempt={}, delay_seconds={}", attempt, delay);
                    trace!("🔍 Rate limit delay: delay_seconds={}, attempt={}, max_retries={}, process_uuid={}", 
                           delay, attempt, max_retries, process_uuid);
                    
                    tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
                    debug!("✅ Wait completed, proceeding to retry");
                    trace!("🔍 Rate limit delay completed, continuing to next attempt: process_uuid={}", process_uuid);
                    continue;
                } else {
                    warn!("❌ === MAX RETRIES REACHED ===");
                    warn!("❌ Max retries reached, cannot retry rate limit");
                    debug!("❌ Final attempt reached: attempt={}, max_retries={}", attempt, max_retries);
                    trace!("🔍 Max retries reached: attempt={}, max_retries={}, process_uuid={}", attempt, max_retries, process_uuid);
                }
            }
            
            // Method 2: Try manual subtitles only
            debug!("🔄 === METHOD 2: MANUAL SUBTITLES ===");
            debug!("🔄 Method 2: Trying manual subtitles only...");
            trace!("🔍 Method 2 (manual subtitles) started: attempt={}, process_uuid={}", attempt, process_uuid);
            
            let mut command2 = Command::new("yt-dlp");
            command2
                .arg("--write-sub")
                .arg("--sub-langs").arg("en")
                .arg("--sub-format").arg("vtt")
                .arg("--skip-download")
                .arg("--no-warnings")
                .arg("--no-playlist")
                .arg("--sleep-interval").arg("2")  // Add 2 second delay between requests
                .arg("--max-sleep-interval").arg("5")  // Max 5 second delay
                .arg("--retries").arg("3")  // Retry failed downloads
                .arg("--fragment-retries").arg("3")  // Retry failed fragments
                .arg("--user-agent").arg("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")  // Use realistic user agent
                .arg("--output").arg(&format!("{}/{}", subtitles_dir, temp_file))
                .arg(url);
            
            debug!("📋 === METHOD 2 COMMAND ARGUMENTS ===");
            debug!("📋 Method 2 yt-dlp command arguments:");
            debug!("📋   - --write-sub");
            debug!("📋   - --sub-langs en");
            debug!("📋   - --sub-format vtt");
            debug!("📋   - --skip-download");
            debug!("📋   - --no-warnings");
            debug!("📋   - --no-playlist");
            debug!("📋   - --sleep-interval 2");
            debug!("📋   - --max-sleep-interval 5");
            debug!("📋   - --retries 3");
            debug!("📋   - --fragment-retries 3");
            debug!("📋   - --user-agent [Chrome 120]");
            debug!("📋   - --output {}/{}", subtitles_dir, temp_file);
            debug!("📋   - URL: {}", url);
            trace!("🔍 Method 2 command details: attempt={}, output_path={}/{}, url_length={}, process_uuid={}", 
                   attempt, subtitles_dir, temp_file, url.len(), process_uuid);
            
            debug!("🚀 === METHOD 2 COMMAND EXECUTION ===");
            debug!("🚀 Executing Method 2 yt-dlp command...");
            trace!("🔍 Method 2 command execution started: attempt={}, process_uuid={}", attempt, process_uuid);
            
            let output2 = command2.output()?;
            
            debug!("📊 === METHOD 2 COMMAND RESULTS ===");
            debug!("📊 Method 2 yt-dlp command completed with exit status: {}", output2.status);
            debug!("📊 Method 2 yt-dlp command success: {}", output2.status.success());
            debug!("📊 Method 2 yt-dlp stdout length: {} bytes", output2.stdout.len());
            debug!("📊 Method 2 yt-dlp stderr length: {} bytes", output2.stderr.len());
            trace!("🔍 Method 2 command execution completed: success={}, stdout_len={}, stderr_len={}, attempt={}, process_uuid={}", 
                   output2.status.success(), output2.stdout.len(), output2.stderr.len(), attempt, process_uuid);
            
            if output2.status.success() {
                success = true;
                info!("✅ === METHOD 2 SUCCESS ===");
                info!("✅ Method 2 (manual subtitles) succeeded on attempt {}", attempt);
                debug!("📄 Method 2 yt-dlp stdout: {}", String::from_utf8_lossy(&output2.stdout));
                trace!("🔍 Method 2 success details: attempt={}, stdout_length={}, process_uuid={}", 
                       attempt, output2.stdout.len(), process_uuid);
                break;
            } else {
                let stderr2 = String::from_utf8_lossy(&output2.stderr);
                last_error = stderr2.to_string();
                
                warn!("❌ === METHOD 2 FAILED ===");
                warn!("❌ Method 2 failed on attempt {}: {}", attempt, stderr2);
                debug!("📄 Method 2 yt-dlp stdout: {}", String::from_utf8_lossy(&output2.stdout));
                debug!("❌ Method 2 yt-dlp stderr: {}", stderr2);
                debug!("❌ Method 2 stderr length: {} characters", stderr2.len());
                debug!("❌ Method 2 stdout length: {} characters", output2.stdout.len());
                trace!("🔍 Method 2 failure details: attempt={}, stderr_length={}, stdout_length={}, process_uuid={}", 
                       attempt, stderr2.len(), output2.stdout.len(), process_uuid);
                
                // Check if it's a rate limit error
                debug!("🔍 === METHOD 2 RATE LIMIT CHECK ===");
                debug!("🔍 Checking Method 2 for rate limit errors...");
                debug!("🔍 Method 2 stderr contains '429': {}", stderr2.contains("429"));
                debug!("🔍 Method 2 stderr contains 'Too Many Requests': {}", stderr2.contains("Too Many Requests"));
                trace!("🔍 Method 2 rate limit detection: stderr_contains_429={}, stderr_contains_too_many_requests={}, attempt={}, process_uuid={}", 
                       stderr2.contains("429"), stderr2.contains("Too Many Requests"), attempt, process_uuid);
                
                if stderr2.contains("429") || stderr2.contains("Too Many Requests") {
                    if attempt < max_retries {
                        let delay = attempt * 5; // Exponential backoff: 5s, 10s, 15s
                        warn!("⏳ === METHOD 2 RATE LIMIT DELAY ===");
                        warn!("⏳ Rate limited. Waiting {} seconds before retry...", delay);
                        debug!("⏳ Method 2 delay calculation: attempt={}, delay_seconds={}", attempt, delay);
                        trace!("🔍 Method 2 rate limit delay: delay_seconds={}, attempt={}, max_retries={}, process_uuid={}", 
                               delay, attempt, max_retries, process_uuid);
                        
                        tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
                        debug!("✅ Method 2 wait completed, proceeding to retry");
                        trace!("🔍 Method 2 rate limit delay completed, continuing to next attempt: process_uuid={}", process_uuid);
                        continue;
                    }
                }
            }
        }
    }
    
    if !success {
        error!("❌ === ALL SUBTITLE EXTRACTION METHODS FAILED ===");
        error!("❌ All subtitle extraction methods failed");
        error!("❌ Last error: {}", last_error);
        debug!("🔍 Final failure summary: success={}, last_error_length={}, process_uuid={}", 
               success, last_error.len(), process_uuid);
        trace!("🔍 All methods failed: success={}, last_error={}, process_uuid={}", 
               success, last_error, process_uuid);
        
        // Check for common error patterns and provide helpful messages
        debug!("🔍 === ERROR PATTERN ANALYSIS ===");
        debug!("🔍 Analyzing error patterns for helpful messages...");
        debug!("🔍 Error contains 'Did not get any data blocks': {}", last_error.contains("Did not get any data blocks"));
        debug!("🔍 Error contains 'Sign in to confirm you're not a bot': {}", last_error.contains("Sign in to confirm you're not a bot"));
        debug!("🔍 Error contains 'Private video': {}", last_error.contains("Private video"));
        debug!("🔍 Error contains 'Video unavailable': {}", last_error.contains("Video unavailable"));
        debug!("🔍 Error contains '429': {}", last_error.contains("429"));
        debug!("🔍 Error contains 'Too Many Requests': {}", last_error.contains("Too Many Requests"));
        debug!("🔍 Error contains 'No subtitles': {}", last_error.contains("No subtitles"));
        debug!("🔍 Error contains 'no automatic captions': {}", last_error.contains("no automatic captions"));
        debug!("🔍 Error contains 'This video is not available': {}", last_error.contains("This video is not available"));
        debug!("🔍 Error contains '403': {}", last_error.contains("403"));
        debug!("🔍 Error contains 'Forbidden': {}", last_error.contains("Forbidden"));
        debug!("🔍 Error contains 'fragment 1 not found': {}", last_error.contains("fragment 1 not found"));
        trace!("🔍 Error pattern analysis: data_blocks={}, bot_confirmation={}, private_video={}, video_unavailable={}, rate_limit={}, forbidden={}, fragment_not_found={}, no_subtitles={}, not_available={}, process_uuid={}", 
               last_error.contains("Did not get any data blocks"), last_error.contains("Sign in to confirm you're not a bot"), 
               last_error.contains("Private video"), last_error.contains("Video unavailable"), 
               last_error.contains("429") || last_error.contains("Too Many Requests"),
               last_error.contains("403") || last_error.contains("Forbidden"),
               last_error.contains("fragment 1 not found"),
               last_error.contains("No subtitles") || last_error.contains("no automatic captions"),
               last_error.contains("This video is not available"), process_uuid);
        
        if last_error.contains("Did not get any data blocks") {
            return Err("YouTube subtitles extraction failed: 'Did not get any data blocks'. This is usually caused by YouTube's anti-bot measures or an outdated yt-dlp version. Try updating yt-dlp with: yt-dlp -U".into());
        }
        
        if last_error.contains("Sign in to confirm you're not a bot") {
            return Err("YouTube is blocking requests: 'Sign in to confirm you're not a bot'. This is a temporary YouTube restriction. Try again later or use a different video.".into());
        }
        
        if last_error.contains("Private video") || last_error.contains("Video unavailable") {
            return Err("Video is private or unavailable. Please check the URL and try again.".into());
        }
        
        if last_error.contains("429") || last_error.contains("Too Many Requests") {
            return Err("YouTube is rate limiting requests. Please wait a few minutes and try again, or try a different video.".into());
        }
        
        if last_error.contains("403") || last_error.contains("Forbidden") {
            return Err("YouTube is blocking access to this video (403 Forbidden). This could be due to:\n• Video is age-restricted or private\n• YouTube's anti-bot measures\n• Regional restrictions\n• Try updating yt-dlp: `yt-dlp -U`\n• Try a different video or wait and retry later.".into());
        }
        
        if last_error.contains("fragment 1 not found") {
            return Err("YouTube video fragment not found. This usually indicates:\n• Video is being processed or temporarily unavailable\n• YouTube's servers are having issues\n• Try again in a few minutes\n• If persistent, try updating yt-dlp: `yt-dlp -U`".into());
        }
        
        if last_error.contains("No subtitles") || last_error.contains("no automatic captions") {
            return Err("This video has no automatic captions or subtitles available.".into());
        }
        
        if last_error.contains("Video unavailable") {
            return Err("This video is unavailable or has been removed.".into());
        }
        
        if last_error.contains("This video is not available") {
            return Err("This video is not available in your region or has been made private.".into());
        }
        
        return Err(format!("yt-dlp failed to extract subtitles: {}", last_error).into());
    }
    
    info!("✅ === YT-DLP SUBTITLE EXTRACTION SUCCESS ===");
    info!("✅ yt-dlp subtitle extraction completed successfully");
    debug!("🔧 Subtitle extraction phase completed: success={}, process_uuid={}", success, process_uuid);
    trace!("🔍 yt-dlp subtitle extraction success: process_uuid={}", process_uuid);
    
    // Look for the subtitle file with multiple possible naming patterns
    debug!("📄 === SUBTITLE FILE SEARCH ===");
    debug!("📄 Looking for subtitle files with multiple naming patterns...");
    
    let possible_vtt_files = vec![
        format!("{}/{}.en.vtt", subtitles_dir, temp_file),
        format!("{}/{}.en-auto.vtt", subtitles_dir, temp_file),
        format!("{}/{}.en-manual.vtt", subtitles_dir, temp_file),
        format!("{}/{}.vtt", subtitles_dir, temp_file),
    ];
    
    debug!("📄 Possible VTT file patterns: {:?}", possible_vtt_files);
    trace!("🔍 Subtitle file search: patterns={:?}, process_uuid={}", possible_vtt_files, process_uuid);
    
    // List all files in the subtitles directory that match the temp_file pattern
    debug!("📁 === DIRECTORY SCAN ===");
    debug!("📁 Scanning subtitles directory for matching files...");
    if let Ok(entries) = std::fs::read_dir(subtitles_dir) {
        let files: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains(&temp_file) && name.ends_with(".vtt"))
            .collect();
        debug!("📁 Found VTT files in subtitles directory: {:?}", files);
        debug!("📁 Total matching files found: {}", files.len());
        trace!("🔍 Directory scan: found_files={:?}, file_count={}, process_uuid={}", files, files.len(), process_uuid);
    } else {
        warn!("⚠️ Could not read subtitles directory for file listing");
        debug!("🔍 Directory read error: path={}, process_uuid={}", subtitles_dir, process_uuid);
    }
    
    let mut vtt_file = None;
    debug!("🔍 === FILE PATTERN MATCHING ===");
    for (i, file_path) in possible_vtt_files.iter().enumerate() {
        debug!("🔍 Checking pattern {}: {}", i+1, file_path);
        debug!("🔍 File exists: {}", std::path::Path::new(file_path).exists());
        trace!("🔍 File check: pattern={}, exists={}, process_uuid={}", file_path, std::path::Path::new(file_path).exists(), process_uuid);
        
        if std::path::Path::new(file_path).exists() {
            vtt_file = Some(file_path.clone());
            info!("✅ === SUBTITLE FILE FOUND ===");
            info!("✅ Found subtitle file: {}", file_path);
            debug!("📄 Selected subtitle file: {}", file_path);
            debug!("📄 File size: {} bytes", std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0));
            trace!("🔍 Subtitle file selected: path={}, size={}, process_uuid={}", 
                   file_path, std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0), process_uuid);
            break;
        } else {
            trace!("🔍 Subtitle file not found: path={}, process_uuid={}", file_path, process_uuid);
        }
    }
    
    let vtt_file = match vtt_file {
        Some(path) => path,
        None => {
            error!("❌ === NO SUBTITLE FILE FOUND ===");
            error!("❌ No subtitle file found with any expected pattern");
            debug!("🔍 File search failed: checked_patterns={}, process_uuid={}", possible_vtt_files.len(), process_uuid);
            
            // List files in subtitles directory for debugging
            debug!("📁 === DEBUGGING DIRECTORY CONTENTS ===");
            if let Ok(entries) = std::fs::read_dir(subtitles_dir) {
                let files: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|name| name.contains(&temp_file) && name.ends_with(".vtt"))
                    .collect();
                debug!("📁 Found VTT files in subtitles directory: {:?}", files);
                debug!("📁 Total matching files found: {}", files.len());
                trace!("🔍 Directory scan (on error): found_files={:?}, file_count={}, process_uuid={}", files, files.len(), process_uuid);
            }
            return Err("Subtitle file was not created by yt-dlp. The video may not have automatic captions available.".into());
        }
    };
    
    debug!("📖 === SUBTITLE FILE READING ===");
    debug!("📖 Reading subtitle file: {}", vtt_file);
    trace!("🔍 Subtitle file read started: path={}, process_uuid={}", vtt_file, process_uuid);
    
    let content = fs::read_to_string(&vtt_file)?;
    
    debug!("📖 === SUBTITLE FILE READ SUCCESS ===");
    debug!("📖 Read subtitle file: {} characters from {}", content.len(), vtt_file);
    debug!("📖 File content preview: {}", &content[..std::cmp::min(100, content.len())]);
    debug!("📖 File content contains 'WEBVTT': {}", content.contains("WEBVTT"));
    debug!("📖 File content is empty: {}", content.trim().is_empty());
    trace!("🔍 Subtitle file read: path={}, length={}, preview='{}', process_uuid={}", 
           vtt_file, content.len(), &content[..std::cmp::min(100, content.len())], process_uuid);
    
    // Check if content is valid
    debug!("🔍 === SUBTITLE CONTENT VALIDATION ===");
    debug!("🔍 Validating subtitle file content...");
    
    if content.trim().is_empty() {
        error!("❌ === EMPTY SUBTITLE FILE ERROR ===");
        error!("❌ Downloaded subtitle file is empty: {}", vtt_file);
        debug!("🔍 Subtitle file empty: path={}, content_length={}", vtt_file, content.len());
        trace!("🔍 Subtitle file empty: path={}, process_uuid={}", vtt_file, process_uuid);
        return Err("Downloaded subtitle file is empty".into());
    }
    
    if !content.contains("WEBVTT") {
        error!("❌ === INVALID VTT FILE ERROR ===");
        error!("❌ Downloaded file is not a valid VTT subtitle file: {}", vtt_file);
        debug!("🔍 Subtitle file missing WEBVTT header: path={}", vtt_file);
        debug!("🔍 File content starts with: {}", &content[..std::cmp::min(50, content.len())]);
        trace!("🔍 Subtitle file missing WEBVTT header: path={}, process_uuid={}", vtt_file, process_uuid);
        return Err("Downloaded file is not a valid VTT subtitle file".into());
    }
    
    debug!("✅ Subtitle content validation passed");
    trace!("🔍 Subtitle content validation success: path={}, process_uuid={}", vtt_file, process_uuid);
    
    // Clean VTT content
    debug!("🧹 === VTT CONTENT CLEANING ===");
    debug!("🧹 Cleaning VTT content from file: {}", vtt_file);
    trace!("🔍 VTT cleaning started: original_length={}, process_uuid={}", content.len(), process_uuid);
    
    let cleaned = clean_vtt_content(&content);
    
    debug!("✅ === VTT CLEANING COMPLETED ===");
    debug!("✅ VTT content cleaned: {} characters", cleaned.len());
    debug!("✅ Cleaning ratio: {:.2}%", (cleaned.len() as f64 / content.len() as f64) * 100.0);
    debug!("✅ Cleaned content preview: {}", &cleaned[..std::cmp::min(100, cleaned.len())]);
    trace!("🔍 VTT cleaning completed: cleaned_length={}, preview='{}', process_uuid={}", 
           cleaned.len(), &cleaned[..std::cmp::min(100, cleaned.len())], process_uuid);
    
    if cleaned.trim().is_empty() {
        error!("❌ === EMPTY CLEANED CONTENT ERROR ===");
        error!("❌ No readable text found in subtitle file after cleaning: {}", vtt_file);
        debug!("🔍 Cleaned subtitle file empty: path={}, original_length={}, cleaned_length={}", vtt_file, content.len(), cleaned.len());
        trace!("🔍 Cleaned subtitle file empty: path={}, process_uuid={}", vtt_file, process_uuid);
        return Err("No readable text found in subtitle file after cleaning".into());
    }
    
    info!("✅ === YOUTUBE TRANSCRIPT EXTRACTION COMPLETED ===");
    info!("✅ YouTube transcript extraction completed successfully");
    debug!("📄 Final subtitle file: {}", vtt_file);
    debug!("📄 Original content: {} characters", content.len());
    debug!("📄 Cleaned content: {} characters", cleaned.len());
    debug!("📄 Process UUID: {}", process_uuid);
    trace!("🔍 YouTube transcript extraction success: file_path={}, original_length={}, cleaned_length={}, process_uuid={}", 
           vtt_file, content.len(), cleaned.len(), process_uuid);
    
    // TEMPORARILY BYPASS CACHING - Return temporary file directly
    info!("🔄 === RETURNING TEMPORARY FILE PATH ===");
    info!("🔄 Returning temporary file path for RAG processing: {}", vtt_file);
    debug!("📄 Temporary file path: {}", vtt_file);
    trace!("🔍 Returning temporary path: temp={}, process_uuid={}", vtt_file, process_uuid);
    
    Ok(vtt_file)
}

// Enhanced VTT cleaner
// Removes timestamps, tags, and empty lines from VTT subtitle content
fn clean_vtt_content(vtt: &str) -> String {
    debug!("🧹 === VTT CLEANING STARTED ===");
    debug!("🧹 Cleaning VTT content...");
    debug!("🧹 Original VTT content length: {} characters", vtt.len());
    debug!("🧹 Original VTT line count: {}", vtt.lines().count());
    trace!("🔍 VTT cleaning started: original_length={}, line_count={}", vtt.len(), vtt.lines().count());
    
    let mut lines = Vec::new();
    let mut processed_lines = 0;
    let mut skipped_lines = 0;
    let mut kept_lines = 0;
    
    debug!("📝 === LINE PROCESSING ===");
    for (line_num, line) in vtt.lines().enumerate() {
        processed_lines += 1;
        let original_line = line;
        let line = line.trim();
        
        // Skip headers, timestamps, and empty lines
        let is_empty = line.is_empty();
        let is_webvtt = line.starts_with("WEBVTT");
        let is_note = line.starts_with("NOTE");
        let is_timestamp = line.contains("-->");
        let is_numeric = line.chars().all(|c| c.is_numeric() || c == ':' || c == '.' || c == ' ');
        
        if is_empty || is_webvtt || is_note || is_timestamp || is_numeric {
            skipped_lines += 1;
            trace!("🔍 Line {} skipped: empty={}, webvtt={}, note={}, timestamp={}, numeric={}, content='{}'", 
                   line_num + 1, is_empty, is_webvtt, is_note, is_timestamp, is_numeric, original_line);
            continue;
        }
        
        // Clean various subtitle tags
        debug!("🧹 === TAG CLEANING ===");
        let mut cleaned = line.to_string();
        
        // Track tag removals
        let original_cleaned = cleaned.clone();
        let mut tags_removed = 0;
        
        // Remove various subtitle tags
        if cleaned.contains("<c>") {
            cleaned = cleaned.replace("<c>", "");
            tags_removed += 1;
        }
        if cleaned.contains("</c>") {
            cleaned = cleaned.replace("</c>", "");
            tags_removed += 1;
        }
        if cleaned.contains("<v ") {
            cleaned = cleaned.replace("<v ", "");
            tags_removed += 1;
        }
        if cleaned.contains("</v>") {
            cleaned = cleaned.replace("</v>", "");
            tags_removed += 1;
        }
        if cleaned.contains("<b>") {
            cleaned = cleaned.replace("<b>", "");
            tags_removed += 1;
        }
        if cleaned.contains("</b>") {
            cleaned = cleaned.replace("</b>", "");
            tags_removed += 1;
        }
        if cleaned.contains("<i>") {
            cleaned = cleaned.replace("<i>", "");
            tags_removed += 1;
        }
        if cleaned.contains("</i>") {
            cleaned = cleaned.replace("</i>", "");
            tags_removed += 1;
        }
        if cleaned.contains("<u>") {
            cleaned = cleaned.replace("<u>", "");
            tags_removed += 1;
        }
        if cleaned.contains("</u>") {
            cleaned = cleaned.replace("</u>", "");
            tags_removed += 1;
        }
        
        cleaned = cleaned.trim().to_string();
        
        if tags_removed > 0 {
            trace!("🔍 Line {} tag cleaning: removed {} tags, '{}' -> '{}'", 
                   line_num + 1, tags_removed, original_cleaned, cleaned);
        }
        
        if !cleaned.is_empty() {
            lines.push(cleaned.clone());
            kept_lines += 1;
            trace!("🔍 Line {} kept: '{}'", line_num + 1, cleaned);
        } else {
            skipped_lines += 1;
            trace!("🔍 Line {} skipped after cleaning: was '{}'", line_num + 1, original_line);
        }
    }
    
    debug!("📊 === LINE PROCESSING STATISTICS ===");
    debug!("📊 Total lines processed: {}", processed_lines);
    debug!("📊 Lines skipped: {}", skipped_lines);
    debug!("📊 Lines kept: {}", kept_lines);
    debug!("📊 Keep ratio: {:.2}%", (kept_lines as f64 / processed_lines as f64) * 100.0);
    
    let result = lines.join(" ");
    debug!("🔗 === LINE JOINING ===");
    debug!("🔗 Joined {} lines into single string", lines.len());
    debug!("🔗 Result length: {} characters", result.len());
    trace!("🔍 Line joining completed: line_count={}, result_length={}", lines.len(), result.len());
    
    // Additional cleanup: remove excessive whitespace
    debug!("🧹 === WHITESPACE CLEANUP ===");
    let _original_result = result.clone();
    let final_result = result
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    
    debug!("🧹 === FINAL VTT CLEANING COMPLETED ===");
    debug!("🧹 VTT cleaning complete: {} lines -> {} characters", lines.len(), result.len());
    debug!("🧹 Final VTT cleaning: {} -> {} characters", result.len(), final_result.len());
    debug!("🧹 Total reduction: {:.2}%", (final_result.len() as f64 / vtt.len() as f64) * 100.0);
    debug!("🧹 Final result preview: {}", &final_result[..std::cmp::min(100, final_result.len())]);
    
    trace!("🔍 VTT cleaning final: original_length={}, processed_lines={}, kept_lines={}, final_length={}, reduction_percent={:.2}%", 
           vtt.len(), processed_lines, kept_lines, final_result.len(), 
           (final_result.len() as f64 / vtt.len() as f64) * 100.0);
    
    final_result
}

// Simple webpage fetcher with improved connectivity
// Downloads and cleans HTML content for a given URL using the shared HTTP client
async fn fetch_webpage_content(url: &str) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    let fetch_uuid = Uuid::new_v4();
    
    info!("🌐 === WEBPAGE FETCHING STARTED ===");
    info!("🆔 Fetch UUID: {}", fetch_uuid);
    info!("📍 Target URL: {}", url);
    
    debug!("🔧 === WEBPAGE FETCH INITIALIZATION ===");
    debug!("🔧 URL length: {} characters", url.len());
    debug!("🔧 Fetch UUID: {}", fetch_uuid);
    trace!("🔍 Webpage fetch started: url_length={}, fetch_uuid={}", url.len(), fetch_uuid);
    
    debug!("🌐 Starting webpage fetch for URL: {}", url);
    
    debug!("🔧 === HTTP CLIENT SETUP ===");
    debug!("🔧 Using shared HTTP client with optimized settings...");
    trace!("🔍 HTTP client setup started: fetch_uuid={}", fetch_uuid);
    
    let client = get_http_client().await;
    
    debug!("✅ Shared HTTP client obtained successfully");
    debug!("🔧 Using optimized connection pooling and settings");
    trace!("🔍 HTTP client obtained: fetch_uuid={}", fetch_uuid);
    
    debug!("📡 === HTTP REQUEST EXECUTION ===");
    debug!("📡 Sending HTTP request...");
    trace!("🔍 HTTP request started: url={}, fetch_uuid={}", url, fetch_uuid);
    
    let response = client.get(url).send().await?;
    let status = response.status();
    
    debug!("📡 === HTTP RESPONSE RECEIVED ===");
    debug!("📡 HTTP Response Status: {}", status);
    debug!("📡 HTTP Response Status Code: {}", status.as_u16());
    debug!("📡 HTTP Response Success: {}", status.is_success());
    debug!("📡 HTTP Response Headers: {:?}", response.headers());
    trace!("🔍 HTTP response received: status={}, status_code={}, success={}, fetch_uuid={}", 
           status, status.as_u16(), status.is_success(), fetch_uuid);
    
    if !response.status().is_success() {
        error!("❌ === HTTP ERROR RESPONSE ===");
        error!("❌ HTTP error: {}", status);
        debug!("🔍 HTTP error details: status_code={}, status_text={}", status.as_u16(), status.as_str());
        trace!("🔍 HTTP error: status={}, fetch_uuid={}", status, fetch_uuid);
        return Err(format!("HTTP error: {}", response.status()).into());
    }
    
    debug!("📄 === HTML CONTENT DOWNLOAD ===");
    debug!("📄 Downloading HTML content...");
    trace!("🔍 HTML content download started: fetch_uuid={}", fetch_uuid);
    
    let html = response.text().await?;
    
    debug!("📄 === HTML CONTENT DOWNLOADED ===");
    debug!("📄 Downloaded HTML content: {} characters", html.len());
    debug!("📄 HTML content preview: {}", &html[..std::cmp::min(200, html.len())]);
    debug!("📄 HTML contains '<html': {}", html.contains("<html"));
    debug!("📄 HTML contains '<body': {}", html.contains("<body"));
    debug!("📄 HTML contains '<head': {}", html.contains("<head"));
    trace!("🔍 HTML content downloaded: length={}, preview_length={}, fetch_uuid={}", 
           html.len(), std::cmp::min(200, html.len()), fetch_uuid);
    
    // Save HTML to temporary file for RAG processing
    debug!("💾 === HTML FILE SAVING ===");
    debug!("💾 Saving HTML content to temporary file...");
    trace!("🔍 HTML file saving started: html_length={}, fetch_uuid={}", html.len(), fetch_uuid);
    
    let temp_dir = std::env::temp_dir();
    let file_name = format!("webpage_{}.html", fetch_uuid);
    let file_path = temp_dir.join(&file_name);
    
    debug!("💾 Temporary file path: {:?}", file_path);
    debug!("💾 File name: {}", file_name);
    trace!("🔍 File path created: path={:?}, fetch_uuid={}", file_path, fetch_uuid);
    
    match fs::write(&file_path, &html) {
        Ok(_) => {
            debug!("✅ HTML file saved successfully");
            debug!("💾 File size: {} bytes", html.len());
            debug!("💾 File path: {:?}", file_path);
            trace!("🔍 HTML file saved: path={:?}, size={}, fetch_uuid={}", file_path, html.len(), fetch_uuid);
        },
        Err(e) => {
            error!("❌ === HTML FILE SAVE ERROR ===");
            error!("❌ Failed to save HTML file: {}", e);
            debug!("🔍 File save error: path={:?}, error={}", file_path, e);
            trace!("🔍 File save error: path={:?}, error_type={}, fetch_uuid={}", 
                   file_path, std::any::type_name_of_val(&e), fetch_uuid);
            return Err(format!("Failed to save HTML file: {}", e).into());
        }
    }
    
    // Basic HTML cleaning for immediate use
    debug!("🧹 === HTML CLEANING PHASE ===");
    debug!("🧹 Starting HTML content cleaning...");
    trace!("🔍 HTML cleaning started: original_length={}, fetch_uuid={}", html.len(), fetch_uuid);
    
    let cleaned = clean_html(&html);
    
    debug!("✅ === HTML CLEANING COMPLETED ===");
    debug!("✅ HTML content cleaned: {} characters", cleaned.len());
    debug!("✅ Cleaning ratio: {:.2}%", (cleaned.len() as f64 / html.len() as f64) * 100.0);
    debug!("✅ Cleaned content preview: {}", &cleaned[..std::cmp::min(200, cleaned.len())]);
    trace!("🔍 HTML cleaning completed: original_length={}, cleaned_length={}, reduction_percent={:.2}%, fetch_uuid={}", 
           html.len(), cleaned.len(), (cleaned.len() as f64 / html.len() as f64) * 100.0, fetch_uuid);
    
    info!("✅ === WEBPAGE FETCHING COMPLETED ===");
    info!("✅ Webpage content fetched, saved to file, and cleaned successfully");
    debug!("📄 Final content length: {} characters", cleaned.len());
    debug!("💾 HTML file saved: {:?}", file_path);
    debug!("📄 Fetch UUID: {}", fetch_uuid);
    trace!("🔍 Webpage fetch success: final_length={}, file_path={:?}, fetch_uuid={}", cleaned.len(), file_path, fetch_uuid);
    
    Ok((cleaned, file_path.to_string_lossy().to_string()))
}

// Simple HTML cleaner
// Removes script/style tags and all HTML tags, returns plain text
fn clean_html(html: &str) -> String {
    let clean_uuid = Uuid::new_v4();
    
    debug!("🧹 === HTML CLEANING STARTED ===");
    debug!("🆔 Clean UUID: {}", clean_uuid);
    debug!("🧹 Cleaning HTML content...");
    debug!("🧹 Original HTML length: {} characters", html.len());
    trace!("🔍 HTML cleaning started: original_length={}, clean_uuid={}", html.len(), clean_uuid);
    
    // Remove script and style tags
    let mut result = html.to_string();
    let _original_result = result.clone();
    
    debug!("🧹 === SCRIPT TAG REMOVAL ===");
    debug!("🧹 Removing script tags...");
    let mut script_removals = 0;
    let mut script_removal_rounds = 0;
    
    // Remove script tags
    while let Some(start) = result.find("<script") {
        script_removal_rounds += 1;
        if let Some(end) = result[start..].find("</script>") {
            let script_content = &result[start..start + end + 9];
            script_removals += 1;
            debug!("🧹 Removed script tag {}: {} characters", script_removals, script_content.len());
            trace!("🔍 Script removal: round={}, removal={}, script_length={}, clean_uuid={}", 
                   script_removal_rounds, script_removals, script_content.len(), clean_uuid);
            result.replace_range(start..start + end + 9, "");
        } else {
            debug!("🧹 Found incomplete script tag, stopping removal");
            trace!("🔍 Incomplete script tag found: round={}, clean_uuid={}", script_removal_rounds, clean_uuid);
            break;
        }
    }
    
    debug!("✅ Script tag removal completed: {} removals in {} rounds", script_removals, script_removal_rounds);
    
    debug!("🧹 === STYLE TAG REMOVAL ===");
    debug!("🧹 Removing style tags...");
    let mut style_removals = 0;
    let mut style_removal_rounds = 0;
    
    // Remove style tags
    while let Some(start) = result.find("<style") {
        style_removal_rounds += 1;
        if let Some(end) = result[start..].find("</style>") {
            let style_content = &result[start..start + end + 8];
            style_removals += 1;
            debug!("🧹 Removed style tag {}: {} characters", style_removals, style_content.len());
            trace!("🔍 Style removal: round={}, removal={}, style_length={}, clean_uuid={}", 
                   style_removal_rounds, style_removals, style_content.len(), clean_uuid);
            result.replace_range(start..start + end + 8, "");
        } else {
            debug!("🧹 Found incomplete style tag, stopping removal");
            trace!("🔍 Incomplete style tag found: round={}, clean_uuid={}", style_removal_rounds, clean_uuid);
            break;
        }
    }
    
    debug!("✅ Style tag removal completed: {} removals in {} rounds", style_removals, style_removal_rounds);
    
    debug!("🧹 === HTML TAG REMOVAL ===");
    debug!("🧹 Removing all remaining HTML tags...");
    trace!("🔍 HTML tag removal started: current_length={}, clean_uuid={}", result.len(), clean_uuid);
    
    // Remove all HTML tags
    let tag_regex = regex::Regex::new(r"<[^>]+>").unwrap();
    let cleaned = tag_regex.replace_all(&result, " ");
    
    debug!("✅ HTML tag removal completed");
    debug!("🧹 Content after tag removal: {} characters", cleaned.len());
    debug!("🧹 Tag removal reduction: {:.2}%", (cleaned.len() as f64 / result.len() as f64) * 100.0);
    trace!("🔍 HTML tag removal completed: before_length={}, after_length={}, reduction_percent={:.2}%, clean_uuid={}", 
           result.len(), cleaned.len(), (cleaned.len() as f64 / result.len() as f64) * 100.0, clean_uuid);
    
    debug!("🧹 === WHITESPACE CLEANUP ===");
    debug!("🧹 Cleaning whitespace...");
    let before_whitespace = cleaned.len();
    
    // Clean whitespace
    let final_result: String = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(15000)
        .collect();
    
    debug!("✅ Whitespace cleanup completed");
    debug!("🧹 Content after whitespace cleanup: {} characters", final_result.len());
    debug!("🧹 Whitespace cleanup reduction: {:.2}%", (final_result.len() as f64 / before_whitespace as f64) * 100.0);
    debug!("🧹 Content truncated to 15000 characters: {}", final_result.len() >= 15000);
    trace!("🔍 Whitespace cleanup: before_length={}, after_length={}, truncated={}, clean_uuid={}", 
           before_whitespace, final_result.len(), final_result.len() >= 15000, clean_uuid);
    
    debug!("🧹 === FINAL HTML CLEANING COMPLETED ===");
    debug!("🧹 HTML cleaning complete: {} -> {} characters", html.len(), final_result.len());
    debug!("🧹 Total reduction: {:.2}%", (final_result.len() as f64 / html.len() as f64) * 100.0);
    debug!("🧹 Final result preview: {}", &final_result[..std::cmp::min(100, final_result.len())]);
    debug!("🧹 Clean UUID: {}", clean_uuid);
    
    trace!("🔍 HTML cleaning final: original_length={}, script_removals={}, style_removals={}, final_length={}, total_reduction_percent={:.2}%, clean_uuid={}", 
           html.len(), script_removals, style_removals, final_result.len(), 
           (final_result.len() as f64 / html.len() as f64) * 100.0, clean_uuid);
    
    final_result
}

// Stream summary using SSE (like lm command approach)
// Streams the AI's summary response, chunking and updating Discord messages as needed
async fn stream_summary(
    content: &str,
    url: &str,
    config: &LMConfig,
    selected_model: &str,
    msg: &mut Message,
    ctx: &Context,
    is_youtube: bool,
    file_path: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    
    let stream_uuid = Uuid::new_v4();
    
    // Trace-level function entry
    trace!("[TRACE][SUM][stream_summary] === FUNCTION ENTRY ===");
    trace!("[TRACE][SUM][stream_summary] Function: stream_summary()");
    trace!("[TRACE][SUM][stream_summary] Stream UUID: {}", stream_uuid);
    trace!("[TRACE][SUM][stream_summary] Input content length: {} chars", content.len());
    trace!("[TRACE][SUM][stream_summary] URL: '{}'", url);
    trace!("[TRACE][SUM][stream_summary] Is YouTube: {}", is_youtube);
    trace!("[TRACE][SUM][stream_summary] File path: {:?}", file_path);
    trace!("[TRACE][SUM][stream_summary] Config base URL: {}", config.base_url);
    trace!("[TRACE][SUM][stream_summary] Selected model: {}", selected_model);
    trace!("[TRACE][SUM][stream_summary] Config temperature: {}", config.default_temperature);
    trace!("[TRACE][SUM][stream_summary] Config max tokens: {}", config.default_max_tokens);
    
    info!("🤖 === AI SUMMARIZATION STREAMING STARTED ===");
    info!("🆔 Stream UUID: {}", stream_uuid);
    info!("🌐 URL: {}", url);
    info!("📺 Content type: {}", if is_youtube { "YouTube" } else { "Webpage" });
    info!("📄 Content length: {} characters", content.len());
    
    debug!("🔧 === STREAM SUMMARY INITIALIZATION ===");
    debug!("🔧 Stream UUID: {}", stream_uuid);
    debug!("🔧 URL length: {} characters", url.len());
    debug!("🔧 Content length: {} characters", content.len());
    debug!("🔧 Is YouTube: {}", is_youtube);
    debug!("🔧 File path: {:?}", file_path);
    debug!("🔧 Model: {}", selected_model);
    debug!("🔧 Base URL: {}", config.base_url);
    debug!("🔧 Temperature: {}", config.default_temperature);
    debug!("🔧 Max tokens: {}", config.default_max_tokens);
    trace!("🔍 Stream summary started: content_length={}, url={}, is_youtube={}, model={}, stream_uuid={}", 
           content.len(), url, is_youtube, selected_model, stream_uuid);
    
    debug!("🤖 Preparing AI request...");
    trace!("🔍 Stream summary started: content_length={}, url={}, is_youtube={}, model={}, stream_uuid={}", 
           content.len(), url, is_youtube, selected_model, stream_uuid);    
    
    // Load appropriate system prompt from files
    debug!("📄 === SYSTEM PROMPT LOADING ===");
    debug!("📄 Loading system prompt for content type: {}", if is_youtube { "YouTube" } else { "Webpage" });
    trace!("🔍 Loading system prompt: is_youtube={}, stream_uuid={}", is_youtube, stream_uuid);
    
    let system_prompt = if is_youtube {
        debug!("📺 Loading YouTube summarization prompt...");
        match load_youtube_summarization_prompt().await {
            Ok(prompt) => {
                debug!("✅ YouTube summarization prompt loaded: {} characters", prompt.len());
                trace!("🔍 YouTube prompt loaded: length={}, stream_uuid={}", prompt.len(), stream_uuid);
                prompt
            },
            Err(e) => {
                error!("❌ Failed to load YouTube summarization prompt: {}", e);
                debug!("🔍 YouTube prompt error: {:?}", e);
                trace!("🔍 YouTube prompt error: error_type={}, stream_uuid={}", 
                       std::any::type_name_of_val(&e), stream_uuid);
                return Err(e);
            }
        }
    } else {
        debug!("📄 Loading general summarization prompt...");
        match load_summarization_prompt().await {
            Ok(prompt) => {
                debug!("✅ General summarization prompt loaded: {} characters", prompt.len());
                trace!("🔍 General prompt loaded: length={}, stream_uuid={}", prompt.len(), stream_uuid);
                prompt
            },
            Err(e) => {
                error!("❌ Failed to load general summarization prompt: {}", e);
                debug!("🔍 General prompt error: {:?}", e);
                trace!("🔍 General prompt error: error_type={}, stream_uuid={}", 
                       std::any::type_name_of_val(&e), stream_uuid);
                return Err(e);
            }
        }
    };
    
    debug!("📄 System prompt loaded successfully: {} characters", system_prompt.len());
    debug!("📄 System prompt preview: {}", &system_prompt[..std::cmp::min(200, system_prompt.len())]);
    trace!("🔍 System prompt loaded: length={}, stream_uuid={}", system_prompt.len(), stream_uuid);
    
    // FIXED: Properly handle content processing for YouTube vs webpage
    trace!("[TRACE][SUM][stream_summary] === CONTENT PROCESSING ENTRY ===");
    trace!("[TRACE][SUM][stream_summary] File path provided: {}", file_path.is_some());
    trace!("[TRACE][SUM][stream_summary] Content type: {}", if is_youtube { "YouTube" } else { "Webpage" });
    trace!("[TRACE][SUM][stream_summary] Input content length: {} chars", content.len());
    
    debug!("🔧 === CONTENT PROCESSING ===");
    debug!("🔧 Processing content for AI request...");
    
    let (user_prompt, content_to_process) = if file_path.is_some() {
        // Use RAG document processing with the file (YouTube subtitle or HTML)
        let file_path = file_path.unwrap();
        debug!("🔧 === RAG PROCESSING TRIGGERED ===");
        debug!("🔧 File path provided: {}", file_path);
        debug!("🔧 File exists: {}", std::path::Path::new(file_path).exists());
        debug!("🔧 Is YouTube: {}", is_youtube);
        debug!("📁 === RAG DOCUMENT PROCESSING ===");
        debug!("📁 Using RAG document processing for file: {}", file_path);
        debug!("📁 Content type: {}", if is_youtube { "YouTube subtitle" } else { "HTML webpage" });
        trace!("🔍 RAG document processing: file_path={}, content_type={}, stream_uuid={}", 
               file_path, if is_youtube { "youtube" } else { "webpage" }, stream_uuid);
        
        // Enhanced logging for RAG processing
        log::info!("📁 === RAG PROCESSING STARTED ===");
        log::info!("📁 File path: {}", file_path);
        log::info!("📁 Content type: {}", if is_youtube { "YouTube subtitle" } else { "HTML webpage" });
        log::info!("📁 File exists: {}", std::path::Path::new(file_path).exists());
        log::info!("📁 Stream UUID: {}", stream_uuid);
        
        // Read the file content
        debug!("📖 === FILE READING FOR RAG ===");
        debug!("📖 Reading file for RAG processing: {}", file_path);
        let file_content = match fs::read_to_string(file_path) {
            Ok(content) => {
                debug!("✅ File read successfully: {} characters", content.len());
                debug!("📖 File content preview: {}", &content[..std::cmp::min(200, content.len())]);
                trace!("🔍 File read success: path={}, length={}, stream_uuid={}", file_path, content.len(), stream_uuid);
                
                // Enhanced logging for file reading success
                log::info!("📖 === FILE READ SUCCESS ===");
                log::info!("📖 File path: {}", file_path);
                log::info!("📖 File size: {} characters", content.len());
                log::info!("📖 File size in bytes: {} bytes", content.as_bytes().len());
                log::info!("📖 Content preview: {}", &content[..std::cmp::min(500, content.len())]);
                log::info!("📖 Content type indicators:");
                log::info!("📖   - Contains HTML tags: {}", content.contains("<html"));
                log::info!("📖   - Contains VTT timestamps: {}", content.contains("-->"));
                log::info!("📖   - Contains script tags: {}", content.contains("<script"));
                log::info!("📖   - Contains style tags: {}", content.contains("<style"));
                
                content
            },
            Err(e) => {
                error!("❌ === FILE READ ERROR ===");
                error!("❌ Failed to read file: {}", e);
                debug!("🔍 File read error: path={}, error={}", file_path, e);
                debug!("🔍 File read error type: {:?}", std::any::type_name_of_val(&e));
                trace!("🔍 File read error: path={}, error_type={}, stream_uuid={}", 
                       file_path, std::any::type_name_of_val(&e), stream_uuid);
                return Err(format!("Failed to read file: {}", e).into());
            }
        };
        
        // Clean the content based on file type
        let cleaned_content = if is_youtube {
            debug!("🧹 === VTT CLEANING FOR RAG ===");
            debug!("🧹 Cleaning VTT content for RAG processing...");
            trace!("🔍 VTT cleaning for RAG: original_length={}, stream_uuid={}", file_content.len(), stream_uuid);
            
            let cleaned = clean_vtt_content(&file_content);
            
            debug!("✅ VTT content cleaned for RAG: {} characters", cleaned.len());
            debug!("🧹 Content preview: {}", &cleaned[..std::cmp::min(200, cleaned.len())]);
            debug!("🧹 Cleaning ratio: {:.2}%", (cleaned.len() as f64 / file_content.len() as f64) * 100.0);
            trace!("🔍 VTT cleaning for RAG completed: original_length={}, cleaned_length={}, stream_uuid={}", 
                   file_content.len(), cleaned.len(), stream_uuid);
            
            // Enhanced logging for VTT cleaning
            log::info!("🧹 === VTT CLEANING COMPLETED ===");
            log::info!("🧹 Original size: {} characters", file_content.len());
            log::info!("🧹 Cleaned size: {} characters", cleaned.len());
            log::info!("🧹 Reduction: {:.2}%", (cleaned.len() as f64 / file_content.len() as f64) * 100.0);
            log::info!("🧹 Cleaned preview: {}", &cleaned[..std::cmp::min(400, cleaned.len())]);
            
            cleaned
        } else {
            debug!("🧹 === HTML CLEANING FOR RAG ===");
            debug!("🧹 Cleaning HTML content for RAG processing...");
            trace!("🔍 HTML cleaning for RAG: original_length={}, stream_uuid={}", file_content.len(), stream_uuid);
            
            let cleaned = clean_html(&file_content);
            
            debug!("✅ HTML content cleaned for RAG: {} characters", cleaned.len());
            debug!("🧹 Content preview: {}", &cleaned[..std::cmp::min(200, cleaned.len())]);
            debug!("🧹 Cleaning ratio: {:.2}%", (cleaned.len() as f64 / file_content.len() as f64) * 100.0);
            trace!("🔍 HTML cleaning for RAG completed: original_length={}, cleaned_length={}, stream_uuid={}", 
                   file_content.len(), cleaned.len(), stream_uuid);
            
            // Enhanced logging for HTML cleaning
            log::info!("🧹 === HTML CLEANING COMPLETED ===");
            log::info!("🧹 Original size: {} characters", file_content.len());
            log::info!("🧹 Cleaned size: {} characters", cleaned.len());
            log::info!("🧹 Reduction: {:.2}%", (cleaned.len() as f64 / file_content.len() as f64) * 100.0);
            log::info!("🧹 Cleaned preview: {}", &cleaned[..std::cmp::min(400, cleaned.len())]);
            log::info!("🧹 Word count: {} words", cleaned.split_whitespace().count());
            
            cleaned
        };
        
        // Verify we have actual content
        if cleaned_content.trim().is_empty() {
            error!("❌ === EMPTY CLEANED CONTENT ERROR ===");
            error!("❌ Cleaned content is empty");
            debug!("🔍 Cleaned content empty: original_length={}, cleaned_length={}", file_content.len(), cleaned_content.len());
            trace!("🔍 Cleaned content empty: stream_uuid={}", stream_uuid);
            return Err("File contains no readable content after cleaning".into());
        }
        
        let prompt = format!(
            "Please analyze and summarize this {} from {}:\n\n{}",
            if is_youtube { "YouTube video subtitle file" } else { "webpage HTML content" },
            url, cleaned_content
        );
        
        debug!("📝 === USER PROMPT CREATION FOR RAG ===");
        debug!("📝 Created user prompt with {} content: {} characters", 
               if is_youtube { "subtitle" } else { "HTML" }, prompt.len());
        debug!("📝 Prompt preview: {}", &prompt[..std::cmp::min(300, prompt.len())]);
        trace!("🔍 User prompt created: prompt_length={}, cleaned_content_length={}, content_type={}, stream_uuid={}", 
               prompt.len(), cleaned_content.len(), if is_youtube { "youtube" } else { "webpage" }, stream_uuid);
        
        (prompt, cleaned_content)
    } else {
        // For webpages or fallback, use the original content
        debug!("🔧 === DIRECT PROCESSING TRIGGERED ===");
        debug!("🔧 No file path provided, using direct content processing");
        debug!("🔧 Content length: {} characters", content.len());
        debug!("🔧 Is YouTube: {}", is_youtube);
        debug!("📄 === WEBPAGE CONTENT PROCESSING ===");
        debug!("📄 Using webpage content processing");
        trace!("🔍 Webpage content processing: content_length={}, stream_uuid={}", content.len(), stream_uuid);
        
        // For YouTube videos, don't truncate content since RAG will handle chunking
        // For webpages, apply reasonable limits to prevent context overflow
        let max_content_length = if is_youtube { 
            usize::MAX // No limit for YouTube videos - RAG will handle chunking
        } else { 
            20000 // Limit for webpages
        };
        
        debug!("📏 === CONTENT LENGTH CHECK ===");
        debug!("📏 Checking content length: {} characters", content.len());
        debug!("📏 Max content length: {} characters", max_content_length);
        debug!("📏 Content type: {}", if is_youtube { "YouTube" } else { "Webpage" });
        debug!("📏 Needs truncation: {}", content.len() > max_content_length);
        trace!("🔍 Content length check: content_length={}, max_length={}, content_type={}, stream_uuid={}", 
               content.len(), max_content_length, if is_youtube { "youtube" } else { "webpage" }, stream_uuid);
        
        let truncated_content = if content.len() > max_content_length {
            let truncated = format!("{} [Content truncated due to length]", &content[0..max_content_length]);
            debug!("📏 Content truncated: {} -> {} characters", content.len(), truncated.len());
            debug!("📏 Truncation reduction: {:.2}%", (truncated.len() as f64 / content.len() as f64) * 100.0);
            trace!("🔍 Content truncated: original_length={}, truncated_length={}, stream_uuid={}", 
                   content.len(), truncated.len(), stream_uuid);
            truncated
        } else {
            debug!("📏 Content length is within limits, no truncation needed");
            trace!("🔍 Content within limits: length={}, stream_uuid={}", content.len(), stream_uuid);
            content.to_string()
        };
        
        let prompt = format!(
            "Please summarize this {} from {}:\n\n{}",
            if is_youtube { "YouTube video transcript" } else { "webpage content" },
            url,
            truncated_content
        );
        
        debug!("📝 === USER PROMPT CREATION FOR WEBPAGE ===");
        debug!("📝 Created user prompt with webpage content: {} characters", prompt.len());
        debug!("📝 Prompt preview: {}", &prompt[..std::cmp::min(300, prompt.len())]);
        trace!("🔍 User prompt created: prompt_length={}, truncated_content_length={}, stream_uuid={}", 
               prompt.len(), truncated_content.len(), stream_uuid);
        
        (prompt, truncated_content)
    };
    
    debug!("📝 === PROMPT SUMMARY ===");
    debug!("📝 System prompt length: {} characters", system_prompt.len());
    debug!("📝 User prompt length: {} characters", user_prompt.len());
    debug!("📝 Content to process length: {} characters", content_to_process.len());
    debug!("📝 Total prompt length: {} characters", system_prompt.len() + user_prompt.len());
    trace!("🔍 Prompt details: system_length={}, user_length={}, content_length={}, url_length={}, stream_uuid={}", 
           system_prompt.len(), user_prompt.len(), content_to_process.len(), url.len(), stream_uuid);
    
    // Use model context limit of 32,000 tokens, with safety margin for prompts
    // Assuming ~4 characters per token, use ~24,000 characters per chunk to leave room for prompts
    let chunk_size = if is_youtube { 24000 } else { 16000 }; // Optimized for 32K context limit
    let mut chunk_summaries = Vec::new();
    let request_payload;
    
            debug!("📄 === CHUNKING DECISION ===");
        debug!("📄 Content length: {} characters", content_to_process.len());
        debug!("📄 Chunk size: {} characters (optimized for 32K context)", chunk_size);
        debug!("📄 Model context limit: 32,000 tokens");
        debug!("📄 Max tokens per response: {}", config.default_max_tokens);
        debug!("📄 Needs chunking: {}", content_to_process.len() > chunk_size);
    trace!("🔍 Chunking decision: content_length={}, chunk_size={}, needs_chunking={}, stream_uuid={}", 
           content_to_process.len(), chunk_size, content_to_process.len() > chunk_size, stream_uuid);
    
    if content_to_process.len() > chunk_size {
        info!("📄 === RAG SUMMARIZATION STARTED ===");
        info!("📄 Content too long ({} chars), using map-reduce RAG summarization", content_to_process.len());
        debug!("📄 Starting RAG summarization with chunking...");
        trace!("🔍 RAG summarization started: content_length={}, chunk_size={}, stream_uuid={}", 
               content_to_process.len(), chunk_size, stream_uuid);
        
        // Enhanced logging for chunking process
        log::info!("📄 === CHUNKING PROCESS STARTED ===");
        log::info!("📄 Content length: {} characters", content_to_process.len());
        log::info!("📄 Chunk size: {} characters", chunk_size);
        log::info!("📄 Estimated chunks: {}", (content_to_process.len() + chunk_size - 1) / chunk_size);
        log::info!("📄 Content type: {}", if is_youtube { "YouTube" } else { "Webpage" });
        log::info!("📄 Processing method: Map-reduce RAG summarization");
        
        // FIXED: Proper character-based chunking of the actual content
        debug!("📄 === CONTENT CHUNKING ===");
        debug!("📄 Splitting content into chunks using character-based splitting...");
        
        // Use character-based splitting to avoid breaking UTF-8 characters
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        let words: Vec<&str> = content_to_process.split_whitespace().collect();
        
        // Safety check for extremely long content
        if words.len() > 100000 {
            warn!("⚠️ === EXTREMELY LONG CONTENT WARNING ===");
            warn!("⚠️ Content has {} words, this may cause performance issues", words.len());
        }
        
        for word in words {
            // Check if a single word is too long (might be corrupted data)
            if word.len() > chunk_size / 2 {
                warn!("⚠️ Skipping extremely long word: {} characters", word.len());
                continue;
            }
            
            if current_chunk.len() + word.len() + 1 > chunk_size && !current_chunk.is_empty() {
                chunks.push(current_chunk.trim().to_string());
                current_chunk = String::new();
            }
            
            if !current_chunk.is_empty() {
                current_chunk.push(' ');
            }
            current_chunk.push_str(word);
        }
        
        // Add the last chunk if it's not empty
        if !current_chunk.is_empty() {
            chunks.push(current_chunk.trim().to_string());
        }
        
        // Safety check for too many chunks
        if chunks.len() > 50 {
            warn!("⚠️ === TOO MANY CHUNKS WARNING ===");
            warn!("⚠️ Content split into {} chunks, this may take a very long time", chunks.len());
        }
        
        debug!("📄 Split content into {} chunks", chunks.len());
        debug!("📄 Chunk sizes: {:?}", chunks.iter().map(|c| c.len()).collect::<Vec<_>>());
        trace!("🔍 Content chunked: total_chunks={}, stream_uuid={}", chunks.len(), stream_uuid);
        
        // For very long videos, implement hierarchical summarization
        let is_very_long_video = is_youtube && chunks.len() > 10;
        if is_very_long_video {
            info!("📄 === VERY LONG VIDEO DETECTED ===");
            info!("📄 Video has {} chunks, using hierarchical summarization", chunks.len());
            debug!("📄 Implementing hierarchical summarization for very long video");
        }
        
        for (i, chunk) in chunks.iter().enumerate() {
            info!("🤖 === CHUNK {} PROCESSING ===", i+1);
            info!("🤖 Summarizing chunk {} of {} ({} chars)", i+1, chunks.len(), chunk.len());
            debug!("🤖 Chunk {} preview: {}", i+1, &chunk[..std::cmp::min(100, chunk.len())]);
            trace!("🔍 Chunk {} processing: chunk_length={}, stream_uuid={}", i+1, chunk.len(), stream_uuid);
            
            // FIXED: Create a more specific prompt for each chunk with actual content
            let chunk_prompt = format!(
                "Create a detailed summary of this content chunk from {}. Focus on key points, topics, and important information:\n\n{}",
                if is_youtube { "a YouTube video" } else { "a webpage" },
                chunk
            );
            
            debug!("📝 === CHUNK PROMPT CREATION ===");
            debug!("📝 Created chunk prompt: {} characters", chunk_prompt.len());
            debug!("📝 Chunk prompt preview: {}", &chunk_prompt[..std::cmp::min(200, chunk_prompt.len())]);
            trace!("🔍 Chunk prompt created: chunk={}, prompt_length={}, stream_uuid={}", 
                   i+1, chunk_prompt.len(), stream_uuid);
            
            let chunk_messages = vec![
                ChatMessage { 
                    role: "system".to_string(), 
                    content: "You are an expert content summarizer. Create comprehensive summaries that capture all important details, key points, and main topics from the provided content. Aim for summaries that are informative and detailed while remaining concise.".to_string() 
                },
                ChatMessage { 
                    role: "user".to_string(), 
                    content: chunk_prompt.clone() 
                }
            ];
            
            debug!("🤖 === CHUNK LLM REQUEST ===");
            debug!("🤖 Sending chunk {} to LLM with {} characters", i+1, chunk.len());
            debug!("🤖 Using selected model: {}", selected_model);
            debug!("🤖 Max tokens: 2000 (optimized for 32K context)");
            trace!("🔍 Chunk {} LLM request: chunk_length={}, prompt_length={}, model={}, stream_uuid={}", 
                   i+1, chunk.len(), chunk_prompt.len(), selected_model, stream_uuid);
            
            // Use selected model with retry logic for chunk summaries
            let chunk_summary = match chat_completion(chunk_messages, selected_model, config, Some(2000)).await {
                Ok(summary) => {
                    debug!("✅ Chunk {} summary received: {} characters", i+1, summary.len());
                    debug!("📝 Chunk {} summary preview: {}", i+1, &summary[..std::cmp::min(200, summary.len())]);
                    trace!("🔍 Chunk {} summary completed: summary_length={}, stream_uuid={}", 
                           i+1, summary.len(), stream_uuid);
                    summary
                },
                Err(e) => {
                    error!("❌ === CHUNK {} LLM ERROR ===", i+1);
                    error!("❌ Failed to get summary for chunk {}: {}", i+1, e);
                    debug!("🔍 Chunk {} LLM error: {:?}", i+1, e);
                    debug!("🔍 Chunk {} LLM error type: {:?}", i+1, std::any::type_name_of_val(&e));
                    trace!("🔍 Chunk {} LLM error: error_type={}, stream_uuid={}", 
                           i+1, std::any::type_name_of_val(&e), stream_uuid);
                    
                    // Enhanced error handling with user-friendly messages
                    let error_msg = format!("{}", e);
                    if error_msg.contains("Connection refused") || error_msg.contains("Connection Error") {
                        return Err(format!(
                            "❌ **LM Studio Connection Lost During Chunk Processing**\n\n\
                            Failed to process chunk {} of {}\n\n\
                            **Solutions:**\n\
                            • **Check LM Studio**: Ensure LM Studio is still running\n\
                            • **Model Loaded**: Verify model `{}` is still loaded\n\
                            • **Server Status**: Check if LM Studio server is still active\n\
                            • **Try Shorter Content**: Consider using shorter videos/webpages\n\n\
                            **Progress:** Successfully processed {} of {} chunks before failure",
                            i+1, chunks.len(), selected_model, i, chunks.len()
                        ).into());
                    } else if error_msg.contains("Windows Permission Error") || error_msg.contains("os error 10013") {
                        return Err(format!(
                            "❌ **Windows Permission Error During Processing**\n\n\
                            Network access was denied while processing chunk {} of {}\n\n\
                            **Quick Fix:**\n\
                            • **Restart as Administrator**: Close the bot and run as administrator\n\n\
                            **Progress:** Successfully processed {} of {} chunks before failure",
                            i+1, chunks.len(), i, chunks.len()
                        ).into());
                    } else {
                        return Err(format!(
                            "❌ **Chunk Processing Error**\n\n\
                            Failed to process chunk {} of {}\n\n\
                            **Error:** {}\n\n\
                            **Progress:** Successfully processed {} of {} chunks\n\
                            **Suggestion:** Try using shorter content or a different model",
                            i+1, chunks.len(), e, i, chunks.len()
                        ).into());
                    }
                }
            };
            
            // Check if the model returned a fallback message
            debug!("🔍 === CHUNK SUMMARY VALIDATION ===");
            debug!("🔍 Checking chunk {} summary for fallback messages...", i+1);
            debug!("🔍 Contains 'Search functionality is not available': {}", chunk_summary.contains("Search functionality is not available"));
            debug!("🔍 Contains 'fallback': {}", chunk_summary.contains("fallback"));
            
            if chunk_summary.contains("Search functionality is not available") || chunk_summary.contains("fallback") {
                warn!("⚠️ === CHUNK {} FALLBACK DETECTED ===", i+1);
                warn!("⚠️ Model {} appears to be a search model, not suitable for summarization", selected_model);
                debug!("🔍 Chunk {} returned search model response: {}", i+1, chunk_summary);
                debug!("🔍 Using direct content approach for this chunk");
                // Use a more direct approach for this chunk
                let direct_summary = format!("Content chunk {}: {}", i+1, chunk);
                                chunk_summaries.push(direct_summary.clone());
                trace!("🔍 Chunk {} fallback used: direct_summary_length={}, stream_uuid={}",        
                       i+1, direct_summary.len(), stream_uuid);
            } else {
                                chunk_summaries.push(chunk_summary.clone());
                trace!("🔍 Chunk {} summary added: summary_length={}, stream_uuid={}",
                       i+1, chunk_summary.len(), stream_uuid);
            }
        }
        
        // FIXED: Combine chunk summaries for final prompt with better structure
        debug!("📝 === CHUNK SUMMARIES COMBINATION ===");
        debug!("📝 Combining {} chunk summaries...", chunk_summaries.len());
        
        let combined = if is_very_long_video {
            // For very long videos, implement hierarchical summarization
            info!("📄 === HIERARCHICAL SUMMARIZATION FOR VERY LONG VIDEO ===");
            info!("📄 Processing {} chunk summaries hierarchically", chunk_summaries.len());
            
            // Group chunk summaries into sections (every 5 chunks)
            let mut section_summaries = Vec::new();
            let section_size = 5;
            
            for (section_idx, section_chunks) in chunk_summaries.chunks(section_size).enumerate() {
                info!("📄 === SECTION {} SUMMARIZATION ===", section_idx + 1);
                info!("📄 Summarizing section {} with {} chunks", section_idx + 1, section_chunks.len());
                
                let section_combined = section_chunks.join("\n\n---\n\n");
                debug!("📝 Section {} combined: {} characters", section_idx + 1, section_combined.len());
                
                let section_prompt = format!(
                    "Create a comprehensive summary of this section from a YouTube video. Focus on the main topics, key points, and important information:\n\n{}",
                    section_combined
                );
                
                let section_messages = vec![
                    ChatMessage { 
                        role: "system".to_string(), 
                        content: "You are an expert content summarizer. Create comprehensive section summaries that capture the main topics and key information.".to_string() 
                    },
                    ChatMessage { 
                        role: "user".to_string(), 
                        content: section_prompt 
                    }
                ];
                
                let section_summary = match chat_completion(section_messages, selected_model, config, Some(1500)).await {
                    Ok(summary) => {
                        info!("✅ Section {} summary completed: {} characters", section_idx + 1, summary.len());
                        summary
                    },
                    Err(e) => {
                        error!("❌ Failed to summarize section {}: {}", section_idx + 1, e);
                        
                        // Enhanced error handling for section processing
                        let error_msg = format!("{}", e);
                        if error_msg.contains("Connection refused") || error_msg.contains("Connection Error") {
                            return Err(format!(
                                "❌ **LM Studio Connection Lost During Section Processing**\n\n\
                                Failed to process section {} during hierarchical summarization\n\n\
                                **Solutions:**\n\
                                • **Check LM Studio**: Ensure LM Studio is still running\n\
                                • **Model Status**: Verify model `{}` is still loaded\n\
                                • **Try Again**: Restart the command\n\n\
                                **Progress:** Individual chunks completed, failed during section consolidation",
                                section_idx + 1, selected_model
                            ).into());
                        } else {
                            return Err(format!(
                                "❌ **Section Processing Error**\n\n\
                                Failed during hierarchical summarization of section {}\n\n\
                                **Error:** {}\n\n\
                                **Note:** Individual chunks were processed successfully",
                                section_idx + 1, e
                            ).into());
                        }
                    }
                };
                
                section_summaries.push(format!("Section {}: {}", section_idx + 1, section_summary));
            }
            
            // Combine section summaries for final summary
            let final_sections = section_summaries.join("\n\n---\n\n");
            debug!("📝 Final sections combined: {} characters", final_sections.len());
            final_sections
        } else {
            // For normal videos, use direct combination
            let combined = chunk_summaries.join("\n\n---\n\n");
            debug!("📝 Combined chunk summaries: {} characters", combined.len());
            debug!("📝 Combined summaries preview: {}", &combined[..std::cmp::min(300, combined.len())]);
            trace!("🔍 Chunk summaries combined: combined_length={}, chunk_count={}, stream_uuid={}", 
                   combined.len(), chunk_summaries.len(), stream_uuid);
            combined
        };
        
        // Limit final prompt size to prevent context overflow
        let max_final_prompt_size = 80000; // ~20K tokens
        let final_content = if combined.len() > max_final_prompt_size {
            warn!("⚠️ === FINAL PROMPT TOO LARGE ===");
            warn!("⚠️ Combined content too large ({} chars), truncating to {} chars", combined.len(), max_final_prompt_size);
            format!("{} [Content truncated due to size - showing first {} characters]", 
                    &combined[0..max_final_prompt_size], max_final_prompt_size)
        } else {
            combined
        };
        
        let final_user_prompt = format!(
            "Create a comprehensive, well-structured summary of this {} from {}. Use the following detailed chunk summaries to build a complete overview that covers all major topics, key points, and important information:\n\n{}\n\nPlease organize the summary with clear sections and highlight the most important takeaways.",
            if is_youtube { "YouTube video" } else { "webpage" },
            url, final_content
        );
        
        debug!("📝 === FINAL RAG PROMPT CREATION ===");
        debug!("📝 Created final RAG prompt: {} characters", final_user_prompt.len());
        debug!("📝 Final prompt preview: {}", &final_user_prompt[..std::cmp::min(300, final_user_prompt.len())]);
        trace!("🔍 Final RAG prompt created: final_prompt_length={}, stream_uuid={}", final_user_prompt.len(), stream_uuid);
        
        let final_messages = vec![
            ChatMessage { role: "system".to_string(), content: system_prompt.clone() },
            ChatMessage { role: "user".to_string(), content: final_user_prompt.clone() }
        ];
        
        debug!("📝 Final RAG prompt created: {} characters", final_user_prompt.len());
        debug!("📝 Final message count: {}", final_messages.len());
        trace!("🔍 Final RAG prompt created: final_prompt_length={}, message_count={}, stream_uuid={}", 
               final_user_prompt.len(), final_messages.len(), stream_uuid);
        
        request_payload = serde_json::json!(
            {
                "model": selected_model,
                "messages": final_messages,
                "temperature": config.default_temperature,
                "max_tokens": config.default_max_tokens,
                "stream": true
            }
        );
    } else {
        info!("📄 === DIRECT SUMMARIZATION ===");
        info!("📄 Content length ({}) is within limits, using direct summarization", content_to_process.len());
        debug!("📄 Using direct summarization approach for {}", if is_youtube { "YouTube" } else { "webpage" });
        trace!("🔍 Direct summarization: content_length={}, chunk_size={}, content_type={}, stream_uuid={}", 
               content_to_process.len(), chunk_size, if is_youtube { "youtube" } else { "webpage" }, stream_uuid);
        
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt,
            },
        ];
        
        debug!("📝 === DIRECT PROMPT CREATION ===");
        debug!("📝 Created direct summarization messages for {}: {} messages", if is_youtube { "YouTube" } else { "webpage" }, messages.len());
        debug!("📝 System message length: {} characters", messages[0].content.len());
        debug!("📝 User message length: {} characters", messages[1].content.len());
        debug!("📝 Total message length: {} characters", messages[0].content.len() + messages[1].content.len());
        trace!("🔍 Direct summarization: message_count={}, system_length={}, user_length={}, content_type={}, stream_uuid={}", 
               messages.len(), messages[0].content.len(), messages[1].content.len(), if is_youtube { "youtube" } else { "webpage" }, stream_uuid);
        
        request_payload = serde_json::json!(
            {
                "model": selected_model,
                "messages": messages,
                "temperature": config.default_temperature,
                "max_tokens": config.default_max_tokens,
                "stream": true
            }
        );
    }
    
    // Use shared HTTP client with optimal settings
    debug!("🔧 === HTTP CLIENT SETUP ===");
    debug!("🔧 Using shared HTTP client for streaming request...");
    trace!("🔍 HTTP client setup started: stream_uuid={}", stream_uuid);
    
    // Calculate appropriate timeout for content complexity
    let timeout_seconds = 300; // 5 minutes for all content types
    
    let client = get_http_client().await;
    
    debug!("✅ Shared HTTP client obtained successfully");
    debug!("🔧 Using optimized timeout: {} seconds", timeout_seconds);
    debug!("🔧 Content length: {} characters", content_to_process.len());
    debug!("🔧 Video type: {}", if is_youtube { "YouTube" } else { "Webpage" });
    debug!("🔧 Connection pooling and SSL bypass enabled");
    trace!("🔍 HTTP client obtained: timeout={}s, content_length={}, stream_uuid={}", timeout_seconds, content_to_process.len(), stream_uuid);
    
    // Send streaming request
    let api_url = format!("{}/v1/chat/completions", config.base_url);
    let payload_size = serde_json::to_string(&request_payload).unwrap_or_default().len();
    
    debug!("🚀 === STREAMING REQUEST PREPARATION ===");
    debug!("🚀 API URL: {}", api_url);
    debug!("🚀 Payload size: {} bytes", payload_size);
    debug!("🚀 Model: {}", selected_model);
    debug!("🚀 Temperature: {}", config.default_temperature);
    debug!("🚀 Max tokens: {}", config.default_max_tokens);
    debug!("🚀 Streaming: true");
    trace!("🔍 API request preparation: url={}, payload_size={}, stream_uuid={}", api_url, payload_size, stream_uuid);
    
    debug!("🚀 Sending streaming request to LLM...");
    trace!("🔍 Streaming request started: stream_uuid={}", stream_uuid);
    
    let mut response = match client
        .post(&api_url)
        .json(&request_payload)
        .timeout(Duration::from_secs(timeout_seconds))
        .send()
        .await {
            Ok(resp) => resp,
            Err(e) => {
                error!("❌ === HTTP REQUEST ERROR ===");
                error!("❌ Failed to send HTTP request: {}", e);
                
                let error_msg = format!("{}", e);
                let error_message = if e.is_timeout() {
                    format!("⏰ **Request Timeout**\n\nThe request to LM Studio timed out after {} seconds.\n\n**Solutions:**\n• **Reduce Content**: Try a shorter video/webpage\n• **Check LM Studio**: Ensure LM Studio is running and responsive\n• **Model Performance**: Consider using a faster model\n• **System Resources**: Check if your system has enough RAM/CPU\n\n**Current Setup:**\n• LM Studio URL: `{}`\n• Model: `{}`\n• Content Length: {} characters\n\n*Source: <{}>*", 
                            timeout_seconds, config.base_url, selected_model, content_to_process.len(), url)
                } else if e.is_connect() {
                                          format!("🚫 **Connection Error**\n\nCannot connect to LM Studio at `{}`.\n\n**Solutions:**\n• **Start LM Studio**: Ensure LM Studio is running\n• **Load Model**: Load model `{}` in LM Studio\n• **Enable Server**: Click 'Start Server' in LM Studio\n• **Check Configuration**: Verify URL in lmapiconf.txt\n• **Firewall**: Check Windows Defender/firewall settings\n• **Try localhost**: Use `http://127.0.0.1:1234` instead of localhost\n\n*Source: <{}>*", 
                            config.base_url, selected_model, url)
                } else if error_msg.contains("os error 10013") {
                    format!("🚫 **Windows Permission Error (10013)**\n\nNetwork access denied by Windows.\n\n**Quick Fixes:**\n• **Run as Administrator**: Right-click and 'Run as administrator'\n• **Windows Firewall**: Add firewall exception for this program\n• **Use IP Address**: Try `http://127.0.0.1:1234` in lmapiconf.txt\n\n**Current URL:** `{}`\n*Source: <{}>*", 
                            config.base_url, url)
                } else {
                    format!("🔗 **Network Error**\n\nUnexpected network error occurred.\n\n**Error:** {}\n\n**Solutions:**\n• **Check Connection**: Verify your internet connection\n• **Restart LM Studio**: Try restarting LM Studio\n• **Check Configuration**: Verify settings in lmapiconf.txt\n• **Try Again**: Network issues are often temporary\n\n**Current Setup:**\n• LM Studio URL: `{}`\n• Model: `{}`\n\n*Source: <{}>*", 
                            e, config.base_url, selected_model, url)
                };
                
                msg.edit(ctx, |m| m.content(&error_message)).await?;
                return Ok(());
            }
        };
    
    debug!("📡 === STREAMING RESPONSE RECEIVED ===");
    debug!("📡 HTTP Response Status: {}", response.status());
    debug!("📡 HTTP Response Status Code: {}", response.status().as_u16());
    debug!("📡 HTTP Response Success: {}", response.status().is_success());
    trace!("🔍 Streaming response received: status={}, success={}, stream_uuid={}", 
           response.status(), response.status().is_success(), stream_uuid);
    
    if !response.status().is_success() {
        error!("❌ === STREAMING API ERROR ===");
        error!("❌ API request failed: HTTP {}", response.status());
        debug!("🔍 API error details: status_code={}, status_text={}", response.status().as_u16(), response.status().as_str());
        trace!("🔍 API request failed: status={}, status_code={}, stream_uuid={}", 
               response.status(), response.status().as_u16(), stream_uuid);
        return Err(format!("API returned error: {}", response.status()).into());
    }
    
    debug!("✅ API request successful: HTTP {}", response.status());
    trace!("🔍 API request successful: status={}, stream_uuid={}", response.status(), stream_uuid);
    
    debug!("📡 === STREAMING PROCESSING ===");
    debug!("📡 Starting to process streaming response...");
    trace!("🔍 Streaming processing started: stream_uuid={}", stream_uuid);
    
    let mut accumulated = String::new();
    let start_time = Instant::now();
    let mut last_update = Instant::now();
    let mut chunk_count = 0;
    
    debug!("📊 === STREAMING STATISTICS INITIALIZATION ===");
    debug!("📊 Start time: {:?}", start_time);
    debug!("📊 Last update time: {:?}", last_update);
    debug!("📊 Initial chunk count: {}", chunk_count);
    trace!("🔍 Streaming started: start_time={:?}, stream_uuid={}", start_time, stream_uuid);
    
    while let Some(chunk) = match response.chunk().await {
        Ok(chunk_opt) => chunk_opt,
        Err(e) => {
            error!("❌ === STREAMING CHUNK ERROR ===");
            error!("❌ Failed to read streaming chunk: {}", e);
            
            let error_message = format!(
                "❌ **Streaming Error**\n\nFailed to read streaming response: {}\n\n**Solutions:**\n• Try again with a shorter video/webpage\n• Check your internet connection\n• Verify AI model is stable\n\n*Source: <{}>*", 
                e, url
            );
            
            msg.edit(ctx, |m| m.content(&error_message)).await?;
            return Ok(());
        }
    } {
        chunk_count += 1;
        debug!("📡 === CHUNK {} RECEIVED ===", chunk_count);
        debug!("📡 Received chunk {}: {} bytes", chunk_count, chunk.len());
        trace!("🔍 Received chunk {}: size={} bytes, stream_uuid={}", chunk_count, chunk.len(), stream_uuid);
        
        let chunk_str = String::from_utf8_lossy(&chunk);
        debug!("📡 Chunk {} as string: {} characters", chunk_count, chunk_str.len());
        debug!("📡 Chunk {} preview: {}", chunk_count, &chunk_str[..std::cmp::min(100, chunk_str.len())]);
        
        for (line_num, line) in chunk_str.lines().enumerate() {
            debug!("📝 === LINE {} PROCESSING ===", line_num + 1);
            debug!("📝 Processing line: '{}'", line);
            
            if line.starts_with("data: ") {
                let data = &line[6..];
                debug!("📝 Found data line: {} characters", data.len());
                debug!("📝 Data content: '{}'", data);
                
                if data == "[DONE]" {
                    debug!("✅ === STREAM COMPLETION ===");
                    debug!("✅ Received [DONE] signal, ending stream");
                    trace!("🔍 Received [DONE] signal, ending stream: stream_uuid={}", stream_uuid);
                    break;
                }
                
                match serde_json::from_str::<StreamResponse>(data) {
                    Ok(stream_resp) => {
                        debug!("✅ === STREAM RESPONSE PARSED ===");
                        debug!("✅ Successfully parsed stream response");
                        debug!("✅ Choices count: {}", stream_resp.choices.len());
                        
                        if let Some(choice) = stream_resp.choices.get(0) {
                            if let Some(content) = &choice.delta.content {
                                debug!("📝 === CONTENT CHUNK ADDED ===");
                                debug!("📝 Adding content chunk: {} characters", content.len());
                                debug!("📝 Content chunk: '{}'", content);
                                accumulated.push_str(content);
                                debug!("📝 Total accumulated: {} characters", accumulated.len());
                                trace!("🔍 Added content chunk: length={}, total_accumulated={}, stream_uuid={}", 
                                       content.len(), accumulated.len(), stream_uuid);
                            }
                            if choice.finish_reason.is_some() {
                                debug!("✅ === STREAM FINISHED ===");
                                debug!("✅ Received finish_reason: {:?}", choice.finish_reason);
                                trace!("🔍 Received finish_reason: {:?}, ending stream: stream_uuid={}", 
                                       choice.finish_reason, stream_uuid);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        debug!("❌ === STREAM PARSE ERROR ===");
                        debug!("❌ Failed to parse stream response: {}", e);
                        debug!("❌ Raw data: '{}'", data);
                        trace!("🔍 Failed to parse stream response: error={}, data={}, stream_uuid={}", 
                               e, data, stream_uuid);
                    }
                }
            } else {
                debug!("📝 Line does not start with 'data: ', skipping");
                trace!("🔍 Skipped non-data line: line={}, stream_uuid={}", line, stream_uuid);
            }
        }
        
        // Periodic update to Discord every 5 seconds
        if last_update.elapsed() > Duration::from_secs(5) {
            let elapsed = start_time.elapsed().as_secs();
            debug!("⏰ === PERIODIC DISCORD UPDATE ===");
            debug!("⏰ Periodic Discord update: {} seconds elapsed", elapsed);
            debug!("⏰ Accumulated content: {} characters", accumulated.len());
            trace!("🔍 Periodic Discord update: elapsed_seconds={}, accumulated_length={}, stream_uuid={}", 
                   elapsed, accumulated.len(), stream_uuid);
            
            msg.edit(ctx, |m| m.content(format!("🤖 Generating summary... ({}s)", elapsed))).await?;
            last_update = Instant::now();
            debug!("✅ Discord message updated successfully");
        }
    }
    
    debug!("📊 === STREAMING COMPLETED ===");
    debug!("📊 Total chunks received: {}", chunk_count);
    debug!("📊 Total streaming time: {:.2}s", start_time.elapsed().as_secs_f64());
    debug!("📊 Final accumulated content: {} characters", accumulated.len());
    trace!("🔍 Streaming completed: chunk_count={}, total_time={:.2}s, final_length={}, stream_uuid={}", 
           chunk_count, start_time.elapsed().as_secs_f64(), accumulated.len(), stream_uuid);
    
    // Strip <think> sections from full accumulated response (normal for reasoning models)
    debug!("🧹 === THINKING TAG REMOVAL ===");
    debug!("🧹 Removing <think> tags from accumulated content...");
    let before_stripping = accumulated.len();
    
    let re = Regex::new(r"(?s)<think>.*?</think>").unwrap();
    let stripped = re.replace_all(&accumulated, "").to_string();
    
    debug!("✅ === THINKING TAG REMOVAL COMPLETED ===");
    debug!("✅ Thinking tag removal completed");
    debug!("📊 Before stripping: {} characters", before_stripping);
    debug!("📊 After stripping: {} characters", stripped.len());
    debug!("📊 Stripping reduction: {:.2}%", (stripped.len() as f64 / before_stripping as f64) * 100.0);
    debug!("📊 Content preview: {}", &stripped[..std::cmp::min(300, stripped.len())]);
    trace!("🔍 Content processing: original_accumulated={}, stripped_length={}, chunk_count={}, stream_uuid={}", 
           accumulated.len(), stripped.len(), chunk_count, stream_uuid);
    
    // Check if the model returned a fallback message
    debug!("🔍 === FALLBACK MESSAGE CHECK ===");
    debug!("🔍 Checking for fallback messages in final response...");
    debug!("🔍 Contains 'Search functionality is not available': {}", stripped.contains("Search functionality is not available"));
    debug!("🔍 Contains 'fallback': {}", stripped.contains("fallback"));
    
    if stripped.contains("Search functionality is not available") || stripped.contains("fallback") {
        warn!("⚠️ === FALLBACK MESSAGE DETECTED ===");
        warn!("⚠️ Model {} returned fallback message, indicating it's not suitable for summarization", selected_model);
        debug!("🔍 Final response contains fallback message: {}", stripped);
        trace!("🔍 Fallback message detected: model={}, stream_uuid={}", selected_model, stream_uuid);
        
        // Provide a user-friendly error message
        let error_message = format!(
            "❌ **Summarization Failed**\n\n**Issue:** The AI model `{}` appears to be unsuitable for content summarization.\n\n**Solution:** This may indicate:\n• **Model Type**: Model may need different prompting strategies\n• **Content Complexity**: Try shorter content or different approach\n• **Model Configuration**: Verify model is properly loaded in LM Studio\n\n**Alternative:** Try using a different model configured for summarization.\n\n*Source: <{}>*",
            selected_model, url
        );
        
        debug!("📝 === FALLBACK ERROR MESSAGE ===");
        debug!("📝 Sending fallback error message to Discord...");
        msg.edit(ctx, |m| m.content(&error_message)).await?;
        debug!("✅ Fallback error message sent successfully");
        trace!("🔍 Fallback error message sent: stream_uuid={}", stream_uuid);
        return Ok(());
    }
    
    // Check if we got meaningful content
    debug!("🔍 === CONTENT VALIDATION ===");
    debug!("🔍 Validating final content...");
    debug!("🔍 Content length: {} characters", stripped.len());
    debug!("🔍 Content is empty: {}", stripped.trim().is_empty());
    debug!("🔍 Content is too short: {}", stripped.len() < 50);
    
    if stripped.trim().is_empty() || stripped.len() < 50 {
        error!("❌ === INSUFFICIENT CONTENT ERROR ===");
        error!("❌ LLM returned insufficient content: {} characters", stripped.len());
        debug!("🔍 Insufficient content: length={}, content='{}'", stripped.len(), stripped);
        trace!("🔍 Insufficient content: length={}, stream_uuid={}", stripped.len(), stream_uuid);
        
        let error_message = format!(
            "❌ **Summarization Failed**\n\n**Issue:** The AI model returned insufficient content ({} characters).\n\n**Possible causes:**\n• Model is not properly configured for summarization\n• Content was too long or complex\n• API connection issues\n\n*Source: <{}>*",
            stripped.len(), url
        );
        
        debug!("📝 === INSUFFICIENT CONTENT ERROR MESSAGE ===");
        debug!("📝 Sending insufficient content error message to Discord...");
        msg.edit(ctx, |m| m.content(&error_message)).await?;
        debug!("✅ Insufficient content error message sent successfully");
        trace!("🔍 Insufficient content error message sent: stream_uuid={}", stream_uuid);
        return Ok(());
    }
    
    // Final update
    debug!("📝 === FINAL MESSAGE CREATION ===");
    debug!("📝 Creating final Discord message...");
    
    let final_message = format!(
        "**{} Summary**\n\n{}\n\n*Source: <{}>*",
        if is_youtube { "YouTube Video" } else { "Webpage" },
        stripped.trim(),
        url
    );
    
    debug!("📝 Final message created: {} characters", final_message.len());
    debug!("📝 Final message preview: {}", &final_message[..std::cmp::min(300, final_message.len())]);
    trace!("🔍 Final message created: length={}, is_youtube={}, stream_uuid={}", 
           final_message.len(), is_youtube, stream_uuid);
    
    // Split if too long
    let max_length = config.max_discord_message_length - config.response_format_padding;
    debug!("📏 === MESSAGE LENGTH CHECK ===");
    debug!("📏 Final message length: {} characters", final_message.len());
    debug!("📏 Max Discord message length: {}", config.max_discord_message_length);
    debug!("📏 Response format padding: {}", config.response_format_padding);
    debug!("📏 Effective max length: {} characters", max_length);
    debug!("📏 Needs splitting: {}", final_message.len() > max_length);
    trace!("🔍 Message length check: final_length={}, max_length={}, needs_splitting={}, stream_uuid={}", 
           final_message.len(), max_length, final_message.len() > max_length, stream_uuid);
    
    if final_message.len() > max_length {
        info!("📄 === MESSAGE SPLITTING ===");
        info!("📄 Message too long, splitting into chunks...");
        debug!("📄 Original message length: {} characters", final_message.len());
        debug!("📄 Max chunk length: {} characters", max_length);
        trace!("🔍 Message splitting started: original_length={}, max_chunk_length={}, stream_uuid={}", 
               final_message.len(), max_length, stream_uuid);
        
        let chunks = split_message(&final_message, max_length);
        debug!("📄 Split into {} chunks", chunks.len());
        debug!("📄 Chunk sizes: {:?}", chunks.iter().map(|c| c.len()).collect::<Vec<_>>());
        trace!("🔍 Message split completed: chunk_count={}, stream_uuid={}", chunks.len(), stream_uuid);
        
        for (i, chunk) in chunks.iter().enumerate() {
            debug!("📤 === SENDING CHUNK {} ===", i+1);
            debug!("📤 Sending chunk {}: {} characters", i+1, chunk.len());
            trace!("🔍 Sending chunk {}: length={}, stream_uuid={}", i+1, chunk.len(), stream_uuid);
            
            if i == 0 {
                debug!("📤 Sending first chunk via edit");
                msg.edit(ctx, |m| m.content(chunk)).await?;
                trace!("🔍 First chunk sent via edit: stream_uuid={}", stream_uuid);
            } else {
                debug!("📤 Sending additional chunk {} via new message", i+1);
                msg.channel_id.say(ctx, chunk).await?;
                trace!("🔍 Additional chunk {} sent via new message: stream_uuid={}", i+1, stream_uuid);
            }
            debug!("✅ Chunk {} sent successfully", i+1);
        }
    } else {
        debug!("📤 === SENDING SINGLE MESSAGE ===");
        debug!("📤 Sending single message: {} characters", final_message.len());
        trace!("🔍 Sending single message: length={}, stream_uuid={}", final_message.len(), stream_uuid);
        
        msg.edit(ctx, |m| m.content(&final_message)).await?;
        debug!("✅ Single message sent successfully");
        trace!("🔍 Single message sent successfully: stream_uuid={}", stream_uuid);
    }
    
    info!("✅ === AI SUMMARIZATION STREAMING COMPLETED ===");
    info!("✅ Stream summary completed successfully");
    debug!("📊 Final statistics:");
    debug!("📊   - Stream UUID: {}", stream_uuid);
    debug!("📊   - Total chunks received: {}", chunk_count);
    debug!("📊   - Total streaming time: {:.2}s", start_time.elapsed().as_secs_f64());
    debug!("📊   - Final content length: {} characters", stripped.len());
    debug!("📊   - Final message length: {} characters", final_message.len());
    debug!("📊   - Content type: {}", if is_youtube { "YouTube" } else { "Webpage" });
    trace!("🔍 Stream summary completed successfully: stream_uuid={}", stream_uuid);
    
    Ok(())
}

// Split long messages into Discord-sized chunks
// Used to avoid exceeding Discord's message length limit
fn split_message(content: &str, max_len: usize) -> Vec<String> {
    let split_uuid = Uuid::new_v4();
    
    debug!("📄 === MESSAGE SPLITTING STARTED ===");
    debug!("🆔 Split UUID: {}", split_uuid);
    debug!("📄 Original content length: {} characters", content.len());
    debug!("📄 Max chunk length: {} characters", max_len);
    debug!("📄 Needs splitting: {}", content.len() > max_len);
    trace!("🔍 Message splitting started: content_length={}, max_len={}, split_uuid={}", 
           content.len(), max_len, split_uuid);
    
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut line_count = 0;
    let mut chunk_count = 0;
    
    debug!("📝 === LINE PROCESSING ===");
    debug!("📝 Processing content line by line...");
    
    for (line_num, line) in content.lines().enumerate() {
        line_count += 1;
        debug!("📝 === LINE {} PROCESSING ===", line_num + 1);
        debug!("📝 Line {} length: {} characters", line_num + 1, line.len());
        debug!("📝 Line {} content: '{}'", line_num + 1, line);
        debug!("📝 Current chunk length: {} characters", current.len());
        debug!("📝 Would exceed limit: {}", current.len() + line.len() + 1 > max_len);
        
        if current.len() + line.len() + 1 > max_len && !current.is_empty() {
            chunk_count += 1;
            debug!("📄 === CHUNK {} CREATED ===", chunk_count);
            debug!("📄 Creating chunk {}: {} characters", chunk_count, current.len());
            debug!("📄 Chunk {} content: '{}'", chunk_count, current.trim());
            chunks.push(current.trim().to_string());
            trace!("🔍 Chunk {} created: length={}, split_uuid={}", chunk_count, current.len(), split_uuid);
            current = String::new();
            debug!("📄 Reset current chunk for next content");
        }
        
        if !current.is_empty() {
            debug!("📝 Adding newline to current chunk");
            current.push('\n');
        }
        
        debug!("📝 Adding line {} to current chunk", line_num + 1);
        current.push_str(line);
        debug!("📝 Current chunk after adding line: {} characters", current.len());
        trace!("🔍 Line {} processed: line_length={}, current_chunk_length={}, split_uuid={}", 
               line_num + 1, line.len(), current.len(), split_uuid);
    }
    
    if !current.is_empty() {
        chunk_count += 1;
        debug!("📄 === FINAL CHUNK {} CREATED ===", chunk_count);
        debug!("📄 Creating final chunk {}: {} characters", chunk_count, current.len());
        debug!("📄 Final chunk content: '{}'", current.trim());
        chunks.push(current.trim().to_string());
        trace!("🔍 Final chunk {} created: length={}, split_uuid={}", chunk_count, current.len(), split_uuid);
    }
    
    debug!("✅ === MESSAGE SPLITTING COMPLETED ===");
    debug!("✅ Message splitting completed successfully");
    debug!("📊 Final statistics:");
    debug!("📊   - Split UUID: {}", split_uuid);
    debug!("📊   - Total lines processed: {}", line_count);
    debug!("📊   - Total chunks created: {}", chunks.len());
    debug!("📊   - Original content length: {} characters", content.len());
    debug!("📊   - Total chunked content length: {} characters", chunks.iter().map(|c| c.len()).sum::<usize>());
    debug!("📊   - Chunk sizes: {:?}", chunks.iter().map(|c| c.len()).collect::<Vec<_>>());
    debug!("📊   - Efficiency: {:.2}%", (chunks.iter().map(|c| c.len()).sum::<usize>() as f64 / content.len() as f64) * 100.0);
    
    trace!("🔍 Message splitting completed: line_count={}, chunk_count={}, original_length={}, total_chunked_length={}, split_uuid={}", 
           line_count, chunks.len(), content.len(), chunks.iter().map(|c| c.len()).sum::<usize>(), split_uuid);
    
    chunks
}

// ============================================================================
// COMMAND GROUP
// ============================================================================

// Commands are auto-registered by the #[command] macro

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_clean_vtt() {
        let vtt = r#"WEBVTT

00:00:00.000 --> 00:00:03.000
Hello world

00:00:03.000 --> 00:00:06.000
This is a test"#;
        
        let cleaned = clean_vtt_content(vtt);
        assert_eq!(cleaned, "Hello world This is a test");
    }
    
    #[test]
    fn test_clean_html() {
        let html = "<p>Hello <b>world</b></p><script>alert('test');</script>";
        let cleaned = clean_html(html);
        assert_eq!(cleaned, "Hello world");
    }
    
    #[test]
    fn test_webpage_content_processing() {
        // Test that web page content is processed correctly
        let test_content = "This is a test webpage content with some information about various topics. It contains multiple sentences and should be processed properly for summarization.";
        
        // Simulate the content processing logic
        let max_content_length = 20000;
        let truncated_content = if test_content.len() > max_content_length {
            format!("{} [Content truncated due to length]", &test_content[0..max_content_length])
        } else {
            test_content.to_string()
        };
        
        // Verify content is not truncated
        assert_eq!(truncated_content, test_content);
        
        // Test chunking logic
        let chunk_size = 8000;
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        
        for word in truncated_content.split_whitespace() {
            if current_chunk.len() + word.len() + 1 > chunk_size && !current_chunk.is_empty() {
                chunks.push(current_chunk.trim().to_string());
                current_chunk = String::new();
            }
            
            if !current_chunk.is_empty() {
                current_chunk.push(' ');
            }
            current_chunk.push_str(word);
        }
        
        if !current_chunk.is_empty() {
            chunks.push(current_chunk.trim().to_string());
        }
        
        // Verify chunking works correctly
        assert_eq!(chunks.len(), 1); // Should fit in one chunk
        assert_eq!(chunks[0], test_content);
    }
    
    #[test]
    fn test_html_file_processing() {
        // Test HTML cleaning functionality
        let test_html = r#"<html><head><title>Test Page</title></head><body><h1>Hello World</h1><p>This is a <b>test</b> paragraph.</p><script>alert('test');</script><style>body { color: red; }</style></body></html>"#;
        
        let cleaned = clean_html(test_html);
        
        // Should contain the text content but not HTML tags, scripts, or styles
        assert!(cleaned.contains("Hello World"));
        assert!(cleaned.contains("This is a test paragraph"));
        assert!(!cleaned.contains("<script>"));
        assert!(!cleaned.contains("<style>"));
        assert!(!cleaned.contains("<html>"));
        assert!(!cleaned.contains("<b>"));
    }
    
    #[test]
    fn test_lm_config_structure() {
        // Test that the LMConfig structure can be created and has all expected fields
        let config = LMConfig {
            base_url: "http://localhost:1234".to_string(),
            timeout: 60,
            default_model: "test-model".to_string(),
            default_reason_model: "test-reason-model".to_string(),
            default_summarization_model: "test-sum-model".to_string(),
            default_ranking_model: "test-rank-model".to_string(),
            default_temperature: 0.7,
            default_max_tokens: 2000,
            max_discord_message_length: 2000,
            response_format_padding: 100,
            default_vision_model: "test-vision-model".to_string(),
            default_seed: Some(42),
        };
        
        assert_eq!(config.base_url, "http://localhost:1234");
        assert_eq!(config.timeout, 60);
        assert_eq!(config.default_model, "test-model");
        assert_eq!(config.default_summarization_model, "test-sum-model");
        assert_eq!(config.default_temperature, 0.7);
        assert_eq!(config.default_max_tokens, 2000);
        assert_eq!(config.default_seed, Some(42));
    }
    
    #[test]
    fn test_chat_message_structure() {
        // Test that the ChatMessage structure can be created and serialized
        let message = ChatMessage {
            role: "user".to_string(),
            content: "Hello, world!".to_string(),
        };
        
        assert_eq!(message.role, "user");
        assert_eq!(message.content, "Hello, world!");
        
        // Test serialization
        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("user"));
        assert!(json.contains("Hello, world!"));
        
        // Test deserialization
        let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, "user");
        assert_eq!(deserialized.content, "Hello, world!");
    }
    
    #[test]
    fn test_split_message_functionality() {
        // Test the message splitting functionality with multi-line content
        // The split_message function splits by lines, not character count
        let lines = vec![
            "This is line 1 with some text that makes it moderately long to test splitting.",
            "This is line 2 with additional content that should also be quite lengthy.",
            "This is line 3 with even more text to ensure we exceed the character limit.",
            "This is line 4 which continues to add content for comprehensive testing.",
            "This is line 5 that should definitely push us over the max_len limit.",
            "This is line 6 with final content to ensure proper multi-chunk splitting.",
            "This is line 7 with additional verification content for thorough testing.",
            "This is line 8 providing the last bit of content for split verification."
        ];
        let long_content = lines.join("\n");
        let max_len = 200; // Small limit to force splitting
        
        // Verify the content is actually long enough to require splitting
        assert!(long_content.len() > max_len, "Test content should be longer than max_len for proper testing");
        
        let chunks = split_message(&long_content, max_len);
        
        // Should create multiple chunks since we have multiple lines that exceed the limit
        assert!(chunks.len() > 1, "Content of {} chars with {} lines should split with max_len {}", 
                long_content.len(), lines.len(), max_len);
        
        // Each chunk should be within the limit (except possibly the last one with remaining content)
        for (i, chunk) in chunks.iter().enumerate() {
            if i < chunks.len() - 1 {
                assert!(chunk.len() <= max_len, "Chunk {} exceeds max length: {} > {}", i, chunk.len(), max_len);
            }
        }
        
        // All chunks combined should contain the original content (minus whitespace differences)
        let combined = chunks.join("\n");
        let original_words: Vec<&str> = long_content.split_whitespace().collect();
        let combined_words: Vec<&str> = combined.split_whitespace().collect();
        assert_eq!(original_words.len(), combined_words.len());
        
        // Test with very short content that shouldn't be split
        let short_content = "Short message";
        let short_chunks = split_message(short_content, max_len);
        assert_eq!(short_chunks.len(), 1, "Short content should not be split");
        assert_eq!(short_chunks[0], short_content);
        
        // Test with single long line (should not be split since split_message works by lines)
        let single_long_line = "This is a very long single line that exceeds the max_len but has no line breaks so it cannot be split by the line-based splitting algorithm.".repeat(5);
        let single_line_chunks = split_message(&single_long_line, max_len);
        assert_eq!(single_line_chunks.len(), 1, "Single long line should not be split");
    }
}

// Command group exports
#[group]
#[commands(sum)]
pub struct Sum;

impl Sum {
    pub const fn new() -> Self {
        Sum
    }
} 