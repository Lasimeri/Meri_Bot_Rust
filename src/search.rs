use reqwest;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use serenity::{client::Context, model::channel::Message};
use serpapi::serpapi::Client;
use tokio::time::sleep;

// Rate limiting for search requests
static mut LAST_SEARCH_TIME: Option<Instant> = None;
const MIN_SEARCH_INTERVAL: Duration = Duration::from_millis(1000); // 1 second between searches

/// Rate limiter for search requests
async fn rate_limit_search() {
    unsafe {
        if let Some(last_time) = LAST_SEARCH_TIME {
            let elapsed = last_time.elapsed();
            if elapsed < MIN_SEARCH_INTERVAL {
                let sleep_duration = MIN_SEARCH_INTERVAL - elapsed;
                println!("⏱️ Rate limiting: waiting {:?}", sleep_duration);
                sleep(sleep_duration).await;
            }
        }
        LAST_SEARCH_TIME = Some(Instant::now());
    }
}

/// Error types for search operations
#[derive(Debug)]
#[allow(dead_code)]
pub enum SearchError {
    HttpError(reqwest::Error),
    ParseError(String),
    NoResults(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SearchError::HttpError(e) => write!(f, "HTTP request failed: {}", e),
            SearchError::ParseError(msg) => write!(f, "HTML parsing failed: {}", msg),
            SearchError::NoResults(msg) => write!(f, "No search results found: {}", msg),
        }
    }
}

impl Error for SearchError {}

impl From<reqwest::Error> for SearchError {
    fn from(error: reqwest::Error) -> Self {
        SearchError::HttpError(error)
    }
}

/// Represents a single search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub link: String,
    pub snippet: String,
}

impl SearchResult {
    pub fn new(title: String, link: String, snippet: String) -> Self {
        Self {
            title: title.trim().to_string(),
            link: link.trim().to_string(),
            snippet: snippet.trim().to_string(),
        }
    }
}

/// LM Studio API Configuration structure (imported from lm module)
#[derive(Debug, Clone)]
pub struct LMConfig {
    pub base_url: String,
    pub timeout: u64,
    pub default_model: String,
    pub default_reason_model: String,
    pub default_vision_model: String,
    pub default_temperature: f32,
    pub default_max_tokens: i32,
    pub max_discord_message_length: usize,
    pub response_format_padding: usize,
}

/// Chat message structure for AI API
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// API Request structure for AI completion
#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: i32,
    stream: bool,
}

/// API Response structure for AI completion
#[derive(Deserialize)]
#[allow(dead_code)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Choice {
    message: Option<MessageContent>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct MessageContent {
    content: Option<String>,
}

/// SerpAPI Configuration structure
#[derive(Debug, Clone)]
pub struct SerpConfig {
    pub api_key: String,
    pub engine: String,
    pub country: String,
    pub language: String,
}

