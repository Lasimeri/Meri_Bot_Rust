// rank.rs - Content Ranking Command Module
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
// - Thinking tag filtering and buffered streaming (like reason.rs)
//
// Used by: main.rs (command registration), search.rs (for config)

use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};
use crate::commands::search::{LMConfig, ChatMessage};
use reqwest;
use std::time::Duration;
use std::fs;
use std::process::Command;
use uuid::Uuid;
use log::{info, warn, error, debug, trace};
use serde::{Deserialize, Serialize};

use futures_util::StreamExt;
use regex::Regex;
use once_cell::sync::Lazy;

// Compile regex once for better performance - matches <think> tags
// Used to filter out internal AI thoughts from streaming output
static THINKING_TAG_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<think>.*?</think>").unwrap()
});

// Structures for streaming API responses
// Used to parse streaming JSON chunks from the AI API
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

// API Request structure for ranking model
#[derive(Serialize)]
struct ChatRequest {
    model: String,              // Model name
    messages: Vec<ChatMessage>, // Conversation history
    temperature: f32,           // Sampling temperature
    max_tokens: i32,            // Max tokens to generate
    stream: bool,               // Whether to stream output
}

// Structure to track streaming statistics for ranking
#[derive(Debug)]
struct StreamingStats {
    total_characters: usize,    // Total characters streamed
    message_count: usize,       // Number of Discord messages sent
    filtered_characters: usize, // Characters filtered by <think> tag removal
}

// Structure to track current message state during streaming
struct MessageState {
    current_content: String,    // Accumulated content for current Discord message
    current_message: Message,   // Current Discord message object
    message_index: usize,       // Part number (for chunked output)
    char_limit: usize,          // Discord message length limit
}

