// rank.rs - Self-Contained Webpage and YouTube Ranking Command Module
// This module implements the ^rank command, providing AI-powered content ranking and analysis for webpages and YouTube videos.
// It supports robust content fetching, VTT/HTML cleaning, RAG chunking, and real-time streaming to Discord.
//
// Key Features:
// - Ranks and analyzes arbitrary webpages and YouTube videos
// - Uses yt-dlp for YouTube transcript extraction
// - Cleans and processes VTT/HTML content
// - RAG (map-reduce) chunking for long content
// - Real-time streaming of ranking analysis to Discord
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
            println!("[DEBUG][CONNECTIVITY] API endpoint OK - Status: {}", status);
            if status.is_success() {
                println!("[DEBUG][CONNECTIVITY] API connectivity test PASSED");
                Ok(())
            } else {
                let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
                println!("[DEBUG][CONNECTIVITY] API endpoint returned error status {}: {}", status, error_text);
                Err(format!("API endpoint test failed: HTTP {} - {}", status, error_text).into())
            }
        }
        Err(e) => {
            println!("[DEBUG][CONNECTIVITY] API endpoint test failed: {}", e);
            Err(format!("API endpoint connectivity test failed: {}", e).into())
        }
    }
}

/// Load LM Studio configuration from lmapiconf.txt with multi-path fallback
pub async fn load_lm_config() -> Result<LMConfig, Box<dyn std::error::Error + Send + Sync>> {
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
                println!("Ranking command: Found config file at {}", config_path);
                break;
            }
            Err(_) => {
                continue;
            }
        }
    }
    
    if !found_file {
        return Err("lmapiconf.txt file not found in any expected location (., .., ../.., src/) for ranking command".into());
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
        "DEFAULT_SUMMARIZATION_MODEL",
        "DEFAULT_RANKING_MODEL",
        "DEFAULT_TEMPERATURE",
        "DEFAULT_MAX_TOKENS",
        "MAX_DISCORD_MESSAGE_LENGTH",
        "RESPONSE_FORMAT_PADDING",
        "DEFAULT_VISION_MODEL",
    ];
    
    for key in &required_keys {
        if !config_map.contains_key(*key) {
            return Err(format!("Required setting '{}' not found in {} (ranking command)", key, config_source).into());
        }
    }
    
    // Validate timeout value
    let timeout = config_map.get("LM_STUDIO_TIMEOUT")
        .ok_or("LM_STUDIO_TIMEOUT not found in lmapiconf.txt")?
        .parse::<u64>()
        .map_err(|_| "LM_STUDIO_TIMEOUT must be a valid number (seconds)")?;
    
    if timeout == 0 || timeout > 600 {
        return Err(format!(
            "❌ **Invalid Timeout Value**\n\n\
            LM_STUDIO_TIMEOUT must be between 1 and 600 seconds\n\
            Current value: {} seconds\n\
            Recommended: 300 seconds for complex ranking operations", 
            timeout
        ).into());
    }
    
    // Create config - all values must be present in lmapiconf.txt
    let config = LMConfig {
        base_url: config_map.get("LM_STUDIO_BASE_URL")
            .ok_or("LM_STUDIO_BASE_URL not found")?.clone(),
        timeout,
        default_model: config_map.get("DEFAULT_MODEL")
            .ok_or("DEFAULT_MODEL not found")?.clone(),
        default_reason_model: config_map.get("DEFAULT_REASON_MODEL")
            .ok_or("DEFAULT_REASON_MODEL not found")?.clone(),
        default_summarization_model: config_map.get("DEFAULT_SUMMARIZATION_MODEL")
            .ok_or("DEFAULT_SUMMARIZATION_MODEL not found")?.clone(),
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
        default_seed: config_map.get("DEFAULT_SEED")
            .map(|s| s.parse::<i64>())
            .transpose()
            .map_err(|_| "DEFAULT_SEED must be a valid integer if specified")?,
    };

    println!("Ranking command: Successfully loaded config from {} with ranking model: '{}'", config_source, config.default_ranking_model);
    Ok(config)
} 

/// Chat completion function with retry logic
pub async fn chat_completion(
    messages: Vec<ChatMessage>,
    model: &str,
    config: &LMConfig,
    max_tokens: Option<i32>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    chat_completion_with_retries(messages, model, config, max_tokens, 3).await
}

