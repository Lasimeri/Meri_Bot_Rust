use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};
use crate::search::{LMConfig, ChatMessage, load_lm_config};
use reqwest;
use std::time::Duration;
use std::fs;

// Struct for chat completion request
#[derive(serde::Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: i32,
    stream: bool,
}

// Struct for streaming chat completion response
#[derive(serde::Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(serde::Deserialize)]
struct Choice {
    delta: Option<Delta>,
    finish_reason: Option<String>,
}

#[derive(serde::Deserialize)]
struct Delta {
    content: Option<String>,
}

// Struct for message state management during streaming
struct MessageState {
    current_content: String,
    current_message: Message,
    message_index: usize,
    char_limit: usize,
}

// Struct for streaming statistics
struct StreamingStats {
    total_characters: usize,
    message_count: usize,
}

#[command]
#[aliases("summarize", "webpage")]
pub async fn sum(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    
    let url = args.message().trim();
    
    // Validate URL input
    if url.is_empty() {
        msg.reply(ctx, "❌ Please provide a URL to summarize!\n\n**Usage:** `^sum <url>`\n**Example:** `^sum https://example.com/article`").await?;
        return Ok(());
    }
    
    // Basic URL validation
    if !url.starts_with("http://") && !url.starts_with("https://") {
        msg.reply(ctx, "❌ Please provide a valid URL starting with `http://` or `https://`").await?;
        return Ok(());
    }
    
    println!("🔍 Summarizing webpage: {}", url);
    
    // Load configuration with detailed logging
    println!("⚙️ Loading LM configuration for summarization...");
    let config = match load_lm_config().await {
        Ok(config) => {
            println!("✅ LM configuration loaded successfully");
            println!("🔧 Using reasoning model: {}", config.default_reason_model);
            println!("🌐 API endpoint: {}", config.base_url);
            config
        },
        Err(e) => {
            println!("❌ Failed to load LM configuration: {}", e);
            msg.reply(ctx, &format!("❌ Failed to load LM configuration: {}\n\n**Setup required:** Ensure `lmapiconf.txt` is properly configured with your reasoning model.", e)).await?;
            return Ok(());
        }
    };
    
    // Create initial response message
    let mut response_msg = match msg.reply(ctx, "🌐 Fetching webpage content...").await {
        Ok(msg) => {
            println!("✅ Initial Discord message sent successfully");
            msg
        },
        Err(e) => {
            eprintln!("❌ Failed to send initial message: {}", e);
            return Ok(());
        }
    };
    
    // Fetch webpage content with detailed logging
    println!("📥 Starting webpage content fetching process...");
    let webpage_content = match fetch_webpage_content(url).await {
        Ok(content) => {
            println!("✅ Webpage content fetched successfully: {} characters", content.len());
            content
        },
        Err(e) => {
            println!("❌ Failed to fetch webpage content: {}", e);
            response_msg.edit(ctx, |m| {
                m.content(&format!("❌ Failed to fetch webpage: {}\n\n**Possible issues:**\n• URL might be invalid or unreachable\n• Website may block automated requests (403 Forbidden)\n• Network connectivity issues\n• Server errors on the target website", e))
            }).await?;
            return Ok(());
        }
    };
    
    // Update message to show processing
    println!("📝 Updating Discord message to show AI processing...");
    response_msg.edit(ctx, |m| {
        m.content("🤖 Sending content to reasoning model for summarization...")
    }).await?;
    
    // Generate summary using reasoning model with enhanced logging and streaming
    println!("🧠 Starting AI summarization process with streaming...");
    match stream_summarization_response(&webpage_content, url, &config, &mut response_msg, ctx).await {
        Ok(stats) => {
            println!("✅ Summary streaming completed successfully");
            println!("📊 Final stats - Total characters: {}, Messages: {}", stats.total_characters, stats.message_count);
        }
        Err(e) => {
            println!("❌ Summary generation failed: {}", e);
            response_msg.edit(ctx, |m| {
                m.content(&format!("❌ Failed to generate summary: {}\n\n**Possible issues:**\n• Reasoning model not responding\n• Content too large for model\n• API configuration issues\n• Network connectivity to AI server", e))
            }).await?;
        }
    }
    
    Ok(())
}

