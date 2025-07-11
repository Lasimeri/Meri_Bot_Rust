use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};
use crate::search::{LMConfig, ChatMessage, load_lm_config};
use reqwest;
use std::time::Duration;
use std::fs;
use std::process::Command;
use uuid::Uuid;
use log::{info, warn, error, debug};
use serde::Deserialize;
use regex::Regex;
use crate::search::chat_completion;
use std::time::Instant;

// SSE response structures
#[derive(Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[command]
#[aliases("summarize", "webpage")]
pub async fn sum(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let start_time = std::time::Instant::now();
    info!("📺 Sum command initiated by user {} ({}) in channel {}", 
          msg.author.name, msg.author.id, msg.channel_id);
    
    let url = args.message().trim();
    debug!("🔗 Received URL: {}", url);
    
    if url.is_empty() {
        warn!("❌ Empty URL provided by user {} ({})", msg.author.name, msg.author.id);
        msg.reply(ctx, "Please provide a URL to summarize!\n\n**Usage:** `^sum <url>`").await?;
        return Ok(());
    }
    
    if !url.starts_with("http://") && !url.starts_with("https://") {
        warn!("❌ Invalid URL format provided: {}", url);
        msg.reply(ctx, "Please provide a valid URL starting with `http://` or `https://`").await?;
        return Ok(());
    }
    
    // Load LM configuration from lmapiconf.txt BEFORE starting typing indicator
    debug!("🔧 Loading LM configuration from lmapiconf.txt...");
    
    let config = match load_lm_config().await {
        Ok(cfg) => {
            info!("✅ LM configuration loaded successfully");
            debug!("🧠 Using reasoning model: {}", cfg.default_reason_model);
            debug!("🌐 API endpoint: {}", cfg.base_url);
            cfg
        },
        Err(e) => {
            error!("❌ Failed to load LM configuration: {}", e);
            msg.reply(ctx, &format!("Failed to load LM configuration: {}\n\n**Setup required:** Ensure `lmapiconf.txt` is properly configured with your reasoning model.", e)).await?;
            return Ok(());
        }
    };
    
    debug!("🔧 Configuration loaded, proceeding with next steps");
    
    // Start typing indicator AFTER config is loaded
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    
    let is_youtube = url.contains("youtube.com/") || url.contains("youtu.be/");
    info!("🎯 Processing {} URL: {}", if is_youtube { "YouTube" } else { "webpage" }, url);
    
    // Create response message
    let mut response_msg = msg.reply(ctx, "🔄 Fetching content...").await?;
    debug!("✅ Initial Discord message sent successfully");
    
    // Fetch content
    info!("🌐 Starting content fetching process...");
    let content = if is_youtube {
        match fetch_youtube_transcript(url).await {
            Ok(transcript) => {
                info!("✅ YouTube transcript fetched successfully: {} characters", transcript.len());
                debug!("📝 Transcript preview: {}", &transcript[..std::cmp::min(200, transcript.len())]);
                transcript
            },
            Err(e) => {
                error!("❌ Failed to fetch YouTube transcript: {}", e);
                response_msg.edit(ctx, |m| {
                    m.content(format!("❌ Failed to fetch YouTube transcript: {}", e))
                }).await?;
                return Ok(());
            }
        }
    } else {
        match fetch_webpage_content(url).await {
            Ok(content) => {
                info!("✅ Webpage content fetched successfully: {} characters", content.len());
                debug!("📄 Content preview: {}", &content[..std::cmp::min(200, content.len())]);
                content
            },
            Err(e) => {
                error!("❌ Failed to fetch webpage content: {}", e);
                response_msg.edit(ctx, |m| {
                    m.content(format!("❌ Failed to fetch webpage: {}", e))
                }).await?;
                return Ok(());
            }
        }
    };
    
    // Update status
    debug!("📝 Updating Discord message to show AI processing...");
    response_msg.edit(ctx, |m| {
        m.content("🤖 Generating summary...")
    }).await?;
    
    // Stream the summary
    info!("🧠 Starting AI summarization process with streaming...");
    let processing_start = std::time::Instant::now();
    match stream_summary(&content, url, &config, &mut response_msg, ctx, is_youtube).await {
        Ok(_) => {
            let processing_time = processing_start.elapsed();
            info!("✅ Summary streaming completed successfully in {:.2}s", processing_time.as_secs_f64());
        },
        Err(e) => {
            error!("❌ Summary generation failed: {}", e);
            response_msg.edit(ctx, |m| {
                m.content(format!("❌ Failed to generate summary: {}", e))
            }).await?;
        }
    }
    
    let total_time = start_time.elapsed();
    info!("⏱️ Sum command completed in {:.2}s for user {} ({})", 
          total_time.as_secs_f64(), msg.author.name, msg.author.id);
    
    Ok(())
}