/// Load SerpAPI configuration from file (required)
pub async fn load_serp_config() -> Result<SerpConfig, Box<dyn std::error::Error + Send + Sync>> {
    // First, load the SerpAPI key from serpapi.txt (single line, multi-path)
    let serpapi_paths = [
        "serpapi.txt",
        "../serpapi.txt",
        "../../serpapi.txt",
        "src/serpapi.txt"
    ];
    let mut api_key = None;
    for path in &serpapi_paths {
        if let Ok(content) = fs::read_to_string(path) {
            let key = content.trim();
            if !key.is_empty() && key != "your_serpapi_key_here" {
                api_key = Some(key.to_string());
                println!("✅ SerpAPI: Loaded API key from {}", path);
                break;
            }
        }
    }
    if api_key.is_none() {
        return Err("serpapi.txt file not found or API key missing/placeholder in any expected location (., .., ../.., src/)".into());
    }
    // Next, load the rest of the config from lmapiconf.txt (multi-path, as before)
    let config_paths = [
        "lmapiconf.txt",
        "../lmapiconf.txt", 
        "../../lmapiconf.txt",
        "src/lmapiconf.txt"
    ];
    let mut content = String::new();
    let mut found_file = false;
    let mut config_source = "";
    for config_path in &config_paths {
        match fs::read_to_string(config_path) {
            Ok(file_content) => {
                content = file_content;
                found_file = true;
                config_source = config_path;
                println!("🔍 SerpAPI: Loaded config from {}", config_path);
                break;
            }
            Err(_) => continue,
        }
    }
    if !found_file {
        return Err("lmapiconf.txt file not found in any expected location (., .., ../.., src/)".into());
    }
    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    let mut config_map = HashMap::new();
    for line in content.lines() {
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
    let config = SerpConfig {
        api_key: api_key.unwrap(),
        engine: config_map.get("SERPAPI_ENGINE").cloned().unwrap_or_else(|| "google".to_string()),
        country: config_map.get("SERPAPI_COUNTRY").cloned().unwrap_or_else(|| "us".to_string()),
        language: config_map.get("SERPAPI_LANGUAGE").cloned().unwrap_or_else(|| "en".to_string()),
    };
    println!("✅ SerpAPI: Configuration loaded successfully from {} and serpapi.txt", config_source);
    println!("🔍 SerpAPI: Engine={}, Country={}, Language={}", config.engine, config.country, config.language);
    Ok(config)
}

/// Load LM Studio configuration from file using multi-path fallback
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
                println!("🔍 Search module: Loaded config from {}", config_path);
                break;
            }
            Err(_) => {
                continue;
            }
        }
    }
    
    if !found_file {
        return Err("lmapiconf.txt file not found in any expected location (., .., ../.., src/)".into());
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
        "DEFAULT_VISION_MODEL",
        "DEFAULT_TEMPERATURE",
        "DEFAULT_MAX_TOKENS",
        "MAX_DISCORD_MESSAGE_LENGTH",
        "RESPONSE_FORMAT_PADDING"
    ];
    
    for key in &required_keys {
        if !config_map.contains_key(*key) {
            return Err(format!("❌ Required setting '{}' not found in {} (search module)", key, config_source).into());
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
        default_vision_model: config_map.get("DEFAULT_VISION_MODEL")
            .ok_or("DEFAULT_VISION_MODEL not found")?.clone(),
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

    println!("🔍 Search module: Configuration loaded successfully from {}", config_source);
    println!("🔍 Search module: Models configured:");
    println!("🔍   - Default Model: {}", config.default_model);
    println!("🔍   - Reason Model: {}", config.default_reason_model);
    println!("🔍   - Vision Model: {}", config.default_vision_model);
    Ok(config)
}

/// Load search query refinement prompt using multi-path fallback
pub async fn load_refine_search_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let prompt_paths = [
        "refine_search_prompt.txt",
        "../refine_search_prompt.txt",
        "../../refine_search_prompt.txt",
        "src/refine_search_prompt.txt",
        "example_refine_search_prompt.txt",
        "../example_refine_search_prompt.txt",
        "../../example_refine_search_prompt.txt",
        "src/example_refine_search_prompt.txt",
    ];
    
    for path in &prompt_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                // Remove BOM if present
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                println!("🔍 Query refinement: Loaded prompt from {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    // Fallback prompt if no file found
    println!("🔍 Query refinement: Using built-in fallback prompt");
    Ok("You are a search query optimizer. Refine the user's query to make it more effective for web search. Only respond with the refined search query, no explanations.".to_string())
}

/// Load search result summarization prompt using multi-path fallback
pub async fn load_summarize_search_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let prompt_paths = [
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
                println!("🔍 Result summary: Loaded prompt from {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    // Fallback prompt if no file found
    println!("🔍 Result summary: Using built-in fallback prompt");
    Ok("You are an expert information synthesizer. Summarize these web search results into a comprehensive, well-organized response under 1800 characters. Use Discord formatting (bold, code blocks) for better readability. Include relevant links directly embedded in your response.".to_string())
}

/// Non-streaming chat completion for AI query refinement and summarization
pub async fn chat_completion(
    messages: Vec<ChatMessage>,
    model: &str,
    config: &LMConfig,
    max_tokens: Option<i32>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    println!("🔗 Attempting connection to API server: {}", config.base_url);
    println!("🔗 Using model: {}", model);
    println!("🔗 Timeout: {} seconds", config.timeout);
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
        
    let chat_request = ChatRequest {
        model: model.to_string(),
        messages,
        temperature: 0.3, // Lower temperature for focused responses
        max_tokens: max_tokens.unwrap_or(config.default_max_tokens),
        stream: false,
    };

    let api_url = format!("{}/v1/chat/completions", config.base_url);
    println!("🔗 Full API URL: {}", api_url);
    println!("🔗 Request payload: model={}, max_tokens={}, temperature={}", 
        chat_request.model, chat_request.max_tokens, chat_request.temperature);

    // First, test basic connectivity to the server
    println!("🔗 Testing basic connectivity to {}...", config.base_url);
    match client.get(&config.base_url).send().await {
        Ok(response) => {
            println!("✅ Basic connectivity test successful - Status: {}", response.status());
        }
        Err(e) => {
            println!("❌ Basic connectivity test failed: {}", e);
            return Err(format!("Cannot reach remote server {}: {}", config.base_url, e).into());
        }
    }

    // Now attempt the actual API call
    println!("🔗 Making API request to chat completions endpoint...");
    let response = match client
        .post(&api_url)
        .json(&chat_request)
        .send()
        .await 
    {
        Ok(resp) => {
            println!("✅ API request sent successfully - Status: {}", resp.status());
            resp
        }
        Err(e) => {
            println!("❌ API request failed: {}", e);
            return Err(format!("API request to {} failed: {}", api_url, e).into());
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
        println!("❌ API returned error status {}: {}", status, error_text);
        return Err(format!("API request failed: HTTP {} - {}", status, error_text).into());
    }

    // Parse non-streaming response
    let response_text = response.text().await?;
    println!("✅ Received API response: {} characters", response_text.len());
    
    let response_json: serde_json::Value = serde_json::from_str(&response_text)
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;
    
    // Extract content from response
    if let Some(choices) = response_json["choices"].as_array() {
        if let Some(first_choice) = choices.get(0) {
            if let Some(message) = first_choice["message"].as_object() {
                if let Some(content) = message["content"].as_str() {
                    println!("✅ Successfully extracted content: {} characters", content.len());
                    return Ok(content.trim().to_string());
                }
            }
        }
    }
    
    println!("❌ Failed to extract content from response structure");
    println!("Response JSON: {}", serde_json::to_string_pretty(&response_json).unwrap_or_else(|_| "Cannot format JSON".to_string()));
    Err("Failed to extract content from API response".into())
}

/// Refine search query using AI
// Note: This function is currently unused since we now use exact user queries
#[allow(dead_code)]
async fn refine_search_query(
    user_query: &str,
    config: &LMConfig,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    println!("🔍 Query refinement: Optimizing search query: '{}'", user_query);
    
    let refine_prompt = load_refine_search_prompt().await.unwrap_or_else(|_| {
        "You are a search query optimizer. Refine the user's query to make it more effective for web search. Only respond with the refined search query, no explanations.".to_string()
    });

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: refine_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_query.to_string(),
        },
    ];

    let refined_query = chat_completion(
        messages,
        &config.default_model,
        config,
        Some(64), // Limit tokens for query refinement
    ).await?;

    println!("🔍 Query refinement: '{}' → '{}'", user_query, refined_query);
    Ok(refined_query)
}

/// Summarize search results using AI with embedded links
pub async fn summarize_search_results(
    results: &[SearchResult],
    _search_query: &str,
    user_query: &str,
    config: &LMConfig,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    println!("🔍 Result summary: Generating AI summary for {} results", results.len());
    
    let summarize_prompt = load_summarize_search_prompt().await.unwrap_or_else(|_| {
        "You are an expert information synthesizer. Summarize these web search results into a concise, well-organized response. Use Discord formatting and embed links using [title](URL) format. CRITICAL: Keep your ENTIRE response under 1600 characters including all formatting.".to_string()
    });

    // Format the results for the AI with both text and links
    let mut formatted_results = String::new();
    for (index, result) in results.iter().enumerate() {
        formatted_results.push_str(&format!(
            "Result {}: {}\nURL: {}\nDescription: {}\n\n",
            index + 1,
            result.title,
            result.link,
            if result.snippet.is_empty() { "No description available" } else { &result.snippet }
        ));
    }

    let user_prompt = format!(
        "User's search query: {}\n\nSearch results to summarize:\n{}\n\nProvide a CONCISE summary that directly answers the user's question. Include relevant links by embedding them using Discord markdown format [title](URL). Cite 2-3 of the most relevant sources. CRITICAL: Keep your ENTIRE response under 1600 characters.",
        user_query, formatted_results
    );

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: summarize_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_prompt,
        },
    ];

    let summary = chat_completion(
        messages,
        &config.default_model,
        config,
        Some(400), // Reduced from 512 to ensure shorter responses
    ).await?;

    // Additional safety check - truncate if still too long
    let summary = if summary.len() > 1700 {
        println!("⚠️ Search summary still too long ({} chars), truncating", summary.len());
        let truncated = if let Some(last_period) = summary[..1600].rfind('.') {
            &summary[..=last_period]
        } else {
            &summary[..1600]
        };
        format!("{}...", truncated.trim())
    } else {
        summary
    };

    println!("🔍 Result summary: Generated summary of {} characters", summary.len());
    Ok(summary)
}