// Function to fetch webpage content with enhanced error handling and logging
async fn fetch_webpage_content(url: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    println!("🌐 Starting webpage fetch for URL: {}", url);
    
    // Create client with enhanced headers to avoid 403 errors
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8".parse().unwrap());
            headers.insert("Accept-Language", "en-US,en;q=0.5".parse().unwrap());
            headers.insert("Accept-Encoding", "gzip, deflate".parse().unwrap());
            headers.insert("DNT", "1".parse().unwrap());
            headers.insert("Connection", "keep-alive".parse().unwrap());
            headers.insert("Upgrade-Insecure-Requests", "1".parse().unwrap());
            headers.insert("Sec-Fetch-Dest", "document".parse().unwrap());
            headers.insert("Sec-Fetch-Mode", "navigate".parse().unwrap());
            headers.insert("Sec-Fetch-Site", "none".parse().unwrap());
            headers.insert("Cache-Control", "max-age=0".parse().unwrap());
            headers
        })
        .build()?;
    
    println!("🔄 Sending HTTP request to: {}", url);
    
    let response = client.get(url).send().await?;
    let status = response.status();
    
    println!("📊 HTTP Response Status: {}", status);
    
    if !status.is_success() {
        let error_msg = format!("HTTP error: {} - {}", status.as_u16(), status.canonical_reason().unwrap_or("Unknown"));
        println!("❌ HTTP Error: {}", error_msg);
        
        // Provide specific guidance for common errors
        let guidance = match status.as_u16() {
            403 => "This website is blocking automated requests. The site may have anti-bot protection.",
            404 => "The webpage was not found. Please check the URL.",
            429 => "Rate limited. The website is receiving too many requests.",
            500..=599 => "Server error on the website. Try again later.",
            _ => "Unknown HTTP error occurred."
        };
        
        return Err(format!("{}\n\nGuidance: {}", error_msg, guidance).into());
    }
    
    println!("✅ Successfully received response from server");
    
    let html_content = response.text().await?;
    println!("📄 Downloaded HTML content: {} characters", html_content.len());
    
    // Basic HTML cleanup - remove script tags, style tags, and HTML tags
    let cleaned_content = clean_html_content(&html_content);
    println!("🧹 Cleaned content: {} characters", cleaned_content.len());
    
    // Limit content length to prevent overwhelming the model
    if cleaned_content.len() > 15000 {
        println!("✂️ Content truncated from {} to 15000 characters", cleaned_content.len());
        let truncated = safe_truncate_unicode(&cleaned_content, 15000);
        Ok(format!("{}...\n\n[Content truncated due to length - original was {} characters]", truncated, cleaned_content.len()))
    } else {
        println!("✅ Content ready for processing: {} characters", cleaned_content.len());
        Ok(cleaned_content)
    }
}

// Function to safely truncate Unicode strings at character boundaries
fn safe_truncate_unicode(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    
    // Find the last character boundary before max_bytes
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    
    &s[..boundary]
}

// Function to clean HTML content
fn clean_html_content(html: &str) -> String {
    let mut result = html.to_string();
    
    // Remove script tags and their content
    while let Some(start) = result.find("<script") {
        if let Some(end) = result[start..].find("</script>") {
            result.replace_range(start..start + end + 9, "");
        } else {
            break;
        }
    }
    
    // Remove style tags and their content
    while let Some(start) = result.find("<style") {
        if let Some(end) = result[start..].find("</style>") {
            result.replace_range(start..start + end + 8, "");
        } else {
            break;
        }
    }
    
    // Remove HTML tags
    let mut cleaned = String::new();
    let mut inside_tag = false;
    
    for ch in result.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => cleaned.push(ch),
            _ => {}
        }
    }
    
    // Clean up whitespace
    let cleaned = cleaned
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    
    // Remove multiple consecutive newlines
    let mut result = String::new();
    let mut newline_count = 0;
    
    for ch in cleaned.chars() {
        if ch == '\n' {
            newline_count += 1;
            if newline_count <= 2 {
                result.push(ch);
            }
        } else {
            newline_count = 0;
            result.push(ch);
        }
    }
    
    result.trim().to_string()
}