// Load summarization system prompt with multi-path fallback (like lm command)
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

// Simplified YouTube transcript fetcher
async fn fetch_youtube_transcript(url: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let temp_file = format!("yt_transcript_{}", Uuid::new_v4());
    debug!("🎥 Attempting to fetch YouTube transcript using yt-dlp...");
    debug!("📍 URL: {}", url);
    debug!("📁 Temp file base: {}", temp_file);
    
    // Check if yt-dlp is available and get version
    debug!("🔍 Checking yt-dlp version...");
    let version_output = Command::new("yt-dlp")
        .arg("--version")
        .output()
        .map_err(|e| {
            error!("❌ yt-dlp is not installed or not in PATH: {}", e);
            "yt-dlp is not installed. Please install yt-dlp to use YouTube summarization."
        })?;
    
    if !version_output.status.success() {
        error!("❌ yt-dlp version check failed");
        return Err("yt-dlp is not working properly".into());
    }
    
    let version_str = String::from_utf8_lossy(&version_output.stdout);
    debug!("✅ yt-dlp version: {}", version_str.trim());
    
    // Try to download automatic subtitles
    debug!("🔄 Running yt-dlp command for automatic subtitles...");
    let mut command = Command::new("yt-dlp");
    command
        .arg("--write-auto-sub")
        .arg("--sub-langs").arg("en")
        .arg("--sub-format").arg("vtt")
        .arg("--skip-download")
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg("--extract-audio")
        .arg("--audio-format").arg("mp3")
        .arg("--output").arg(&temp_file)
        .arg(url);
    
    let output = command.output()?;
    
    debug!("yt-dlp command exit status: {}", output.status);
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        error!("❌ yt-dlp failed - stdout: {}", stdout);
        error!("❌ yt-dlp failed - stderr: {}", stderr);
        
        // Check for common error patterns and provide helpful messages
        if stderr.contains("Did not get any data blocks") {
            return Err("YouTube subtitles extraction failed: 'Did not get any data blocks'. This is usually caused by YouTube's anti-bot measures or an outdated yt-dlp version. Try updating yt-dlp with: yt-dlp -U".into());
        }
        
        if stderr.contains("Sign in to confirm you're not a bot") {
            return Err("YouTube is blocking requests: 'Sign in to confirm you're not a bot'. This is a temporary YouTube restriction. Try again later or use a different video.".into());
        }
        
        if stderr.contains("Private video") || stderr.contains("Video unavailable") {
            return Err("Video is private or unavailable. Please check the URL and try again.".into());
        }
        
        if stderr.contains("No subtitles") || stderr.contains("no automatic captions") {
            return Err("This video has no automatic captions or subtitles available.".into());
        }
        
        return Err(format!("yt-dlp failed to extract subtitles: {}", stderr).into());
    }
    
    // Look for the subtitle file
    let vtt_file = format!("{}.en.vtt", temp_file);
    debug!("📄 Looking for subtitle file: {}", vtt_file);
    
    if !std::path::Path::new(&vtt_file).exists() {
        error!("❌ Subtitle file not found: {}", vtt_file);
        return Err("Subtitle file was not created by yt-dlp. The video may not have automatic captions available.".into());
    }
    
    let content = fs::read_to_string(&vtt_file)?;
    debug!("📖 Read subtitle file: {} characters", content.len());
    
    // Clean up the file
    match fs::remove_file(&vtt_file) {
        Ok(_) => debug!("🗑️ Cleaned up temp file: {}", vtt_file),
        Err(e) => warn!("⚠️ Failed to clean up temp file {}: {}", vtt_file, e),
    }
    
    // Check if content is valid
    if content.trim().is_empty() {
        return Err("Downloaded subtitle file is empty".into());
    }
    
    if !content.contains("WEBVTT") {
        return Err("Downloaded file is not a valid VTT subtitle file".into());
    }
    
    // Clean VTT content
    let cleaned = clean_vtt_content(&content);
    debug!("✅ VTT content cleaned: {} characters", cleaned.len());
    
    if cleaned.trim().is_empty() {
        return Err("No readable text found in subtitle file after cleaning".into());
    }
    
    Ok(cleaned)
}