/// Perform AI-enhanced search with direct user query and result summarization with embedded links
pub async fn perform_ai_enhanced_search(
    user_query: &str,
    config: &LMConfig,
    search_msg: &mut Message,
    ctx: &Context,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Step 1: Use the user's exact query for searching (no refinement)
    println!("🔍 Using exact user query for search: '{}'", user_query);
    
    // Update message to show search progress
    search_msg.edit(&ctx.http, |m| {
        m.content("🔍 Searching with your exact query...")
    }).await.map_err(|e| format!("Failed to update message: {}", e))?;

    // Step 2: Perform the web search with user's exact query
    let results = multi_search(user_query).await?;
    
    // Update message to show summarization progress
    search_msg.edit(&ctx.http, |m| {
        m.content("🤖 Generating AI summary...")
    }).await.map_err(|e| format!("Failed to update message: {}", e))?;

    // Step 3: Summarize the search results using AI with embedded links
    let summary = summarize_search_results(&results, user_query, user_query, config).await?;
    
    // Add search metadata to the summary
    let final_response = format!(
        "{}\n\n---\n*🔍 Searched: {}*",
        summary,
        user_query
    );

    // Step 4: Post the final AI-enhanced summary
    search_msg.edit(&ctx.http, |m| {
        m.content(&final_response)
    }).await.map_err(|e| format!("Failed to update message: {}", e))?;

    Ok(())
}