/// Chat completion with retry logic for reliability
async fn chat_completion_with_retries(
    messages: Vec<ChatMessage>,
    model: &str,
    config: &LMConfig,
    max_tokens: Option<i32>,
    max_retries: u32,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    let mut last_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    
    for attempt in 1..=max_retries {
        println!("[DEBUG][CHAT] Attempt {} of {} for chat completion", attempt, max_retries);
        
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
                    last_error = Some(Box::new(e));
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
                    println!("[DEBUG][CHAT] Response parsing failed, retrying in {:.1}s...", delay.as_secs_f32());
                    tokio::time::sleep(delay).await;
                    last_error = Some(Box::new(e));
                    continue;
                } else {
                    return Err(format!("Failed to read response text: {}", e).into());
                }
            }
        };

        // Parse JSON response
        let response_json: serde_json::Value = match serde_json::from_str(&response_text) {
            Ok(json) => json,
            Err(e) => {
                if attempt < max_retries {
                    let delay = Duration::from_millis(1000 * attempt as u64);
                    println!("[DEBUG][CHAT] JSON parsing failed, retrying in {:.1}s...", delay.as_secs_f32());
                    tokio::time::sleep(delay).await;
                    last_error = Some(Box::new(e));
                    continue;
                } else {
                    return Err(format!("Failed to parse JSON response: {}", e).into());
                }
            }
        };

        // Extract content from response
        if let Some(choices) = response_json["choices"].as_array() {
            if let Some(first_choice) = choices.get(0) {
                if let Some(message) = first_choice["message"].as_object() {
                    if let Some(content) = message["content"].as_str() {
                        let elapsed = start_time.elapsed();
                        println!("[DEBUG][CHAT] Chat completion successful in {:.2}s - {} characters", 
                                elapsed.as_secs_f32(), content.len());
                        return Ok(content.trim().to_string());
                    }
                }
            }
        }

        // If we get here, the response format was unexpected
        if attempt < max_retries {
            let delay = Duration::from_millis(1000 * attempt as u64);
            println!("[DEBUG][CHAT] Unexpected response format, retrying in {:.1}s...", delay.as_secs_f32());
            tokio::time::sleep(delay).await;
            last_error = Some(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Unexpected response format")));
            continue;
        } else {
            return Err("Failed to extract content from API response - unexpected format".into());
        }
    }

    // If we get here, all retries failed
    match last_error {
        Some(e) => Err(format!("All {} retry attempts failed. Last error: {}", max_retries, e).into()),
        None => Err("All retry attempts failed with unknown errors".into()),
    }
}

// ============================================================================
// STREAMING STRUCTURES FOR REAL-TIME RESPONSES
// ============================================================================

// Structure for streaming API responses
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

// ============================================================================
// HELPER FUNCTIONS FOR CONTENT PROCESSING
// ============================================================================

/// Load ranking-specific system prompt from file
async fn load_ranking_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let prompt_paths = [
        "rank_system_prompt.txt",
        "../rank_system_prompt.txt",
        "../../rank_system_prompt.txt",
        "src/rank_system_prompt.txt",
        "system_prompt.txt",
        "../system_prompt.txt",
        "../../system_prompt.txt",
        "src/system_prompt.txt",
    ];
    
    for path in &prompt_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                println!("Ranking command: Loaded prompt from {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    // Fallback prompt for ranking
    Ok("You are an expert content analyst and evaluator. Your task is to rank and analyze content across multiple dimensions. Provide detailed, objective analysis with specific scores and actionable feedback.".to_string())
}

/// Load YouTube-specific ranking prompt
async fn load_youtube_ranking_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let prompt_paths = [
        "youtube_ranking_prompt.txt",
        "../youtube_ranking_prompt.txt",
        "../../youtube_ranking_prompt.txt",
        "src/youtube_ranking_prompt.txt",
        "rank_system_prompt.txt",
        "../rank_system_prompt.txt",
        "../../rank_system_prompt.txt",
        "src/rank_system_prompt.txt",
    ];
    
    for path in &prompt_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                println!("Ranking command: Loaded YouTube ranking prompt from {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    // Fallback prompt for YouTube ranking
    Ok("You are an expert video content analyst. Analyze YouTube video transcripts and provide comprehensive ranking across multiple dimensions including content quality, educational value, engagement, and technical excellence.".to_string())
}