// Simple VTT cleaner
fn clean_vtt_content(vtt: &str) -> String {
    debug!("🧹 Cleaning VTT content...");    
    let mut lines = Vec::new();
    
    for line in vtt.lines() {
        let line = line.trim();
        
        // Skip headers, timestamps, and empty lines
        if line.is_empty() || 
           line.starts_with("WEBVTT") ||
           line.contains("-->") ||
           line.chars().all(|c| c.is_numeric() || c == ':' || c == '.') {
            continue;
        }
        
        // Clean tags
        let cleaned = line
            .replace("<c>", "")
            .replace("</c>", "")
            .trim()
            .to_string();
            
        if !cleaned.is_empty() {
            lines.push(cleaned);
        }
    }
    
    let result = lines.join(" ");
    debug!("🧹 VTT cleaning complete: {} lines -> {} characters", lines.len(), result.len());
    result
}

// Simple webpage fetcher
async fn fetch_webpage_content(url: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    debug!("🌐 Starting webpage fetch for URL: {}", url);
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;
    
    debug!("📡 Sending HTTP request...");
    let response = client.get(url).send().await?;
    let status = response.status();
    debug!("📡 HTTP Response Status: {}", status);
    
    if !response.status().is_success() {
        error!("❌ HTTP error: {}", status);
        return Err(format!("HTTP error: {}", response.status()).into());
    }
    
    let html = response.text().await?;
    debug!("📄 Downloaded HTML content: {} characters", html.len());
    
    // Basic HTML cleaning
    let cleaned = clean_html(&html);
    debug!("🧹 HTML content cleaned: {} characters", cleaned.len());
    
    Ok(cleaned)
}

// Simple HTML cleaner
fn clean_html(html: &str) -> String {
    debug!("🧹 Cleaning HTML content...");
    
    // Remove script and style tags
    let mut result = html.to_string();
    
    // Remove script tags
    while let Some(start) = result.find("<script") {
        if let Some(end) = result[start..].find("</script>") {
            result.replace_range(start..start + end + 9, "");
        } else {
            break;
        }
    }
    
    // Remove style tags
    while let Some(start) = result.find("<style") {
        if let Some(end) = result[start..].find("</style>") {
            result.replace_range(start..start + end + 8, "");
        } else {
            break;
        }
    }
    
    // Remove all HTML tags
    let tag_regex = regex::Regex::new(r"<[^>]+>").unwrap();
    let cleaned = tag_regex.replace_all(&result, " ");
    
    // Clean whitespace
    let final_result: String = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(15000)
        .collect();
    
    debug!("🧹 HTML cleaning complete: {} -> {} characters", html.len(), final_result.len());
    final_result
}

