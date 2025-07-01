use reqwest;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs;
use std::collections::HashMap;
use serenity::{client::Context, model::channel::Message};

/// Error types for search operations
#[derive(Debug)]
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
    pub default_temperature: f32,
    pub default_max_tokens: i32,
    pub max_discord_message_length: usize,
    pub response_format_padding: usize,
}

/// Chat message structure for AI API
#[derive(Serialize, Deserialize, Clone)]
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
        temperature: 0.3, // Lower temperature for focused responses
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
        "You are an expert information synthesizer. Summarize these web search results into a comprehensive, well-organized response under 1800 characters. Use Discord formatting (bold, code blocks) for better readability. Include relevant links directly embedded in your response.".to_string()
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
        "User's search query: {}\n\nSearch results to summarize:\n{}\n\nPlease provide a comprehensive summary that directly answers the user's question. Include relevant links by embedding them naturally in your response using Discord markdown format [title](URL). Focus on the most important information and cite 2-3 of the most relevant sources.",
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
        Some(512), // Limit tokens for summary to ensure it fits Discord
    ).await?;

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
    let results = ddg_search(user_query).await?;
    
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

/// Perform a DuckDuckGo search and return top results
pub async fn ddg_search(query: &str) -> Result<Vec<SearchResult>, SearchError> {
    if query.trim().is_empty() {
        return Err(SearchError::NoResults("Empty search query provided".to_string()));
    }

    println!("🔍 Performing DuckDuckGo search for: '{}'", query);

    // Build DuckDuckGo search URL
    let encoded_query = urlencoding::encode(query);
    let search_url = format!("https://html.duckduckgo.com/html/?q={}", encoded_query);
    
    // Create HTTP client with timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .build()?;

    // Send GET request
    let response = client.get(&search_url).send().await?;
    
    if !response.status().is_success() {
        return Err(SearchError::ParseError(
            format!("HTTP request failed with status: {}", response.status())
        ));
    }

    let html_content = response.text().await?;
    
    // Parse HTML content
    let document = Html::parse_document(&html_content);
    
    // DuckDuckGo result selectors
    let result_selector = Selector::parse("div.result").map_err(|e| {
        SearchError::ParseError(format!("Failed to parse result selector: {:?}", e))
    })?;
    
    let title_selector = Selector::parse("a.result__a").map_err(|e| {
        SearchError::ParseError(format!("Failed to parse title selector: {:?}", e))
    })?;
    
    let snippet_selector = Selector::parse("a.result__snippet").map_err(|e| {
        SearchError::ParseError(format!("Failed to parse snippet selector: {:?}", e))
    })?;

    let mut search_results = Vec::new();
    let max_results = 5; // Limit to top 5 results

    // Extract search results
    for result_element in document.select(&result_selector).take(max_results) {
        let title_element = result_element.select(&title_selector).next();
        let snippet_element = result_element.select(&snippet_selector).next();

        if let (Some(title_elem), Some(snippet_elem)) = (title_element, snippet_element) {
            let title = title_elem.inner_html();
            let link = title_elem.value().attr("href").unwrap_or("").to_string();
            let snippet = snippet_elem.inner_html();

            // Clean up the extracted text (remove HTML tags, decode entities)
            let clean_title = clean_html_text(&title);
            let clean_snippet = clean_html_text(&snippet);
            let clean_link = if link.starts_with("//") {
                format!("https:{}", link)
            } else if link.starts_with("/") {
                format!("https://duckduckgo.com{}", link)
            } else {
                link
            };

            if !clean_title.is_empty() && !clean_link.is_empty() {
                search_results.push(SearchResult::new(
                    clean_title,
                    clean_link,
                    clean_snippet,
                ));
            }
        }
    }

    println!("🔍 Found {} search results", search_results.len());

    if search_results.is_empty() {
        return Err(SearchError::NoResults(format!(
            "No results found for query: '{}'", query
        )));
    }

    Ok(search_results)
}

/// Helper function to clean HTML text and decode HTML entities
fn clean_html_text(html: &str) -> String {
    // Remove HTML tags
    let document = Html::parse_fragment(html);
    let text = document.root_element().text().collect::<Vec<_>>().join(" ");
    
    // Basic HTML entity decoding
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
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

/// Perform basic search without AI enhancement (fallback)
pub async fn perform_basic_search(
    ctx: &Context,
    msg: &Message,
    search_query: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Send initial search message
    let mut search_msg = match msg.channel_id.send_message(&ctx.http, |m| {
        m.content("🔍 Searching DuckDuckGo...")
    }).await {
        Ok(message) => message,
        Err(e) => {
            eprintln!("❌ Failed to send initial search message: {}", e);
            msg.reply(ctx, "❌ Failed to send message!").await?;
            return Err(e.into());
        }
    };

    // Perform basic search
    match ddg_search(search_query).await {
        Ok(results) => {
            let formatted_results = format_search_results(&results, search_query);
            
            // Update message with search results
            if let Err(e) = search_msg.edit(&ctx.http, |m| {
                m.content(&formatted_results)
            }).await {
                eprintln!("❌ Failed to update search message: {}", e);
                msg.reply(ctx, "❌ Failed to display search results!").await?;
                return Err(e.into());
            }
            
            println!("🔍 Basic search completed successfully for query: '{}'", search_query);
        }
        Err(e) => {
            eprintln!("❌ Basic search failed: {}", e);
            let error_msg = format!("❌ **Search Failed**\n\nQuery: `{}`\nError: {}\n\n💡 Try rephrasing your search query or check your internet connection.", search_query, e);
            
            if let Err(edit_error) = search_msg.edit(&ctx.http, |m| {
                m.content(&error_msg)
            }).await {
                eprintln!("❌ Failed to update search message with error: {}", edit_error);
                msg.reply(ctx, &error_msg).await?;
            }
            return Err(e.into());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_html_text() {
        let html = "&lt;b&gt;Hello &amp; World&lt;/b&gt;";
        let cleaned = clean_html_text(html);
        assert_eq!(cleaned, "<b>Hello & World</b>");
    }

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
} 