/// Generate cache key for YouTube URL
fn generate_youtube_cache_key(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Fetch YouTube transcript using yt-dlp
async fn fetch_youtube_transcript(url: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let process_uuid = Uuid::new_v4();
    trace!("[TRACE][RANK][fetch_youtube_transcript] === FUNCTION ENTRY ===");
    trace!("[TRACE][RANK][fetch_youtube_transcript] Function: fetch_youtube_transcript()");
    trace!("[TRACE][RANK][fetch_youtube_transcript] Process UUID: {}", process_uuid);
    trace!("[TRACE][RANK][fetch_youtube_transcript] Input URL: '{}'", url);
    trace!("[TRACE][RANK][fetch_youtube_transcript] Current working dir: {:?}", std::env::current_dir());
    
    // Create subtitles directory if it doesn't exist
    let subtitles_dir = "subtitles";
    if !std::path::Path::new(subtitles_dir).exists() {
        fs::create_dir_all(subtitles_dir)?;
        println!("Created subtitles directory: {}", subtitles_dir);
    }
    
    // Generate cache key and file path
    let cache_key = generate_youtube_cache_key(url);
    let subtitle_file_path = format!("{}/{}.vtt", subtitles_dir, cache_key);
    
    // Check if we have a cached version
    if std::path::Path::new(&subtitle_file_path).exists() {
        println!("Using cached subtitle file: {}", subtitle_file_path);
        match fs::read_to_string(&subtitle_file_path) {
            Ok(content) => {
                let cleaned_content = clean_vtt_content(&content);
                if !cleaned_content.trim().is_empty() {
                    trace!("[TRACE][RANK][fetch_youtube_transcript] Using cached content: {} chars", cleaned_content.len());
                    return Ok(cleaned_content);
                }
            }
            Err(e) => {
                println!("Failed to read cached subtitle file: {}", e);
            }
        }
    }
    
    // Download transcript using yt-dlp
    println!("Downloading transcript for: {}", url);
    
    let output = Command::new("yt-dlp")
        .args(&[
            "--write-sub",
            "--write-auto-sub",
            "--sub-format", "vtt",
            "--skip-download",
            "--output", &subtitle_file_path,
            url
        ])
        .output()?;
    
    if !output.status.success() {
        let error_output = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp failed: {}\n\nError output:\n{}", output.status, error_output).into());
    }
    
    // Read the downloaded subtitle file
    match fs::read_to_string(&subtitle_file_path) {
        Ok(content) => {
            let cleaned_content = clean_vtt_content(&content);
            if cleaned_content.trim().is_empty() {
                return Err("No subtitle content found after cleaning".into());
            }
            println!("Successfully extracted transcript: {} characters", cleaned_content.len());
            trace!("[TRACE][RANK][fetch_youtube_transcript] Successfully extracted transcript: {} chars", cleaned_content.len());
            Ok(cleaned_content)
        }
        Err(e) => {
            Err(format!("Failed to read subtitle file: {}", e).into())
        }
    }
}

/// Clean VTT subtitle content
fn clean_vtt_content(vtt: &str) -> String {
    let lines: Vec<&str> = vtt.lines().collect();
    let mut cleaned_lines = Vec::new();
    let mut in_cue = false;
    
    for line in lines {
        let line = line.trim();
        
        // Skip empty lines and VTT header
        if line.is_empty() || line == "WEBVTT" || line.starts_with("NOTE") {
            continue;
        }
        
        // Skip timestamp lines
        if line.contains("-->") {
            in_cue = true;
            continue;
        }
        
        // Skip cue identifier lines (numbers)
        if line.chars().all(|c| c.is_numeric()) {
            continue;
        }
        
        // Add content lines
        if in_cue && !line.is_empty() {
            cleaned_lines.push(line);
        }
    }
    
    cleaned_lines.join(" ")
}

/// Fetch webpage content
async fn fetch_webpage_content(url: &str) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    let client = get_http_client().await;
    
    let response = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(format!("HTTP {}: {}", response.status(), response.status().as_str()).into());
    }
    
    let html = response.text().await?;
    let cleaned_content = clean_html(&html);
    
    // Extract title from HTML
    let title = extract_title_from_html(&html).unwrap_or_else(|| "Unknown Title".to_string());
    
    Ok((title, cleaned_content))
}