// Stream summary using SSE (like lm command approach)
async fn stream_summary(
    content: &str,
    url: &str,
    config: &LMConfig,
    msg: &mut Message,
    ctx: &Context,
    is_youtube: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    
    debug!("🤖 Preparing AI request...");    
    
    // Load appropriate system prompt from files
    let system_prompt = if is_youtube {
        load_youtube_summarization_prompt().await?
    } else {
        load_summarization_prompt().await?
    };
    
    // Truncate content to prevent context overflow
    let max_content_length = 20000;
    let truncated_content = if content.len() > max_content_length {
        format!("{} [Content truncated due to length]", &content[0..max_content_length])
    } else {
        content.to_string()
    };
    
    let user_prompt = format!(
        "Please summarize this {} from {}:\n\n{}",
        if is_youtube { "YouTube video transcript" } else { "webpage content" },
        url,
        truncated_content
    );
    
    debug!("📝 System prompt length: {} characters", system_prompt.len());
    debug!("📝 User prompt length: {} characters", user_prompt.len());
    
    let chunk_size = 8000;
    let mut chunk_summaries = Vec::new();
    let request_payload;
    
    if truncated_content.len() > chunk_size {
        debug!("📄 Content too long ({} chars), using map-reduce RAG summarization", truncated_content.len());
        let chunks: Vec<&str> = truncated_content.as_bytes().chunks(chunk_size).map(|c| std::str::from_utf8(c).unwrap()).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            debug!("🤖 Summarizing chunk {} of {}", i+1, chunks.len());
            let chunk_prompt = format!("Briefly summarize this content chunk:\n\n{}", chunk);
            let chunk_messages = vec![
                ChatMessage { role: "system".to_string(), content: "You are a concise summarizer.".to_string() },
                ChatMessage { role: "user".to_string(), content: chunk_prompt }
            ];
            let chunk_summary = chat_completion(chunk_messages, &config.default_reason_model, config, Some(500)).await?;
            chunk_summaries.push(chunk_summary);
        }
        // Combine chunk summaries for final prompt
        let combined = chunk_summaries.join("\n\n");
        let final_user_prompt = format!("Generate a comprehensive summary from these chunk summaries of the {} from {}:\n\n{}", if is_youtube { "YouTube video" } else { "webpage" }, url, combined);
        let final_messages = vec![
            ChatMessage { role: "system".to_string(), content: system_prompt.clone() },
            ChatMessage { role: "user".to_string(), content: final_user_prompt }
        ];
        request_payload = serde_json::json!(
            {
                "model": config.default_reason_model,
                "messages": final_messages,
                "temperature": config.default_temperature,
                "max_tokens": config.default_max_tokens,
                "stream": true
            }
        );
    } else {
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
        request_payload = serde_json::json!(
            {
                "model": config.default_reason_model,
                "messages": messages,
                "temperature": config.default_temperature,
                "max_tokens": config.default_max_tokens,
                "stream": true
            }
        );
    }
    
    // Create reqwest client
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    
    // Send streaming request
    let mut response = client
        .post(&format!("{}/v1/chat/completions", config.base_url))
        .json(&request_payload)
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(format!("API returned error: {}", response.status()).into());
    }
    
    let mut accumulated = String::new();
    let start_time = Instant::now();
    let mut last_update = Instant::now();
    
    while let Some(chunk) = response.chunk().await? {
        let chunk_str = String::from_utf8_lossy(&chunk);
        for line in chunk_str.lines() {
            if line.starts_with("data: ") {
                let data = &line[6..];
                if data == "[DONE]" {
                    break;
                }
                let stream_resp: StreamResponse = serde_json::from_str(data)?;
                if let Some(choice) = stream_resp.choices.get(0) {
                    if let Some(content) = &choice.delta.content {
                        accumulated.push_str(content);
                    }
                    if choice.finish_reason.is_some() {
                        break;
                    }
                }
            }
        }
        // Periodic update to Discord every 5 seconds
        if last_update.elapsed() > Duration::from_secs(5) {
            msg.edit(ctx, |m| m.content(format!("🤖 Generating summary... ({}s)", start_time.elapsed().as_secs()))).await?;
            last_update = Instant::now();
        }
    }
    
    // Strip <think> sections from full accumulated response
    let re = Regex::new(r"(?s)<think>.*?</think>").unwrap();
    let stripped = re.replace_all(&accumulated, "").to_string();
    
    debug!("📊 Final accumulated content: {} characters", stripped.len());
    
    // Final update
    let final_message = format!(
        "**{} Summary**\n\n{}\n\n*Source: <{}>*",
        if is_youtube { "YouTube Video" } else { "Webpage" },
        stripped.trim(),
        url
    );
    
    // Split if too long
    if final_message.len() > config.max_discord_message_length - config.response_format_padding {
        debug!("📄 Message too long, splitting into chunks...");        let chunks = split_message(&final_message, config.max_discord_message_length - config.response_format_padding);
        debug!("📄 Split into {} chunks", chunks.len());
        
        for (i, chunk) in chunks.iter().enumerate() {
            if i == 0 {
                msg.edit(ctx, |m| m.content(chunk)).await?;
            } else {
                msg.channel_id.say(ctx, chunk).await?;
            }
        }
    } else {
        msg.edit(ctx, |m| m.content(&final_message)).await?;
    }
    
    Ok(())
}

// Split long messages
fn split_message(content: &str, max_len: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    
    for line in content.lines() {
        if current.len() + line.len() + 1 > max_len && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current = String::new();
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }
    
    if !current.is_empty() {
        chunks.push(current.trim().to_string());
    }
    
    chunks
}

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
} 