// Multi-path text file loading function with robust error handling
async fn load_text_file_multi_path(
    filename: &str,
    fallback_filename: &str,
    description: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let paths = [
        filename.to_string(),
        format!("../{}", filename),
        format!("../../{}", filename),
        format!("src/{}", filename),
        fallback_filename.to_string(),
        format!("../{}", fallback_filename),
        format!("../../{}", fallback_filename),
        format!("src/{}", fallback_filename),
    ];
    
    for path in &paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                // Remove BOM if present
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                println!("✅ {} loaded from: {}", description, path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    println!("⚠️ {} not found, using built-in default", description);
    
    // Return default summarization prompt if no file found
    let default_prompt = "You are an expert content summarizer. Your task is to analyze the provided webpage content and create a comprehensive, well-structured summary.\n\n\
        Instructions:\n\
        1. **Identify the main topic** and key points from the content\n\
        2. **Summarize the key information** in a clear, logical structure\n\
        3. **Highlight important facts, findings, or conclusions**\n\
        4. **Use Discord formatting** with **bold** for headings and key terms\n\
        5. **Keep the summary comprehensive but concise** (aim for 800-1500 characters)\n\
        6. **Focus on the most valuable information** for the reader\n\
        7. **Organize content logically** with clear structure\n\n\
        Provide a summary that gives readers a clear understanding of the content without needing to read the full webpage.";
    
    Ok(default_prompt.to_string())
}

// Function to load summarization system prompt with multi-path fallback
async fn load_summarization_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    load_text_file_multi_path(
        "summarization_prompt.txt",
        "example_summarization_prompt.txt",
        "Summarization system prompt"
    ).await
}