/// Extract title from HTML
fn extract_title_from_html(html: &str) -> Option<String> {
    let title_regex = Regex::new(r"<title[^>]*>(.*?)</title>").ok()?;
    title_regex.captures(html)
        .and_then(|caps| caps.get(1))
        .map(|m| clean_html(&m.as_str()))
}

/// Clean HTML content
fn clean_html(html: &str) -> String {
    // Remove HTML tags
    let tag_regex = Regex::new(r"<[^>]+>").unwrap();
    let mut cleaned = tag_regex.replace_all(html, " ").to_string();
    
    // Decode HTML entities
    cleaned = cleaned.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    
    // Remove extra whitespace
    let whitespace_regex = Regex::new(r"\s+").unwrap();
    cleaned = whitespace_regex.replace_all(&cleaned, " ").to_string();
    
    cleaned.trim().to_string()
}

/// Stream ranking analysis to Discord
async fn stream_ranking_analysis(
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
    let _content_to_process = content.to_string();
    
    trace!("[TRACE][RANK][stream_ranking_analysis] === FUNCTION ENTRY ===");
    trace!("[TRACE][RANK][stream_ranking_analysis] Function: stream_ranking_analysis()");
    trace!("[TRACE][RANK][stream_ranking_analysis] Stream UUID: {}", stream_uuid);
    trace!("[TRACE][RANK][stream_ranking_analysis] Input content length: {} chars", content.len());
    trace!("[TRACE][RANK][stream_ranking_analysis] URL: '{}'", url);
    trace!("[TRACE][RANK][stream_ranking_analysis] Is YouTube: {}", is_youtube);
    trace!("[TRACE][RANK][stream_ranking_analysis] File path: {:?}", file_path);
    trace!("[TRACE][RANK][stream_ranking_analysis] Selected model: {}", selected_model);
    trace!("[TRACE][RANK][stream_ranking_analysis] Config base URL: {}", config.base_url);
    trace!("[TRACE][RANK][stream_ranking_analysis] Config timeout: {} seconds", config.timeout);
    
    // Load appropriate prompt
    let system_prompt = if is_youtube {
        load_youtube_ranking_prompt().await?
    } else {
        load_ranking_prompt().await?
    };
    
    // Prepare content for analysis
    let content_preview = if content.len() > 1000 {
        format!("{}...", &content[..1000])
    } else {
        content.to_string()
    };
    
    let user_prompt = format!(
        "Please analyze and rank the following content:\n\n\
Content Type: {}\n\
URL: {}\n\n\
Content:\n{}\n\n\
Provide a comprehensive ranking analysis with:\n\
1. Overall Score (1-10)\n\
2. Detailed breakdown by category\n\
3. Strengths and weaknesses\n\
4. Recommendations for improvement\n\
5. Summary conclusion",
        if is_youtube { "YouTube Video" } else { "Webpage" },
        url,
        content_preview
    );
    
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
    
    // Use chat completion for ranking analysis
    match chat_completion(messages, selected_model, config, Some(4000)).await {
        Ok(analysis) => {
            // Split the analysis into Discord-friendly chunks
            let chunks = split_message(&analysis, config.max_discord_message_length - config.response_format_padding);
            
            for (i, chunk) in chunks.iter().enumerate() {
                let chunk_content = if chunks.len() == 1 {
                    format!("**📊 Content Ranking Analysis**\n\n{}", chunk)
                } else {
                    format!("**📊 Content Ranking Analysis (Part {}/{})**\n\n{}", i + 1, chunks.len(), chunk)
                };
                
                if i == 0 {
                    // Update the first message
                    msg.edit(&ctx.http, |m| m.content(&chunk_content)).await?;
                } else {
                    // Send additional messages for remaining chunks
                    msg.channel_id.send_message(&ctx.http, |m| m.content(&chunk_content)).await?;
                }
            }
            
            Ok(())
        }
        Err(e) => {
            Err(format!("Ranking analysis failed: {}", e).into())
        }
    }
}