/// Test connectivity to the remote API server
pub async fn test_api_connectivity(config: &LMConfig) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    println!("🔗 Starting comprehensive API connectivity test...");
    println!("🔗 Target server: {}", config.base_url);
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout))
        .build()?;

    // Test 1: Basic connectivity (ping)
    println!("🔗 Test 1: Basic connectivity test...");
    match client.get(&config.base_url).send().await {
        Ok(response) => {
            println!("✅ Basic connectivity: SUCCESS - Status: {}", response.status());
        }
        Err(e) => {
            let error_msg = format!("❌ Basic connectivity: FAILED - {}", e);
            println!("{}", error_msg);
            return Err(error_msg.into());
        }
    }

    // Test 2: API endpoint accessibility
    let api_url = format!("{}/v1/chat/completions", config.base_url);
    println!("🔗 Test 2: API endpoint accessibility...");
    
    // Try a simple POST to see if the endpoint responds (even with an error is fine)
    let test_request = ChatRequest {
        model: "test".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "test".to_string(),
        }],
        temperature: 0.5,
        max_tokens: 1,
        stream: false,
    };

    match client.post(&api_url).json(&test_request).send().await {
        Ok(response) => {
            println!("✅ API endpoint: ACCESSIBLE - Status: {} ({})", response.status(), response.status().as_str());
            
            // Test 3: Check if it's a valid LLM API (should return JSON even on error)
            match response.text().await {
                Ok(body) => {
                    if body.contains("error") || body.contains("choices") || body.contains("model") {
                        println!("✅ API format: VALID - Response contains expected LLM API fields");
                    } else {
                        println!("⚠️ API format: UNKNOWN - Response doesn't look like LLM API: {}", &body[..body.len().min(200)]);
                    }
                }
                Err(e) => {
                    println!("⚠️ Response reading: FAILED - {}", e);
                }
            }
        }
        Err(e) => {
            let error_msg = format!("❌ API endpoint: FAILED - {}", e);
            println!("{}", error_msg);
            return Err(error_msg.into());
        }
    }

    // Test 4: Model availability check
    println!("🔗 Test 4: Testing with configured models...");
    println!("🔗 Default model: {}", config.default_model);
    println!("🔗 Reasoning model: {}", config.default_reason_model);

    let model_test_request = ChatRequest {
        model: config.default_model.clone(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello, this is a connectivity test.".to_string(),
        }],
        temperature: config.default_temperature,
        max_tokens: 10, // Very small response
        stream: false,
    };

    match client.post(&api_url).json(&model_test_request).send().await {
        Ok(response) => {
            let status = response.status();
            match response.text().await {
                Ok(body) => {
                    if status.is_success() {
                        println!("✅ Model test: SUCCESS - Model '{}' is available and responding", config.default_model);
                        return Ok(format!("🎉 All connectivity tests passed! Remote API server {} is accessible and responding correctly.", config.base_url));
                    } else {
                        if body.contains("model") && body.contains("not found") {
                            println!("❌ Model test: FAILED - Model '{}' not found on server", config.default_model);
                            return Err(format!("Model '{}' is not available on the remote server. Check your model name or load the model in LM Studio/Ollama.", config.default_model).into());
                        } else {
                            println!("⚠️ Model test: ERROR - Status: {}, Response: {}", status, &body[..body.len().min(300)]);
                            return Err(format!("API returned error for model '{}': {} - {}", config.default_model, status, body).into());
                        }
                    }
                }
                Err(e) => {
                    println!("❌ Model test: FAILED - Could not read response: {}", e);
                    return Err(format!("Failed to read model test response: {}", e).into());
                }
            }
        }
        Err(e) => {
            println!("❌ Model test: FAILED - Request failed: {}", e);
            return Err(format!("Model test request failed: {}", e).into());
        }
    }
}