// Function to stream summarization response using reasoning model with enhanced RAG
async fn stream_summarization_response(
    content: &str,
    url: &str,
    config: &LMConfig,
    initial_msg: &mut Message,
    ctx: &Context,
) -> Result<StreamingStats, Box<dyn std::error::Error + Send + Sync>> {
    println!("🤖 Loading summarization prompt...");
    
    // Load system prompt from file with fallback
    let system_prompt = load_summarization_prompt().await?;
    
    println!("🔄 Preparing content for RAG processing...");
    
    // Enhanced RAG processing - chunk content if too large
    let processed_content = if content.len() > 10000 {
        println!("📄 Large content detected, applying RAG chunking...");
        
        // Extract key sections for better summarization
        let chunks = chunk_content_for_rag(content, 5000);
        println!("✂️ Content divided into {} chunks for RAG processing", chunks.len());
        
        // Process chunks and combine most relevant parts
        let mut combined_content = String::new();
        for (i, chunk) in chunks.iter().enumerate() {
            combined_content.push_str(&format!("--- Section {} ---\n{}\n\n", i + 1, chunk));
        }
        combined_content
    } else {
        content.to_string()
    };
    
    println!("📝 Creating user prompt for summarization...");
    
    let user_prompt = format!(
        "Please summarize this webpage content from {}:\n\n---\n\n{}",
        url, processed_content
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
    
    // Use reasoning model for summarization
    let model = &config.default_reason_model;
    println!("🧠 Using reasoning model: {}", model);
    
    let request = ChatRequest {
        model: model.clone(),
        messages,
        temperature: 0.3, // Lower temperature for more focused summaries
        max_tokens: 1000,  // Limit tokens for Discord compatibility
        stream: true,      // Enable streaming
    };
    
    println!("📡 Sending streaming request to reasoning model...");
    
    // Make streaming API request
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.timeout))
        .build()?;
    
    let response = client
        .post(&format!("{}/v1/chat/completions", config.base_url))
        .json(&request)
        .send()
        .await?;
    
    let status = response.status();
    println!("📊 AI API Response Status: {}", status);
    
    if !status.is_success() {
        let error_msg = format!("AI API request failed: {}", status);
        println!("❌ {}", error_msg);
        return Err(error_msg.into());
    }
    
    println!("✅ Starting to stream response from reasoning model");
    
    // Initialize message state
    let mut state = MessageState {
        current_content: format!("📄 **Webpage Summary**\n\n*Generating summary...*\n\n*Source: <{}>*", url),
        current_message: initial_msg.clone(),
        message_index: 1,
        char_limit: config.max_discord_message_length - config.response_format_padding,
    };
    
    // Process streaming response
    let mut stats = StreamingStats {
        total_characters: 0,
        message_count: 1,
    };
    
    let mut accumulated_content = String::new();
    let mut last_update = std::time::Instant::now();
    
    // Update initial message
    state.current_message.edit(ctx, |m| {
        m.content(&state.current_content)
    }).await?;
    
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    
    while let Some(item) = futures_util::stream::StreamExt::next(&mut stream).await {
        match item {
            Ok(chunk) => {
                if let Ok(text) = String::from_utf8(chunk.to_vec()) {
                    buffer.push_str(&text);
                    
                    // Process complete lines
                    while let Some(line_end) = buffer.find('\n') {
                        let line = buffer[..line_end].trim().to_string();
                        buffer.drain(..line_end + 1);
                        
                        if line.starts_with("data: ") {
                            let json_str = &line[6..];
                            
                            if json_str == "[DONE]" {
                                println!("🏁 Streaming completed");
                                break;
                            }
                            
                            if let Ok(response) = serde_json::from_str::<ChatResponse>(json_str) {
                                if let Some(choice) = response.choices.first() {
                                    if let Some(delta) = &choice.delta {
                                        if let Some(content) = &delta.content {
                                            accumulated_content.push_str(content);
                                            stats.total_characters += content.len();
                                            
                                            // Update Discord message periodically (every 0.8 seconds)
                                            if last_update.elapsed() >= Duration::from_millis(800) {
                                                update_summary_message(&mut state, &accumulated_content, ctx, config, url).await?;
                                                last_update = std::time::Instant::now();
                                            }
                                        }
                                    }
                                    
                                    if choice.finish_reason.is_some() {
                                        println!("🏁 Streaming finished with reason: {:?}", choice.finish_reason);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("❌ Error reading stream: {}", e);
                break;
            }
        }
    }
    
    // Final message update
    finalize_summary_message(&mut state, &accumulated_content, ctx, config, url).await?;
    
    println!("📊 Streaming completed - Total characters: {}, Messages: {}", stats.total_characters, stats.message_count);
    
    Ok(stats)
}

// Function to update Discord message during streaming
async fn update_summary_message(
    state: &mut MessageState,
    new_content: &str,
    ctx: &Context,
    _config: &LMConfig,
    url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let formatted_content = format!("📄 **Webpage Summary**\n\n{}\n\n*Source: <{}>*", new_content, url);
    
    // Check if we need to create a new message
    if formatted_content.len() > state.char_limit {
        // Current message is getting too long, create a new one
        let max_content_length = std::cmp::min(new_content.len(), state.char_limit - 100);
        let summary_part = safe_truncate_unicode(new_content, max_content_length);
        let final_content = format!("📄 **Webpage Summary (Part {})**\n\n{}\n\n*Source: <{}>*", state.message_index, summary_part, url);
        
        state.current_message.edit(ctx, |m| {
            m.content(&final_content)
        }).await?;
        
        // Create new message for remaining content
        state.message_index += 1;
        let remaining_content = &new_content[summary_part.len()..];
        if !remaining_content.is_empty() {
            let new_msg_content = format!("📄 **Webpage Summary (Part {})**\n\n{}", state.message_index, remaining_content);
            let new_msg = state.current_message.channel_id.say(ctx, &new_msg_content).await?;
            state.current_message = new_msg;
            state.current_content = new_msg_content;
        }
    } else {
        // Update current message
        state.current_content = formatted_content.clone();
        state.current_message.edit(ctx, |m| {
            m.content(&formatted_content)
        }).await?;
    }
    
    Ok(())
}

// Function to finalize the message after streaming is complete
async fn finalize_summary_message(
    state: &mut MessageState,
    final_content: &str,
    ctx: &Context,
    _config: &LMConfig,
    url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let formatted_content = if state.message_index > 1 {
        format!("📄 **Webpage Summary (Part {})**\n\n{}\n\n*Source: <{}>*", state.message_index, final_content, url)
    } else {
        format!("📄 **Webpage Summary**\n\n{}\n\n*Source: <{}>*", final_content, url)
    };
    
    // Final update to current message
    state.current_message.edit(ctx, |m| {
        m.content(&formatted_content)
    }).await?;
    
    println!("✅ Summary finalized in {} message(s)", state.message_index);
    
    Ok(())
}

// Function to chunk content for RAG processing
fn chunk_content_for_rag(content: &str, chunk_size: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    
    for paragraph in content.split('\n') {
        if current_chunk.len() + paragraph.len() > chunk_size && !current_chunk.is_empty() {
            chunks.push(current_chunk.clone());
            current_chunk = String::new();
        }
        
        if !current_chunk.is_empty() {
            current_chunk.push('\n');
        }
        current_chunk.push_str(paragraph);
    }
    
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }
    
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_clean_html_content() {
        let html = r#"<html><head><title>Test</title><script>alert('test');</script></head><body><h1>Hello</h1><p>World</p></body></html>"#;
        let cleaned = clean_html_content(html);
        assert!(cleaned.contains("Test"));
        assert!(cleaned.contains("Hello"));
        assert!(cleaned.contains("World"));
        assert!(!cleaned.contains("script"));
        assert!(!cleaned.contains("alert"));
    }
    
    #[test]
    fn test_html_tag_removal() {
        let html = "<p>This is <strong>bold</strong> text</p>";
        let cleaned = clean_html_content(html);
        assert_eq!(cleaned, "This is bold text");
    }
    
    #[test]
    fn test_chunk_content_for_rag() {
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6";
        let chunks = chunk_content_for_rag(content, 15);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.len() <= 20)); // Allow some flexibility
    }
    
    #[test]
    fn test_script_and_style_removal() {
        let html = r#"<html><head><script>alert('test');</script><style>body{color:red;}</style></head><body>Content</body></html>"#;
        let cleaned = clean_html_content(html);
        assert!(cleaned.contains("Content"));
        assert!(!cleaned.contains("alert"));
        assert!(!cleaned.contains("color:red"));
    }
    
    #[test]
    fn test_safe_truncate_unicode() {
        // Test with ASCII text
        let ascii_text = "Hello, world!";
        assert_eq!(safe_truncate_unicode(ascii_text, 5), "Hello");
        assert_eq!(safe_truncate_unicode(ascii_text, 50), ascii_text);
        
        // Test with Unicode text (emojis and accented characters)
        let unicode_text = "Hello 👋 café 🌟";
        let truncated = safe_truncate_unicode(unicode_text, 10);
        assert!(truncated.len() <= 10);
        assert!(unicode_text.starts_with(truncated));
        
        // Test edge case: truncation point in middle of multi-byte character
        let emoji_text = "🌟🌟🌟🌟🌟"; // Each emoji is 4 bytes
        let truncated = safe_truncate_unicode(emoji_text, 6); // Should stop at 4 bytes (1 emoji)
        assert_eq!(truncated, "🌟");
        
        // Test empty string
        assert_eq!(safe_truncate_unicode("", 10), "");
    }
} 