/// Split message content into Discord-friendly chunks
fn split_message(content: &str, max_len: usize) -> Vec<String> {
    let lines: Vec<&str> = content.lines().collect();
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    
    for line in lines {
        let potential_chunk = if current_chunk.is_empty() {
            line.to_string()
        } else {
            format!("{}\n{}", current_chunk, line)
        };
        
        if potential_chunk.len() <= max_len {
            current_chunk = potential_chunk;
        } else {
            if !current_chunk.is_empty() {
                chunks.push(current_chunk.clone());
            }
            current_chunk = line.to_string();
        }
    }
    
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }
    
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
        let vtt_content = "WEBVTT\n\n1\n00:00:01.000 --> 00:00:05.000\nHello world\n\n2\n00:00:06.000 --> 00:00:10.000\nThis is a test";
        let cleaned = clean_vtt_content(vtt_content);
        assert_eq!(cleaned, "Hello world This is a test");
    }
    
    #[test]
    fn test_clean_html() {
        let html = "<html><body><h1>Title</h1><p>Content with <strong>bold</strong> text</p></body></html>";
        let cleaned = clean_html(html);
        assert_eq!(cleaned, "Title Content with bold text");
    }
    
    #[test]
    fn test_webpage_content_processing() {
        let html = r#"
        <html>
        <head><title>Test Page</title></head>
        <body>
            <h1>Main Title</h1>
            <p>This is some content with <strong>bold</strong> text.</p>
            <p>Another paragraph with <em>italic</em> text.</p>
        </body>
        </html>
        "#;
        
        let cleaned = clean_html(html);
        assert!(cleaned.contains("Test Page"));
        assert!(cleaned.contains("Main Title"));
        assert!(cleaned.contains("This is some content with bold text"));
        assert!(cleaned.contains("Another paragraph with italic text"));
    }
    
    #[test]
    fn test_lm_config_structure() {
        let config = LMConfig {
            base_url: "http://localhost:1234".to_string(),
            timeout: 300,
            default_model: "test-model".to_string(),
            default_reason_model: "reason-model".to_string(),
            default_summarization_model: "sum-model".to_string(),
            default_ranking_model: "rank-model".to_string(),
            default_temperature: 0.8,
            default_max_tokens: 4000,
            max_discord_message_length: 2000,
            response_format_padding: 100,
            default_vision_model: "vision-model".to_string(),
            default_seed: Some(42),
        };
        
        assert_eq!(config.base_url, "http://localhost:1234");
        assert_eq!(config.timeout, 300);
        assert_eq!(config.default_ranking_model, "rank-model");
        assert_eq!(config.default_temperature, 0.8);
        assert_eq!(config.default_max_tokens, 4000);
        assert_eq!(config.max_discord_message_length, 2000);
        assert_eq!(config.response_format_padding, 100);
        assert_eq!(config.default_seed, Some(42));
    }
    
    #[test]
    fn test_chat_message_structure() {
        let message = ChatMessage {
            role: "user".to_string(),
            content: "Test message".to_string(),
        };
        
        assert_eq!(message.role, "user");
        assert_eq!(message.content, "Test message");
        
        // Test serialization/deserialization
        let json = serde_json::to_string(&message).unwrap();
        let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, message.role);
        assert_eq!(deserialized.content, message.content);
    }
    
    #[test]
    fn test_split_message_functionality() {
        let short_content = "This is a short message.";
        let chunks = split_message(short_content, 50);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], short_content);
        
        let long_content = vec![
            "This is the first line of a longer message.",
            "This is the second line with more content.",
            "This is the third line that should be split.",
            "This is the fourth line to test chunking.",
            "This is the fifth line to complete the test."
        ].join("\n");
        
        let chunks = split_message(&long_content, 80);
        assert!(chunks.len() > 1, "Long content should be split into multiple chunks");
        
        // Test that each chunk is within the limit
        for chunk in &chunks {
            assert!(chunk.len() <= 80, "Chunk exceeds maximum length: {}", chunk.len());
        }
        
        // Test single long line
        let single_long_line = "This is a very long line that should not be split because it exceeds the maximum length but we want to keep it as one chunk for testing purposes.";
        let chunks = split_message(single_long_line, 50);
        assert_eq!(chunks.len(), 1, "Single long line should remain as one chunk");
    }
}

// ============================================================================
// MAIN RANK COMMAND
// ============================================================================