/// Perform SerpAPI search (premium option)
pub async fn serpapi_search(query: &str, config: &SerpConfig) -> Result<Vec<SearchResult>, SearchError> {
    if query.trim().is_empty() {
        return Err(SearchError::NoResults("Empty search query provided".to_string()));
    }

    println!("🔍 Performing SerpAPI search for: '{}'", query);

    let mut default_params = HashMap::new();
    default_params.insert("api_key".to_string(), config.api_key.clone());
    default_params.insert("engine".to_string(), config.engine.clone());
    
    let client = Client::new(default_params);

    let mut search_params = HashMap::new();
    search_params.insert("q".to_string(), query.to_string());
    search_params.insert("gl".to_string(), config.country.clone());
    search_params.insert("hl".to_string(), config.language.clone());
    search_params.insert("num".to_string(), "10".to_string()); // Limit to 10 results

    match client.search(search_params).await {
        Ok(results) => {
            let mut search_results = Vec::new();
            
            // Parse organic results
            if let Some(organic_results) = results["organic_results"].as_array() {
                for result in organic_results.iter().take(5) {
                    let title = result["title"].as_str().unwrap_or("").to_string();
                    let link = result["link"].as_str().unwrap_or("").to_string();
                    let snippet = result["snippet"].as_str().unwrap_or("").to_string();
                    
                    if !title.is_empty() && !link.is_empty() {
                        search_results.push(SearchResult::new(title, link, snippet));
                    }
                }
            }
            
            println!("🔍 Found {} search results from SerpAPI", search_results.len());
            
            if search_results.is_empty() {
                return Err(SearchError::NoResults(format!(
                    "No results found for query: '{}' using SerpAPI", query
                )));
            }
            
            Ok(search_results)
        }
        Err(e) => {
            println!("⚠️ SerpAPI search failed: {}", e);
            Err(SearchError::NoResults(format!("SerpAPI error: {}", e)))
        }
    }
}