// SSE response structures for streaming ranking analysis
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
#[aliases("rank", "analyze", "evaluate")]
/// Main ^rank command handler
/// Handles ranking and analysis of webpages and YouTube videos
/// Supports:
///   - ^rank <url> (webpage or YouTube)
pub async fn rank(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let start_time = std::time::Instant::now();
    let command_uuid = Uuid::new_v4();
    
    info!("📊 === RANK COMMAND STARTED ===");
    info!("🆔 Command UUID: {}", command_uuid);
    info!("👤 User: {} ({})", msg.author.name, msg.author.id);
    info!("📊 Channel: {} ({})", msg.channel_id, msg.channel_id.0);
    info!("📊 Guild: {:?}", msg.guild_id);
    info!("📊 Message ID: {}", msg.id);
    info!("📊 Timestamp: {:?}", msg.timestamp);
    
    // Enhanced logging for content ranking debugging
    log::info!("🔍 === RANK COMMAND DEBUG INFO ===");
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
        msg.reply(ctx, "Please provide a URL to rank and analyze!\n\n**Usage:** `^rank <url>`").await?;
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
        msg.reply(ctx, "Please provide a valid URL starting with `http://` or `https://`").await?;
        debug!("✅ Invalid URL error message sent");
        return Ok(());
    }
    debug!("✅ URL format validation passed");
    trace!("🔍 URL validation success: protocol={}, command_uuid={}", 
           if url.starts_with("https://") { "https" } else { "http" }, command_uuid);
    
    // Load LM configuration from lmapiconf.txt BEFORE starting typing indicator
    debug!("🔧 === CONFIGURATION LOADING ===");
    debug!("🔧 Loading LM configuration from lmapiconf.txt...");
    trace!("🔍 Configuration loading phase started: command_uuid={}", command_uuid);
    
    let config = match crate::commands::search::load_lm_config().await {
        Ok(cfg) => {
            info!("✅ === CONFIGURATION LOADED SUCCESSFULLY ===");
            info!("✅ LM configuration loaded successfully");
            debug!("🧠 Using default model: {}", cfg.default_model);
            debug!("🧠 Using ranking model: {}", cfg.default_ranking_model);
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
            msg.reply(ctx, &format!("Failed to load LM configuration: {}\n\n**Setup required:** Ensure `lmapiconf.txt` is properly configured with your reasoning model.", e)).await?;
            debug!("✅ Configuration error message sent");
            return Ok(());
        }
    };
    
    debug!("🔧 Configuration loaded successfully, proceeding with next steps");
    trace!("🔍 Configuration phase completed, moving to typing indicator: command_uuid={}", command_uuid);
    
    // Start typing indicator AFTER config is loaded
    debug!("⌨️ === TYPING INDICATOR ===");
    debug!("⌨️ Starting typing indicator...");
    trace!("🔍 Typing indicator request: channel_id={}, command_uuid={}", msg.channel_id.0, command_uuid);
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    debug!("✅ Typing indicator started successfully");
    trace!("🔍 Typing indicator phase completed: command_uuid={}", command_uuid);
    
    debug!("🔍 === URL TYPE DETECTION ===");
    debug!("🔍 Detecting URL type...");
    let is_youtube = url.contains("youtube.com/") || url.contains("youtu.be/");
    debug!("🔍 URL contains youtube.com/: {}", url.contains("youtube.com/"));
    debug!("🔍 URL contains youtu.be/: {}", url.contains("youtu.be/"));
    debug!("🔍 Final YouTube detection: {}", is_youtube);
    trace!("🔍 URL type detection details: contains_youtube_com={}, contains_youtu_be={}, is_youtube={}, command_uuid={}", 
           url.contains("youtube.com/"), url.contains("youtu.be/"), is_youtube, command_uuid);
    info!("🎯 === CONTENT TYPE DETECTED ===");
    info!("🎯 Processing {} URL: {}", if is_youtube { "YouTube" } else { "webpage" }, url);
    debug!("📊 URL type detection: YouTube = {}", is_youtube);
    
    // Create response message
    debug!("💬 === DISCORD MESSAGE CREATION ===");
    debug!("💬 Creating initial Discord response message...");
    trace!("🔍 Discord message creation: author={}, channel={}, command_uuid={}", msg.author.name, msg.channel_id, command_uuid);
    let mut response_msg = msg.reply(ctx, "🔄 Fetching content for ranking...").await?;
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

    let mut content = String::new();
    let subtitle_file_path = if is_youtube {
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
                
                // Read the subtitle file content for statistics
                debug!("📖 === SUBTITLE FILE READING ===");
                debug!("📖 Reading subtitle file for statistics...");
                match fs::read_to_string(&path) {
                    Ok(file_content) => {
                        debug!("📖 Subtitle file read successfully: {} characters", file_content.len());
                        debug!("📖 File content preview: {}", &file_content[..std::cmp::min(200, file_content.len())]);
                        trace!("🔍 Subtitle file read: path={}, length={}, command_uuid={}", path, file_content.len(), command_uuid);
                        
                        let cleaned_content = clean_vtt_content(&file_content);
                        debug!("🧹 === VTT CLEANING FOR STATISTICS ===");
                        debug!("🧹 Cleaning VTT content for statistics...");
                        debug!("📝 Original subtitle content: {} characters", file_content.len());
                        debug!("📝 Cleaned subtitle content: {} characters", cleaned_content.len());
                        debug!("📝 Content preview: {}", &cleaned_content[..std::cmp::min(200, cleaned_content.len())]);
                        debug!("📊 Subtitle statistics: {} characters, {} words", cleaned_content.len(), cleaned_content.split_whitespace().count());
                        trace!("🔍 VTT cleaning for statistics: original_length={}, cleaned_length={}, word_count={}, command_uuid={}", 
                               file_content.len(), cleaned_content.len(), cleaned_content.split_whitespace().count(), command_uuid);
                        content = cleaned_content;
                    },
                    Err(e) => {
                        warn!("⚠️ === SUBTITLE FILE READ ERROR ===");
                        warn!("⚠️ Could not read subtitle file for statistics: {}", e);
                        debug!("🔍 Subtitle file read error: path={}, error={}", path, e);
                        trace!("🔍 Subtitle file read error: path={}, error_type={}, command_uuid={}", 
                               path, std::any::type_name_of_val(&e), command_uuid);
                    }
                }
                Some(path)
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
                
                content = page_content;
                Some(html_file_path)
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
    trace!("🔍 Discord message update: changing content to '🤖 Generating ranking analysis...', command_uuid={}", command_uuid);
    response_msg.edit(ctx, |m| {
        m.content("🤖 Generating ranking analysis...")
    }).await?;
    debug!("✅ Discord message updated to show AI processing");
    trace!("🔍 Discord message update completed: command_uuid={}", command_uuid);
    
    // Stream the ranking analysis
    info!("🧠 === AI RANKING ANALYSIS PHASE ===");
    info!("🧠 Starting AI ranking analysis process with streaming...");
    debug!("🚀 AI ranking analysis phase initiated");
    
    let content_length = if let Some(ref path) = subtitle_file_path {
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
        debug!("📏 Content length from direct content: {} characters", content.len());
        trace!("🔍 Content length calculation: direct_length={}, command_uuid={}", content.len(), command_uuid);
        content.len()
    };
    
    trace!("🔍 AI ranking analysis phase: content_length={}, url={}, is_youtube={}, command_uuid={}", 
           content_length, url, is_youtube, command_uuid);
    let processing_start = std::time::Instant::now();
    debug!("⏱️ AI processing start time: {:?}", processing_start);
    
    match stream_ranking_analysis(&content, url, &config, &mut response_msg, ctx, is_youtube, subtitle_file_path.as_deref()).await {
        Ok(stats) => {
            let processing_time = processing_start.elapsed();
            info!("✅ === AI RANKING ANALYSIS SUCCESS ===");
            info!("✅ Ranking analysis streaming completed successfully in {:.2}s", processing_time.as_secs_f64());
            debug!("📊 AI processing statistics: {:.2}s processing time", processing_time.as_secs_f64());
            debug!("📊 Processing time in milliseconds: {} ms", processing_time.as_millis());
            debug!("📊 Streaming stats: {} total chars, {} messages, {} filtered chars", 
                   stats.total_characters, stats.message_count, stats.filtered_characters);
            trace!("🔍 AI ranking analysis success: processing_time_ms={}, content_length={}, total_chars={}, messages={}, filtered_chars={}, command_uuid={}", 
                   processing_time.as_millis(), content_length, stats.total_characters, stats.message_count, stats.filtered_characters, command_uuid);
        },
        Err(e) => {
            error!("❌ === AI RANKING ANALYSIS ERROR ===");
            error!("❌ Ranking analysis generation failed: {}", e);
            debug!("🔍 AI ranking analysis error details: {:?}", e);
            debug!("🔍 AI ranking analysis error type: {:?}", std::any::type_name_of_val(&e));
            trace!("🔍 AI ranking analysis error: error_type={}, command_uuid={}", 
                   std::any::type_name_of_val(&e), command_uuid);
            response_msg.edit(ctx, |m| {
                m.content(format!("❌ Failed to generate ranking analysis: {}", e))
            }).await?;
            debug!("✅ AI ranking analysis error message sent to Discord");
        }
    }
    
    let total_time = start_time.elapsed();
    info!("⏱️ === COMMAND COMPLETION ===");
    info!("⏱️ Rank command completed in {:.2}s for user {} ({})", 
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
    log::info!("🎯 === RANK COMMAND COMPLETION SUMMARY ===");
    log::info!("🎯 Command UUID: {}", command_uuid);
    log::info!("🎯 Total execution time: {:.2}s", total_time.as_secs_f64());
    log::info!("🎯 Content type: {}", if is_youtube { "YouTube" } else { "Webpage" });
    log::info!("🎯 Content length: {} characters", content_length);
    log::info!("🎯 User: {} ({})", msg.author.name, msg.author.id);
    log::info!("🎯 Channel: {} ({})", msg.channel_id, msg.channel_id.0);
    log::info!("🎯 URL: {}", url);
    log::info!("🎯 Processing method: {}", if subtitle_file_path.is_some() { "RAG with file" } else { "Direct processing" });
    
    // Clean up temporary files
    if let Some(file_path) = subtitle_file_path {
        // Log file path before cleanup
        log::info!("🎯 File path used: {}", file_path);
        
        debug!("🧹 === TEMPORARY FILE CLEANUP ===");
        debug!("🧹 Cleaning up temporary file: {}", file_path);
        
        // Enhanced logging for cleanup process
        log::info!("🧹 === TEMPORARY FILE CLEANUP STARTED ===");
        log::info!("🧹 File path: {}", file_path);
        log::info!("🧹 File exists: {}", std::path::Path::new(&file_path).exists());
        log::info!("🧹 Command UUID: {}", command_uuid);
        
        match fs::remove_file(&file_path) {
            Ok(_) => {
                debug!("✅ Temporary file cleaned up successfully: {}", file_path);
                trace!("🔍 File cleanup success: path={}, command_uuid={}", file_path, command_uuid);
                
                // Enhanced logging for successful cleanup
                log::info!("✅ === TEMPORARY FILE CLEANUP SUCCESS ===");
                log::info!("✅ File removed: {}", file_path);
                log::info!("✅ File no longer exists: {}", !std::path::Path::new(&file_path).exists());
            },
            Err(e) => {
                warn!("⚠️ Failed to clean up temporary file: {} - {}", file_path, e);
                debug!("🔍 File cleanup error: path={}, error={}", file_path, e);
                trace!("🔍 File cleanup error: path={}, error_type={}, command_uuid={}", 
                       file_path, std::any::type_name_of_val(&e), command_uuid);
                
                // Enhanced logging for cleanup failure
                log::error!("❌ === TEMPORARY FILE CLEANUP FAILED ===");
                log::error!("❌ File path: {}", file_path);
                log::error!("❌ Error: {}", e);
                log::error!("❌ Error type: {}", std::any::type_name_of_val(&e));
                log::error!("❌ File still exists: {}", std::path::Path::new(&file_path).exists());
            }
        }
        
        // Log cleanup status
        log::info!("🎯 File cleaned up: {}", !std::path::Path::new(&file_path).exists());
    }
    
    log::info!("🎯 Status: SUCCESS");
    
    Ok(())
} 

// Load ranking analysis system prompt with multi-path fallback
// Loads ranking_analysis_prompt.txt from multiple locations, returns prompt string or fallback
async fn load_ranking_analysis_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let prompt_paths = [
        "rank_system_prompt.txt",
        "ranking_analysis_prompt.txt",
        "../rank_system_prompt.txt",
        "../ranking_analysis_prompt.txt",
        "../../rank_system_prompt.txt",
        "../../ranking_analysis_prompt.txt",
        "src/rank_system_prompt.txt",
        "src/ranking_analysis_prompt.txt",
        "example_ranking_analysis_prompt.txt",
        "../example_ranking_analysis_prompt.txt",
        "../../example_ranking_analysis_prompt.txt",
        "src/example_ranking_analysis_prompt.txt",
    ];
    
    for path in &prompt_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                // Remove BOM if present
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                debug!("📊 Qwen3 reranking prompt loaded from: {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    // Fallback prompt if no file found
    debug!("📊 Using built-in fallback Qwen3 reranking prompt");
    Ok("You are a Qwen3 Reranking model (qwen3-reranker-4b) specialized in content analysis and ranking. Your task is to evaluate and rank content across multiple dimensions: Content Quality (1-10), Relevance (1-10), Engagement Potential (1-10), Educational Value (1-10), and Technical Excellence (1-10). Provide detailed analysis with specific examples, strengths, areas for improvement, and an overall recommendation.".to_string())
}

// Load YouTube ranking analysis system prompt with multi-path fallback
// Loads youtube_ranking_analysis_prompt.txt from multiple locations, returns prompt string or fallback
async fn load_youtube_ranking_analysis_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let prompt_paths = [
        "youtube_ranking_analysis_prompt.txt",
        "../youtube_ranking_analysis_prompt.txt",
        "../../youtube_ranking_analysis_prompt.txt",
        "src/youtube_ranking_analysis_prompt.txt",
        "example_youtube_ranking_analysis_prompt.txt",
        "../example_youtube_ranking_analysis_prompt.txt",
        "../../example_youtube_ranking_analysis_prompt.txt",
        "src/example_youtube_ranking_analysis_prompt.txt",
    ];
    
    for path in &prompt_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                // Remove BOM if present
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                debug!("📺 YouTube ranking analysis prompt loaded from: {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    // Fallback prompt if no file found
    debug!("📺 Using built-in fallback YouTube ranking analysis prompt");
    Ok("You are an expert YouTube content analyst and evaluator. Analyze the provided YouTube video content and rank different aspects including educational value, entertainment quality, production value, accuracy, engagement potential, and overall viewer satisfaction. Provide detailed ratings (1-10 scale) and explanations for each aspect, along with specific examples from the content. Use clear formatting and provide actionable insights for content creators and viewers.".to_string())
}

// Enhanced YouTube transcript fetcher using yt-dlp with detailed logging
// Downloads and cleans VTT subtitles for a given YouTube URL
async fn fetch_youtube_transcript(url: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let temp_file = format!("yt_transcript_{}", Uuid::new_v4());
    let process_uuid = Uuid::new_v4();
    
    info!("🎥 === YOUTUBE TRANSCRIPT EXTRACTION STARTED ===");
    info!("🆔 Process UUID: {}", process_uuid);
    info!("📍 Target URL: {}", url);
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
            "yt-dlp is not installed. Please install yt-dlp to use YouTube ranking analysis."
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
        trace!("🔍 Error pattern analysis: data_blocks={}, bot_confirmation={}, private_video={}, video_unavailable={}, rate_limit={}, no_subtitles={}, not_available={}, process_uuid={}", 
               last_error.contains("Did not get any data blocks"), last_error.contains("Sign in to confirm you're not a bot"), 
               last_error.contains("Private video"), last_error.contains("Video unavailable"), 
               last_error.contains("429") || last_error.contains("Too Many Requests"),
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
    
    // Return the path to the subtitle file for RAG processing
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

// Simple webpage fetcher
// Downloads and cleans HTML content for a given URL
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
    debug!("🔧 Creating HTTP client with timeout...");
    trace!("🔍 HTTP client creation started: fetch_uuid={}", fetch_uuid);
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;
    
    debug!("✅ HTTP client created successfully");
    debug!("🔧 Timeout: 30 seconds");
    debug!("🔧 User agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36");
    trace!("🔍 HTTP client created: timeout=30s, fetch_uuid={}", fetch_uuid);
    
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

// Simple and reliable thinking tag filter
// Removes all <think>...</think> blocks from the content
fn filter_thinking_tags(content: &str) -> String {
    // Use pre-compiled regex to remove thinking tags and their content
    let filtered = THINKING_TAG_REGEX.replace_all(content, "");
    
    // Clean up whitespace and empty lines
    let lines: Vec<&str> = filtered
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    
    lines.join("\n").trim().to_string()
}

// Simple processing function that just filters thinking tags
// Returns filtered content or a message if only thinking content remains
fn process_ranking_content(content: &str) -> String {
    let filtered = filter_thinking_tags(content);
    
    // If we have filtered content, return it
    if !filtered.trim().is_empty() {
        return filtered;
    }
    
    // If no content after filtering, return a message
    "The AI response appears to contain only thinking content.".to_string()
}

// Helper function to update Discord message with new content for ranking
// Handles chunking and message creation if content exceeds Discord's limit
#[allow(unused_variables)]
async fn update_discord_message(
    state: &mut MessageState,
    new_content: &str,
    ctx: &Context,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("[DEBUG][RANKING_UPDATE] Updating Discord message with {} chars", new_content.len());
    
    println!("[DEBUG][RANKING_UPDATE] New content to add: '{}' ({} chars)", new_content, new_content.len());
    
    // First, add the new content to the state
    if state.current_content.is_empty() {
        println!("[DEBUG][RANKING_UPDATE] State content was empty, setting to new content");
        state.current_content = new_content.to_string();
    } else {
        println!("[DEBUG][RANKING_UPDATE] State content was not empty, appending new content");
        state.current_content.push_str(new_content);
    }
    
    println!("[DEBUG][RANKING_UPDATE] State content after adding: '{}' ({} chars)", state.current_content, state.current_content.len());
    
    // Then create the formatted content for Discord
    let potential_content = format!("📊 **Ranking Analysis (Part {}):**\n\n{}", 
        state.message_index, state.current_content);
    
    println!("[DEBUG][RANKING_UPDATE] Formatted content for Discord: '{}' ({} chars)", potential_content, potential_content.len());

    // Check if we need to create a new message
    if potential_content.len() > state.char_limit {
        println!("[DEBUG][RANKING_UPDATE] Content exceeds limit ({} > {}), creating new message", 
            potential_content.len(), state.char_limit);
        
        // Finalize current message
        let final_content = format!("📊 **Ranking Analysis (Part {}):**\n\n{}", 
            state.message_index, state.current_content);
        let edit_result = state.current_message.edit(&ctx.http, |m| {
            m.content(final_content)
        }).await;
        if let Err(e) = edit_result {
            eprintln!("[ERROR][RANKING_UPDATE] Failed to finalize message part {}: {}", state.message_index, e);
        } else {
            println!("[DEBUG][RANKING_UPDATE] Finalized message part {}", state.message_index);
        }

        // Create new message
        state.message_index += 1;
        // Reset current_content for the new message
        state.current_content = new_content.to_string();
        let new_msg_content = format!("📊 **Ranking Analysis (Part {}):**\n\n{}", 
            state.message_index, state.current_content);
        let send_result = state.current_message.channel_id.send_message(&ctx.http, |m| {
            m.content(new_msg_content)
        }).await;
        match send_result {
            Ok(new_message) => {
                println!("[DEBUG][RANKING_UPDATE] Created new message part {}", state.message_index);
                state.current_message = new_message;
            }
            Err(e) => {
                eprintln!("[ERROR][RANKING_UPDATE] Failed to create new message part {}: {}", state.message_index, e);
            }
        }
    } else {
        // Update current message
        println!("[DEBUG][RANKING_UPDATE] Updating existing message part {}", state.message_index);
        let edit_result = state.current_message.edit(&ctx.http, |m| {
            m.content(&potential_content)
        }).await;
        if let Err(e) = edit_result {
            eprintln!("[ERROR][RANKING_UPDATE] Failed to update existing message part {}: {}", state.message_index, e);
        }
    }

    Ok(())
}

// Helper function to finalize message content at the end of streaming for ranking
// Ensures all remaining content is posted and marks the message as complete
#[allow(unused_variables)]
async fn finalize_message_content(
    state: &mut MessageState,
    remaining_content: &str,
    ctx: &Context,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("[DEBUG][RANKING_FINALIZE] Finalizing message with {} chars", remaining_content.len());
    println!("[DEBUG][RANKING_FINALIZE] Current state content: {} chars", state.current_content.len());
    
    // Check for zero content error condition - this should catch cases where API returned content but it wasn't streamed properly
    if remaining_content.is_empty() && state.current_content.trim().is_empty() {
        eprintln!("[DEBUG][RANKING_FINALIZE] ERROR: Attempting to finalize message with 0 total characters");
        eprintln!("[DEBUG][RANKING_FINALIZE] Remaining content: '{}' ({} chars)", remaining_content, remaining_content.len());
        eprintln!("[DEBUG][RANKING_FINALIZE] State content: '{}' ({} chars)", state.current_content, state.current_content.len());
        eprintln!("[DEBUG][RANKING_FINALIZE] This indicates either:");
        eprintln!("[DEBUG][RANKING_FINALIZE] 1. No content was received from the API");
        eprintln!("[DEBUG][RANKING_FINALIZE] 2. Content was received but not properly streamed to Discord");
        eprintln!("[DEBUG][RANKING_FINALIZE] 3. The update_discord_message function failed to populate current_content");
        return Err("Cannot finalize message with 0 characters - this indicates no content was received from the API or streaming failed".into());
    }
    
    // Add any remaining content if provided
    if !remaining_content.trim().is_empty() {
        update_discord_message(state, remaining_content, ctx, config).await?;
    }
    
    // Check if we have any content to finalize (either from remaining_content or existing state)
    if state.current_content.trim().is_empty() {
        println!("[DEBUG][RANKING_FINALIZE] No content to finalize");
        return Ok(());
    }
    
    // Mark the final message as complete
    let final_display = if state.message_index == 1 {
        format!("📊 **Ranking Analysis Complete**\n\n{}", state.current_content)
    } else {
        format!("📊 **Ranking Analysis Complete (Part {}/{})**\n\n{}", 
            state.message_index, state.message_index, state.current_content)
    };

    println!("[DEBUG][RANKING_FINALIZE] Marking message as complete - Part {}", state.message_index);
    let edit_result = state.current_message.edit(&ctx.http, |m| {
        m.content(final_display)
    }).await;
    if let Err(e) = edit_result {
        eprintln!("[ERROR][RANKING_FINALIZE] Failed to finalize Discord message part {}: {}", state.message_index, e);
    }

    Ok(())
}

// Main streaming function that handles real-time response with Discord message editing
// Streams the AI's ranking analysis response, filtering <think> tags in real time
// Handles chunking, message updates, and finalization
async fn stream_ranking_analysis(
    content: &str,
    url: &str,
    config: &LMConfig,
    initial_msg: &mut Message,
    ctx: &Context,
    is_youtube: bool,
    file_path: Option<&str>,
) -> Result<StreamingStats, Box<dyn std::error::Error + Send + Sync>> {
    println!("[DEBUG][RANKING] === STARTING RANKING STREAM RESPONSE ===");
    println!("[DEBUG][RANKING] URL: {}", url);
    println!("[DEBUG][RANKING] Content length: {} characters", content.len());
    println!("[DEBUG][RANKING] Is YouTube: {}", is_youtube);
    println!("[DEBUG][RANKING] File path: {:?}", file_path);
    println!("[DEBUG][RANKING] Base URL: {}", config.base_url);
    println!("[DEBUG][RANKING] Timeout: {} seconds", config.timeout);
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    println!("[DEBUG][RANKING] HTTP client created");
    
    // Load appropriate system prompt
    let system_prompt = if is_youtube {
        match load_youtube_ranking_analysis_prompt().await {
            Ok(prompt) => prompt,
            Err(e) => {
                eprintln!("Failed to load YouTube ranking analysis prompt: {}", e);
                return Err(e);
            }
        }
    } else {
        match load_ranking_analysis_prompt().await {
            Ok(prompt) => prompt,
            Err(e) => {
                eprintln!("Failed to load ranking analysis prompt: {}", e);
                return Err(e);
            }
        }
    };
    
    // Process content based on file path
    let (user_prompt, _content_to_process) = if let Some(path) = file_path {
        // Read and process file content
        let file_content = fs::read_to_string(path)?;
        let cleaned_content = if is_youtube {
            clean_vtt_content(&file_content)
        } else {
            clean_html(&file_content)
        };
        
        let prompt = format!(
            "Please analyze and rank this {} from {}:\n\n{}",
            if is_youtube { "YouTube video subtitle file" } else { "webpage HTML content" },
            url, cleaned_content
        );
        
        (prompt, cleaned_content)
    } else {
        // Use direct content
        let max_content_length = 20000;
        let truncated_content = if content.len() > max_content_length {
            format!("{} [Content truncated due to length]", &content[0..max_content_length])
        } else {
            content.to_string()
        };
        
        let prompt = format!(
            "Please analyze and rank this {} from {}:\n\n{}",
            if is_youtube { "YouTube video transcript" } else { "webpage content" },
            url, truncated_content
        );
        
        (prompt, truncated_content)
    };
    
    // Build message list
    let messages = vec![
        ChatMessage { role: "system".to_string(), content: system_prompt },
        ChatMessage { role: "user".to_string(), content: user_prompt },
    ];
    
    // Check if the model supports streaming (reranker models typically don't)
    let should_stream = !config.default_ranking_model.contains("reranker");
    
    let chat_request = ChatRequest {
        model: config.default_ranking_model.clone(),
        messages,
        temperature: config.default_temperature,
        max_tokens: config.default_max_tokens,
        stream: should_stream,
    };
    
    let api_url = format!("{}/v1/chat/completions", config.base_url);
    println!("[DEBUG][RANKING] API URL: {}", api_url);
    
    // Test basic connectivity
    match client.get(&config.base_url).send().await {
        Ok(response) => {
            println!("[DEBUG][RANKING] Basic connectivity test successful - Status: {}", response.status());
        }
        Err(e) => {
            println!("[DEBUG][RANKING] Basic connectivity test failed: {}", e);
            return Err(format!("Cannot reach remote server {}: {}", config.base_url, e).into());
        }
    }
    
    // Make API request (streaming or non-streaming)
    let response = match client
        .post(&api_url)
        .json(&chat_request)
        .send()
        .await
    {
        Ok(resp) => {
            println!("[DEBUG][RANKING] API request sent successfully - Status: {}", resp.status());
            resp
        }
        Err(e) => {
            println!("[DEBUG][RANKING] API request failed: {}", e);
            return Err(format!("API request to {} failed: {}", api_url, e).into());
        }
    };
    
    // Handle non-streaming response for reranker models
    if !should_stream {
        println!("[DEBUG][RANKING] Using non-streaming mode for reranker model");
        let response_text = response.text().await?;
        println!("[DEBUG][RANKING] Received non-streaming response: {} chars", response_text.len());
        println!("[DEBUG][RANKING] Response preview: {}", &response_text[..std::cmp::min(200, response_text.len())]);
        
        // Parse the JSON response - handle both streaming and non-streaming formats
        let mut raw_response = String::new();
        
        if let Ok(chat_response) = serde_json::from_str::<ChatResponse>(&response_text) {
            // Streaming format
            for choice in chat_response.choices {
                if let Some(delta) = choice.delta {
                    if let Some(content) = delta.content {
                        raw_response.push_str(&content);
                    }
                }
            }
        } else {
            // Non-streaming format - try to extract content directly
            println!("[DEBUG][RANKING] Trying non-streaming format parsing");
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&response_text) {
                if let Some(choices) = json_value.get("choices").and_then(|c| c.as_array()) {
                    for choice in choices {
                        if let Some(message) = choice.get("message") {
                            if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                                raw_response.push_str(content);
                            }
                        }
                    }
                }
            }
        }
        
        if raw_response.is_empty() {
            return Err("API returned empty response".into());
        }
        
        // Continue with processing and streaming to Discord
        println!("[DEBUG][RANKING] === PROCESSING NON-STREAMING RESPONSE ===");
        
        // Apply thinking tag filtering to the complete response
        let filtered_response = filter_thinking_tags(&raw_response);
        println!("[DEBUG][RANKING] Filtered response length: {} chars", filtered_response.len());
        
        // Apply ranking content processing
        let processed_response = process_ranking_content(&filtered_response);
        println!("[DEBUG][RANKING] Processed response length: {} chars", processed_response.len());
        
        if processed_response.is_empty() {
            println!("[DEBUG][RANKING] Processed response is empty, sending fallback message");
            let _ = initial_msg.edit(&ctx.http, |m| {
                m.content("📊 **Ranking Analysis Complete**\n\nThe AI completed its ranking analysis, but the response appears to contain only thinking content.")
            }).await;
            
            let stats = StreamingStats {
                total_characters: raw_response.len(),
                message_count: 1,
                filtered_characters: raw_response.len() - filtered_response.len(),
            };
            return Ok(stats);
        }
        
        let mut message_state = MessageState {
            current_content: String::new(),
            current_message: initial_msg.clone(),
            message_index: 1,
            char_limit: config.max_discord_message_length - config.response_format_padding,
        };
        
        // Stream the processed response to Discord in chunks
        let chunk_size = 100;
        let mut chars_processed = 0;
        
        while chars_processed < processed_response.len() {
            let end_pos = std::cmp::min(chars_processed + chunk_size, processed_response.len());
            let chunk = &processed_response[chars_processed..end_pos];
            
            if let Err(e) = update_discord_message(&mut message_state, chunk, ctx, config).await {
                eprintln!("[DEBUG][RANKING] Failed to update Discord message: {}", e);
                return Err(e);
            }
            
            chars_processed = end_pos;
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        
        // Finalize the message
        finalize_message_content(&mut message_state, "", ctx, config).await?;
        
        let stats = StreamingStats {
            total_characters: raw_response.len(),
            message_count: message_state.message_index,
            filtered_characters: raw_response.len() - filtered_response.len(),
        };
        return Ok(stats);
    }
    
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
        println!("[DEBUG][RANKING] API returned error status {}: {}", status, error_text);
        return Err(format!("Streaming API request failed: HTTP {} - {}", status, error_text).into());
    }
    
    println!("[DEBUG][RANKING] === BUFFERING COMPLETE RESPONSE ===");
    let mut stream = response.bytes_stream();
    
    let mut raw_response = String::new();
    let mut chunk_count = 0;
    let mut line_buffer = String::new();
    let mut received_any_content = false;
    let mut stream_complete = false;
    
    println!("[DEBUG][RANKING] Starting to buffer response from API...");
    
    // STEP 1: Buffer the complete response from the API
    while let Some(chunk) = stream.next().await {
        if stream_complete {
            println!("[DEBUG][RANKING] Stream marked as complete, stopping buffering");
            break;
        }
        
        match chunk {
            Ok(bytes) => {
                chunk_count += 1;
                if chunk_count == 1 {
                    println!("[DEBUG][RANKING] Received first chunk ({} bytes)", bytes.len());
                } else if chunk_count % 10 == 0 {
                    println!("[DEBUG][RANKING] Buffered {} chunks, total response: {} chars", chunk_count, raw_response.len());
                }
                
                line_buffer.push_str(&String::from_utf8_lossy(&bytes));
                
                while let Some(i) = line_buffer.find('\n') {
                    let line = line_buffer.drain(..=i).collect::<String>();
                    let line = line.trim();
                    
                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str.trim() == "[DONE]" {
                            println!("[DEBUG][RANKING] Received [DONE] signal, marking stream complete");
                            stream_complete = true;
                            break;
                        }
                        
                        if let Ok(response_chunk) = serde_json::from_str::<ChatResponse>(json_str) {
                            for choice in response_chunk.choices {
                                if let Some(finish_reason) = choice.finish_reason {
                                    if finish_reason == "stop" {
                                        println!("[DEBUG][RANKING] Received finish_reason=stop, marking stream complete");
                                        stream_complete = true;
                                        break;
                                    }
                                }
                                
                                if let Some(delta) = choice.delta {
                                    if let Some(content) = delta.content {
                                        received_any_content = true;
                                        raw_response.push_str(&content);
                                        println!("[DEBUG][RANKING] Added content chunk: '{}' (total: {} chars)", 
                                            content, raw_response.len());
                                    }
                                }
                            }
                        }
                    }
                }
                
                if stream_complete {
                    println!("[DEBUG][RANKING] Breaking out of chunk processing loop");
                    break;
                }
            }
            Err(e) => {
                eprintln!("[DEBUG][RANKING] Stream error: {}", e);
                return Err(e.into());
            }
        }
    }
    
    println!("[DEBUG][RANKING] === BUFFERING COMPLETE ===");
    println!("[DEBUG][RANKING] Buffered {} chunks, total response: {} chars", chunk_count, raw_response.len());
    
    if !received_any_content {
        eprintln!("[DEBUG][RANKING] No content received from API stream");
        return Err("No content received from API stream".into());
    }
    
    if raw_response.is_empty() {
        eprintln!("[DEBUG][RANKING] ERROR: API returned 0 characters in response");
        return Err("API returned 0 characters in response - this indicates a problem with the API or model".into());
    }
    
    // STEP 2: Process the buffered content and stream to Discord
    println!("[DEBUG][RANKING] === PROCESSING AND STREAMING TO DISCORD ===");
    
    // Apply thinking tag filtering to the complete response
    let filtered_response = filter_thinking_tags(&raw_response);
    println!("[DEBUG][RANKING] Filtered response length: {} chars", filtered_response.len());
    
    // Apply ranking content processing
    let processed_response = process_ranking_content(&filtered_response);
    println!("[DEBUG][RANKING] Processed response length: {} chars", processed_response.len());
    
    if processed_response.is_empty() {
        println!("[DEBUG][RANKING] Processed response is empty, sending fallback message");
        let _ = initial_msg.edit(&ctx.http, |m| {
            m.content("📊 **Ranking Analysis Complete**\n\nThe AI completed its ranking analysis, but the response appears to contain only thinking content.")
        }).await;
        
        let stats = StreamingStats {
            total_characters: raw_response.len(),
            message_count: 1,
            filtered_characters: raw_response.len() - filtered_response.len(),
        };
        return Ok(stats);
    }
    
    let mut message_state = MessageState {
        current_content: String::new(),
        current_message: initial_msg.clone(),
        message_index: 1,
        char_limit: config.max_discord_message_length - config.response_format_padding,
    };
    println!("[DEBUG][RANKING] Message state initialized - Char limit: {}", message_state.char_limit);
    
    // Split the processed response into chunks for Discord streaming
    let chunk_size = 100; // Characters per Discord update
    let mut chars_processed = 0;
    
    while chars_processed < processed_response.len() {
        let end_pos = std::cmp::min(chars_processed + chunk_size, processed_response.len());
        let chunk = &processed_response[chars_processed..end_pos];
        
        println!("[DEBUG][RANKING] Streaming chunk {} chars to Discord", chunk.len());
        
        if let Err(e) = update_discord_message(&mut message_state, chunk, ctx, config).await {
            eprintln!("[DEBUG][RANKING] Failed to update Discord message: {}", e);
            return Err(e);
        }
        
        chars_processed = end_pos;
        
        // Small delay to make streaming visible
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    // Finalize the message
    println!("[DEBUG][RANKING] === FINALIZING DISCORD MESSAGE ===");
    
    if processed_response.is_empty() {
        eprintln!("[DEBUG][RANKING] ERROR: Cannot finalize - no content was processed from API");
        return Err("No content was processed from API - cannot finalize empty message".into());
    }
    
    if message_state.current_content.trim().is_empty() {
        eprintln!("[DEBUG][RANKING] ERROR: Message state has no content despite processed response");
        return Err("Message state has no content despite processed response - streaming to Discord failed".into());
    }
    
    if let Err(e) = finalize_message_content(&mut message_state, "", ctx, config).await {
        eprintln!("[DEBUG][RANKING] Failed to finalize Discord message: {}", e);
        return Err(e);
    }
    
    let stats = StreamingStats {
        total_characters: raw_response.len(),
        message_count: message_state.message_index,
        filtered_characters: raw_response.len() - filtered_response.len(),
    };
    
    println!("[DEBUG][RANKING] === RANKING STREAMING COMPLETED ===");
    println!("[DEBUG][RANKING] Final stats - Total chars: {}, Messages: {}, Filtered chars: {}", 
        stats.total_characters, stats.message_count, stats.filtered_characters);
    Ok(stats)
}

// Helper function to split messages for Discord's character limit
fn split_message(content: &str, max_len: usize) -> Vec<String> {
    let mut messages = Vec::new();
    let mut current_message = String::new();
    
    for line in content.lines() {
        if current_message.len() + line.len() + 1 > max_len {
            if !current_message.is_empty() {
                messages.push(current_message.trim().to_string());
                current_message = String::new();
            }
            
            // If a single line is too long, split it
            if line.len() > max_len {
                let mut remaining = line;
                while remaining.len() > max_len {
                    let split_point = remaining[..max_len].rfind(' ').unwrap_or(max_len);
                    messages.push(remaining[..split_point].to_string());
                    remaining = &remaining[split_point..];
                }
                if !remaining.is_empty() {
                    current_message.push_str(remaining);
                    current_message.push('\n');
                }
            } else {
                current_message.push_str(line);
                current_message.push('\n');
            }
        } else {
            current_message.push_str(line);
            current_message.push('\n');
        }
    }
    
    if !current_message.is_empty() {
        messages.push(current_message.trim().to_string());
    }
    
    messages
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_vtt() {
        let vtt_content = r#"WEBVTT

00:00:01.000 --> 00:00:04.000
Hello world, this is a test

00:00:05.000 --> 00:00:08.000
<c>This is some content</c>

00:00:09.000 --> 00:00:12.000
<v Speaker>This is spoken content</v>
"#;
        
        let cleaned = clean_vtt_content(vtt_content);
        assert!(!cleaned.contains("WEBVTT"));
        assert!(!cleaned.contains("-->"));
        assert!(!cleaned.contains("<c>"));
        assert!(!cleaned.contains("</c>"));
        assert!(!cleaned.contains("<v"));
        assert!(!cleaned.contains("</v>"));
        assert!(cleaned.contains("Hello world"));
        assert!(cleaned.contains("This is some content"));
        assert!(cleaned.contains("This is spoken content"));
    }

    #[test]
    fn test_clean_html() {
        let html_content = r#"<html><head><title>Test</title></head><body><script>alert('test');</script><style>body { color: red; }</style><p>This is <b>bold</b> text</p></body></html>"#;
        
        let cleaned = clean_html(html_content);
        assert!(!cleaned.contains("<script>"));
        assert!(!cleaned.contains("<style>"));
        assert!(!cleaned.contains("<html>"));
        assert!(!cleaned.contains("<p>"));
        assert!(!cleaned.contains("<b>"));
        assert!(!cleaned.contains("</b>"));
        assert!(cleaned.contains("This is bold text"));
    }

    #[test]
    fn test_webpage_content_processing() {
        // Test that webpage content processing works correctly
        let test_content = "This is a test webpage content with some text to analyze and rank.";
        let test_url = "https://example.com";
        
        // Test content truncation
        let long_content = "A".repeat(25000);
        let truncated = if long_content.len() > 20000 {
            format!("{} [Content truncated due to length]", &long_content[0..20000])
        } else {
            long_content.clone()
        };
        
        assert!(truncated.len() <= 20000 + 30); // 30 chars for truncation message
        assert!(truncated.contains("[Content truncated due to length]"));
        
        // Test normal content
        let normal_content = "Normal length content";
        let normal_truncated = if normal_content.len() > 20000 {
            format!("{} [Content truncated due to length]", &normal_content[0..20000])
        } else {
            normal_content.clone()
        };
        
        assert_eq!(normal_truncated, normal_content);
    }

    #[test]
    fn test_html_file_processing() {
        // Test HTML file processing functionality
        let test_html = r#"<html><head><title>Test Page</title></head><body><h1>Test Content</h1><p>This is a test paragraph with <b>bold</b> text and <i>italic</i> text.</p><script>console.log('test');</script><style>body { font-family: Arial; }</style></body></html>"#;
        
        let cleaned = clean_html(test_html);
        
        // Should remove HTML tags
        assert!(!cleaned.contains("<html>"));
        assert!(!cleaned.contains("<head>"));
        assert!(!cleaned.contains("<body>"));
        assert!(!cleaned.contains("<h1>"));
        assert!(!cleaned.contains("<p>"));
        assert!(!cleaned.contains("<b>"));
        assert!(!cleaned.contains("<i>"));
        assert!(!cleaned.contains("<script>"));
        assert!(!cleaned.contains("<style>"));
        
        // Should preserve text content
        assert!(cleaned.contains("Test Content"));
        assert!(cleaned.contains("This is a test paragraph"));
        assert!(cleaned.contains("bold"));
        assert!(cleaned.contains("italic"));
        
        // Should not contain script or style content
        assert!(!cleaned.contains("console.log"));
        assert!(!cleaned.contains("font-family"));
    }
}