#[command]
#[aliases("analyze", "evaluate")]
/// Main ^rank command handler
/// Analyzes and ranks webpage content or YouTube videos using AI
/// Supports:
///   - ^rank <url> (rank webpage or YouTube video)
///   - ^rank --help (show help)
pub async fn rank(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let command_uuid = Uuid::new_v4();
    let start_time = Instant::now();
    
    // Generate trace logging for function entry
    trace!("[TRACE][RANK] === FUNCTION ENTRY: rank() ===");
    trace!("[TRACE][RANK] Function: rank(), UUID: {}", command_uuid);
    trace!("[TRACE][RANK] Entry timestamp: {:?}", start_time);
    
    // Get context data for logging
    {
        let _data_map = ctx.data.read().await;
        trace!("[TRACE][RANK] Context data lock acquired successfully");
    }
    
    trace!("[TRACE][RANK] Message author: {} (ID: {})", msg.author.name, msg.author.id);
    trace!("[TRACE][RANK] Channel ID: {}", msg.channel_id);
    trace!("[TRACE][RANK] Guild ID: {:?}", msg.guild_id);
    
    let input = args.message().trim();
    trace!("[TRACE][RANK] Raw input: '{}' ({} chars)", input, input.len());
    
    // Check for help command
    if input.is_empty() || input == "--help" || input == "-h" {
        let help_message = "**📊 Content Ranking Command**\n\n\
**Usage:** `^rank <url>`\n\n\
**Features:**\n\
• **Multi-dimensional ranking** across 5 key factors\n\
• **1-10 scale scoring** with detailed explanations\n\
• **YouTube and webpage support** with specialized analysis\n\
• **RAG processing** for comprehensive content analysis\n\
• **Streaming responses** with real-time ranking updates\n\
• **Actionable feedback** with strengths and improvement suggestions\n\n\
**Examples:**\n\
• `^rank https://youtube.com/watch?v=...`\n\
• `^rank https://example.com`\n\n\
**Requirements:** yt-dlp installed for YouTube support";
        
        msg.reply(ctx, help_message).await?;
        
        trace!("[TRACE][RANK] === FUNCTION EXIT: rank() ===");
        trace!("[TRACE][RANK] Function: rank(), UUID: {}", command_uuid);
        trace!("[TRACE][RANK] Exit status: HELP_REQUESTED");
        trace!("[TRACE][RANK] Total execution time: {:.3}s", start_time.elapsed().as_secs_f64());
        trace!("[TRACE][RANK] Exit timestamp: {:?}", Instant::now());
        
        return Ok(());
    }
    
    // Validate URL format
    if !input.starts_with("http://") && !input.starts_with("https://") {
        let error_message = "**❌ Invalid URL Format**\n\n\
Please provide a valid URL starting with `http://` or `https://`\n\n\
**Examples:**\n\
• `^rank https://youtube.com/watch?v=...`\n\
• `^rank https://example.com`\n\
• `^rank https://github.com/username/repo`";
        
        msg.reply(ctx, error_message).await?;
        
        trace!("[TRACE][RANK] === FUNCTION EXIT: rank() ===");
        trace!("[TRACE][RANK] Function: rank(), UUID: {}", command_uuid);
        trace!("[TRACE][RANK] Exit status: INVALID_URL_FORMAT");
        trace!("[TRACE][RANK] Total execution time: {:.3}s", start_time.elapsed().as_secs_f64());
        trace!("[TRACE][RANK] Exit timestamp: {:?}", Instant::now());
        
        return Ok(());
    }
    
    let url = input;
    trace!("[TRACE][RANK] Validated URL: '{}'", url);
    
    // Determine if this is a YouTube URL
    let is_youtube = url.contains("youtube.com") || url.contains("youtu.be");
    trace!("[TRACE][RANK] Content type: {}", if is_youtube { "YouTube" } else { "Webpage" });
    
    // Load LM Studio configuration
    let config = match load_lm_config().await {
        Ok(config) => {
            trace!("[TRACE][RANK] Configuration loaded successfully");
            config
        },
        Err(e) => {
            error!("Failed to load LM Studio configuration: {}", e);
            let error_message = format!("**❌ Configuration Error**\n\n\
Failed to load LM Studio configuration: {}\n\n\
**Solutions:**\n\
• **Check lmapiconf.txt**: Ensure the file exists and contains all required settings\n\
• **Verify Settings**: Check that all required configuration values are present\n\
• **File Location**: Make sure lmapiconf.txt is in the project root directory\n\n\
**Required Settings:**\n\
• LM_STUDIO_BASE_URL\n\
• LM_STUDIO_TIMEOUT\n\
• DEFAULT_RANKING_MODEL\n\
• DEFAULT_TEMPERATURE\n\
• DEFAULT_MAX_TOKENS\n\
• MAX_DISCORD_MESSAGE_LENGTH\n\
• RESPONSE_FORMAT_PADDING", e);
            
            msg.reply(ctx, &error_message).await?;
            
            trace!("[TRACE][RANK] === FUNCTION EXIT: rank() ===");
            trace!("[TRACE][RANK] Function: rank(), UUID: {}", command_uuid);
            trace!("[TRACE][RANK] Exit status: CONFIGURATION_ERROR");
            trace!("[TRACE][RANK] Total execution time: {:.3}s", start_time.elapsed().as_secs_f64());
            trace!("[TRACE][RANK] Exit timestamp: {:?}", Instant::now());
            
            return Ok(());
        }
    };
    
    // Always use the ranking model for all content types
    let selected_model = &config.default_ranking_model;
    
    // Log the model selection
    info!("🎯 === RANKING MODEL SELECTION ===");
    info!("🎯 Using ranking model: {}", selected_model);
    debug!("🎯 Ranking model selected: {}", selected_model);
    trace!("🔍 Ranking model configuration: model={}, command_uuid={}", selected_model, command_uuid);
    
    // Send initial processing message
    let mut response_msg = match msg.channel_id.send_message(&ctx.http, |m| {
        m.content(if is_youtube {
            "🎥 **YouTube Video Ranking**\n\nProcessing YouTube video for ranking analysis..."
        } else {
            "📄 **Webpage Ranking**\n\nProcessing webpage for ranking analysis..."
        })
    }).await {
        Ok(message) => {
            trace!("[TRACE][RANK] Initial response message sent successfully");
            message
        },
        Err(e) => {
            error!("Failed to send initial response message: {}", e);
            msg.reply(ctx, "Failed to send response message!").await?;
            
            trace!("[TRACE][RANK] === FUNCTION EXIT: rank() ===");
            trace!("[TRACE][RANK] Function: rank(), UUID: {}", command_uuid);
            trace!("[TRACE][RANK] Exit status: MESSAGE_SEND_ERROR");
            trace!("[TRACE][RANK] Total execution time: {:.3}s", start_time.elapsed().as_secs_f64());
            trace!("[TRACE][RANK] Exit timestamp: {:?}", Instant::now());
            
            return Ok(());
        }
    };
    
    // Fetch content based on URL type
    let (content_for_ranking, subtitle_file_path) = if is_youtube {
        trace!("[TRACE][RANK] Starting YouTube transcript extraction");
        
        // Update message to show YouTube processing
        if let Err(e) = response_msg.edit(&ctx.http, |m| {
            m.content("🎥 **YouTube Video Ranking**\n\n📥 Downloading video transcript...")
        }).await {
            warn!("Failed to update message for YouTube processing: {}", e);
        }
        
        match fetch_youtube_transcript(url).await {
            Ok(transcript) => {
                trace!("[TRACE][RANK] YouTube transcript extracted successfully: {} chars", transcript.len());
                (transcript, None::<String>)
            },
            Err(e) => {
                error!("Failed to fetch YouTube transcript: {}", e);
                let error_message = format!("**❌ YouTube Processing Error**\n\n\
Failed to extract video transcript: {}\n\n\
**Solutions:**\n\
• **Install yt-dlp**: `pip install yt-dlp` or download from https://github.com/yt-dlp/yt-dlp\n\
• **Check Video**: Ensure the video is public and has captions/subtitles\n\
• **Try Again**: Some videos may have temporary issues\n\
• **Alternative**: Try a different video or webpage\n\n\
**URL:** {}", e, url);
                
                response_msg.edit(&ctx.http, |m| m.content(&error_message)).await?;
                
                trace!("[TRACE][RANK] === FUNCTION EXIT: rank() ===");
                trace!("[TRACE][RANK] Function: rank(), UUID: {}", command_uuid);
                trace!("[TRACE][RANK] Exit status: YOUTUBE_EXTRACTION_ERROR");
                trace!("[TRACE][RANK] Total execution time: {:.3}s", start_time.elapsed().as_secs_f64());
                trace!("[TRACE][RANK] Exit timestamp: {:?}", Instant::now());
                
                return Ok(());
            }
        }
    } else {
        trace!("[TRACE][RANK] Starting webpage content extraction");
        
        // Update message to show webpage processing
        if let Err(e) = response_msg.edit(&ctx.http, |m| {
            m.content("📄 **Webpage Ranking**\n\n📥 Fetching webpage content...")
        }).await {
            warn!("Failed to update message for webpage processing: {}", e);
        }
        
        match fetch_webpage_content(url).await {
            Ok((title, content)) => {
                trace!("[TRACE][RANK] Webpage content extracted successfully: title='{}', content={} chars", title, content.len());
                (content, None)
            },
            Err(e) => {
                error!("Failed to fetch webpage content: {}", e);
                let error_message = format!("**❌ Webpage Processing Error**\n\n\
Failed to fetch webpage content: {}\n\n\
**Solutions:**\n\
• **Check URL**: Ensure the URL is accessible and valid\n\
• **Network**: Verify your internet connection\n\
• **Try Again**: Some websites may have temporary issues\n\
• **Alternative**: Try a different webpage\n\n\
**URL:** {}", e, url);
                
                response_msg.edit(&ctx.http, |m| m.content(&error_message)).await?;
                
                trace!("[TRACE][RANK] === FUNCTION EXIT: rank() ===");
                trace!("[TRACE][RANK] Function: rank(), UUID: {}", command_uuid);
                trace!("[TRACE][RANK] Exit status: WEBPAGE_EXTRACTION_ERROR");
                trace!("[TRACE][RANK] Total execution time: {:.3}s", start_time.elapsed().as_secs_f64());
                trace!("[TRACE][RANK] Exit timestamp: {:?}", Instant::now());
                
                return Ok(());
            }
        }
    };
    
    let content_length = content_for_ranking.len();
    trace!("[TRACE][RANK] Content prepared for ranking: {} chars", content_length);
    
    // Update message to show ranking analysis
    if let Err(e) = response_msg.edit(&ctx.http, |m| {
        m.content(format!("🎯 **Content Ranking Analysis**\n\n📊 Analyzing {} characters of content...", content_length))
    }).await {
        warn!("Failed to update message for ranking analysis: {}", e);
    }
    
    // Stream the ranking analysis
    match stream_ranking_analysis(&content_for_ranking, url, &config, selected_model, &mut response_msg, ctx, is_youtube, subtitle_file_path.as_deref()).await {
        Ok(()) => {
            let total_time = start_time.elapsed();
            info!("✅ Ranking analysis completed successfully in {:.2}s", total_time.as_secs_f32());
            
            trace!("[TRACE][RANK] === FUNCTION EXIT: rank() ===");
            trace!("[TRACE][RANK] Function: rank(), UUID: {}", command_uuid);
            trace!("[TRACE][RANK] Exit status: SUCCESS");
            trace!("[TRACE][RANK] Total execution time: {:.3}s", total_time.as_secs_f64());
            trace!("[TRACE][RANK] Final content length: {} chars", content_length);
            trace!("[TRACE][RANK] Processing method used: {}", 
                   if let Some(ref _path) = subtitle_file_path { "RAG with file" } else { "Direct processing" });
            trace!("[TRACE][RANK] Exit timestamp: {:?}", Instant::now());
        },
        Err(e) => {
            error!("Failed to stream ranking analysis: {}", e);
            let error_message = format!("**❌ Ranking Analysis Error**\n\n\
Failed to complete ranking analysis: {}\n\n\
**Solutions:**\n\
• **Check LM Studio**: Ensure LM Studio is running and responsive\n\
• **Model Loaded**: Verify the ranking model is loaded\n\
• **Try Again**: Some content may be too complex for analysis\n\
• **Reduce Content**: Try a shorter video or webpage\n\n\
**URL:** {}", e, url);
            
            response_msg.edit(&ctx.http, |m| m.content(&error_message)).await?;
            
            trace!("[TRACE][RANK] === FUNCTION EXIT: rank() ===");
            trace!("[TRACE][RANK] Function: rank(), UUID: {}", command_uuid);
            trace!("[TRACE][RANK] Exit status: RANKING_ANALYSIS_ERROR");
            trace!("[TRACE][RANK] Total execution time: {:.3}s", start_time.elapsed().as_secs_f64());
            trace!("[TRACE][RANK] Exit timestamp: {:?}", Instant::now());
        }
    }
    
    Ok(())
}

// Command group exports
#[group]
#[commands(rank)]
pub struct Rank;

impl Rank {
    pub const fn new() -> Self {
        Rank
    }
}