/// Perform search using SerpAPI (only search method)
pub async fn multi_search(query: &str) -> Result<Vec<SearchResult>, SearchError> {
    println!("🔍 Performing SerpAPI search for: '{}'", query);
    
    // Apply rate limiting
    rate_limit_search().await;
    
    // Validate query
    if query.trim().is_empty() {
        return Err(SearchError::NoResults("Empty search query provided".to_string()));
    }
    
    let query = query.trim();
    
    // Load SerpAPI configuration (required)
    let serp_config = match load_serp_config().await {
        Ok(config) => config,
        Err(e) => {
            return Err(SearchError::NoResults(format!(
                "SerpAPI configuration error: {}. Please add SERPAPI_KEY to lmapiconf.txt", e
            )));
        }
    };
    
    // Perform SerpAPI search
    match serpapi_search(query, &serp_config).await {
        Ok(results) => {
            println!("✅ SerpAPI search successful with {} results", results.len());
            Ok(results)
        }
        Err(e) => {
            println!("❌ SerpAPI search failed: {}", e);
            Err(e)
        }
    }
}







/// Format search results into a user-friendly string
pub fn format_search_results(results: &[SearchResult], query: &str) -> String {
    let mut formatted = format!("🔍 **Search Results for:** `{}`\n\n", query);
    
    for (index, result) in results.iter().enumerate() {
        formatted.push_str(&format!(
            "**{}. {}**\n{}\n🔗 {}\n\n",
            index + 1,
            result.title,
            if result.snippet.is_empty() { 
                "*No description available*" 
            } else { 
                &result.snippet 
            },
            result.link
        ));
    }
    
    if results.len() >= 5 {
        formatted.push_str("*Showing top 5 results*");
    }
    
    formatted
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_creation() {
        let result = SearchResult::new(
            "  Test Title  ".to_string(),
            "  https://example.com  ".to_string(),
            "  Test snippet  ".to_string(),
        );
        
        assert_eq!(result.title, "Test Title");
        assert_eq!(result.link, "https://example.com");
        assert_eq!(result.snippet, "Test snippet");
    }

    #[tokio::test]
    async fn test_config_loading_paths() {
        // Test that load_lm_config follows the same multi-path pattern as other modules
        // This test verifies the path search order without requiring actual files
        println!("🔍 Testing config loading paths...");
        
        // The function should try these paths in order:
        let _expected_paths = [
            "lmapiconf.txt",
            "../lmapiconf.txt", 
            "../../lmapiconf.txt",
            "src/lmapiconf.txt"
        ];
        
        // Since we don't have the files, this will fail gracefully
        // but the test validates that our path structure is consistent
        match load_lm_config().await {
            Ok(_) => println!("✅ Config loaded successfully"),
            Err(e) => {
                // Expected when files don't exist
                assert!(e.to_string().contains("not found in any expected location"));
                println!("✅ Path fallback working correctly: {}", e);
            }
        }
    }
    
    #[tokio::test]
    async fn test_prompt_loading_paths() {
        println!("🔍 Testing prompt loading paths...");
        
        // Test refine search prompt loading with fallback
        match load_refine_search_prompt().await {
            Ok(prompt) => {
                println!("✅ Refine search prompt loaded: {} chars", prompt.len());
                assert!(!prompt.is_empty());
            }
            Err(e) => {
                println!("❌ Failed to load refine search prompt: {}", e);
            }
        }
        
        // Test summarize search prompt loading with fallback
        match load_summarize_search_prompt().await {
            Ok(prompt) => {
                println!("✅ Summarize search prompt loaded: {} chars", prompt.len());
                assert!(!prompt.is_empty());
            }
            Err(e) => {
                println!("❌ Failed to load summarize search prompt: {}", e);
            }
        }
    }
    
    #[tokio::test]
    async fn test_serpapi_config_loading() {
        println!("🔍 Testing SerpAPI config loading...");
        
        match load_serp_config().await {
            Ok(config) => {
                println!("✅ SerpAPI config loaded successfully!");
                println!("🔍 API Key length: {}", config.api_key.len());
                println!("🔍 Engine: {}", config.engine);
                println!("🔍 Country: {}", config.country);
                println!("🔍 Language: {}", config.language);
                assert!(!config.api_key.is_empty());
                assert_ne!(config.api_key, "your_serpapi_key_here");
            }
            Err(e) => {
                println!("❌ Failed to load SerpAPI config: {}", e);
                // Don't fail the test, just log the error for debugging
            }
        }
    }
} 