// lm.rs - Language Model (AI Chat, Vision, and RAG) Command Module
// This module implements the ^lm command, providing real-time AI chat, vision analysis, document RAG, and AI-enhanced search.
// It supports streaming, per-user context, multimodal (text+image) messages, and robust error handling.
//
// Key Features:
// - Real-time streaming chat with context memory
// - Vision analysis (image/GIF support)
// - Document RAG (Retrieval-Augmented Generation) for file attachments
// - AI-enhanced web search (with summarization)
// - Multi-path config and prompt loading
// - Robust error handling and context management
//
// Used by: main.rs (command registration), vis.rs (for vision), search.rs (for config)

use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
    model::channel::Attachment,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use futures_util::StreamExt;
use crate::commands::search::{
    load_lm_config, perform_ai_enhanced_search, LMConfig, ChatMessage
};
use crate::LmContextMap; // TypeMap key defined in main.rs

use std::io::Write;
use uuid::Uuid;

// Structure to track streaming statistics for chat
// Used to report total characters and message count for streamed responses
#[derive(Debug)]
pub struct StreamingStats {
    pub total_characters: usize, // Total characters streamed
    pub message_count: usize,    // Number of Discord messages sent
}

// Structure to track current message state during streaming
// Used to manage chunking and message updates for Discord
pub struct MessageState {
    pub current_content: String, // Accumulated content for current Discord message
    pub current_message: Message,// Current Discord message object
    pub message_index: usize,    // Part number (for chunked output)
    pub char_limit: usize,       // Discord message length limit
}

// Enhanced ChatMessage structure for multimodal content (text and images)
// Used for vision and RAG (document) support
#[derive(Serialize, Deserialize, Clone)]
pub struct MultimodalChatMessage {
    pub role: String,                // "system", "user", or "assistant"
    pub content: Vec<MessageContent>,// List of content blocks (text/image)
}

// Enum for multimodal message content (text or image)
#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum MessageContent {
    Text { #[serde(rename = "type")] content_type: String, text: String },
    Image { #[serde(rename = "type")] content_type: String, image_url: ImageUrl },
}

// Structure for image URLs in multimodal messages
#[derive(Serialize, Deserialize, Clone)]
pub struct ImageUrl {
    pub url: String, // Data URI or external URL
}

// Document processing result for RAG (Retrieval-Augmented Generation)
// Used to store extracted content from user-uploaded files
#[derive(Debug)]
struct ProcessedDocument {
    pub filename: String,      // Name of the file
    pub content: String,       // Extracted text content
    pub content_type: String,  // MIME type
    pub size: usize,           // File size in bytes
}

// API Request/Response structures for streaming
// Used for chat, vision, and RAG requests
#[derive(Serialize)]
pub struct ChatRequest {
    pub model: String,                        // Model name
    pub messages: Vec<MultimodalChatMessage>, // Conversation history (multimodal)
    pub temperature: f32,                     // Sampling temperature
    pub max_tokens: i32,                      // Max tokens to generate
    pub stream: bool,                         // Whether to stream output
}

#[derive(Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>, // Streaming choices (delta content)
}

#[derive(Deserialize)]
pub struct Choice {
    pub delta: Option<Delta>,         // Streaming delta (content chunk)
    pub finish_reason: Option<String>,// Reason for stream completion
}

#[derive(Deserialize)]
pub struct Delta {
    pub content: Option<String>,      // Content chunk
}

// Forward declaration of handle_lm_request for use in lm command

/// Resolve Discord user IDs to usernames in text
/// This function finds patterns like <@123456789> and @123456789 and replaces them with username
/// Features: timeout protection, rate limiting, robust error handling, and fallback mechanisms
async fn resolve_user_mentions(ctx: &Context, text: &str) -> String {
    use regex::Regex;
    use serenity::model::id::UserId;
    use std::collections::HashMap;
    use tokio::time::{timeout, Duration};

    println!("[MENTION_RESOLVE] === STARTING MENTION RESOLUTION ===");
    println!("[MENTION_RESOLVE] Original text: '{}'", text);

    // Early return if no potential mentions found
    if !text.contains('@') && !text.contains('<') {
        println!("[MENTION_RESOLVE] No potential mentions found, returning original text");
        return text.to_string();
    }

    let mut result = text.to_string();
    let mut id_to_username: HashMap<String, String> = HashMap::new();
    let mut user_ids: Vec<String> = Vec::new();

    // More robust patterns with word boundaries and validation
    let mention_pattern = Regex::new(r"<@!?(\d+)>").unwrap(); // Any number of digits in Discord mentions
    let at_pattern = Regex::new(r"@(\d+)(?:\b|$)").unwrap(); // @ followed by any digits with word boundary
    
    // Additional pattern to catch any remaining user ID references
    let _any_user_id_pattern = Regex::new(r"(\d{17,19})").unwrap(); // Discord user IDs are typically 17-19 digits

    println!("[MENTION_RESOLVE] Searching for Discord mention patterns...");
    // Collect all unique user IDs from both patterns
    for caps in mention_pattern.captures_iter(text) {
        let user_id_str = caps[1].to_string();
        println!("[MENTION_RESOLVE] Found Discord mention: <@{}> -> ID: {}", &caps[0], user_id_str);
        if !user_ids.contains(&user_id_str) {
            user_ids.push(user_id_str);
        }
    }

    println!("[MENTION_RESOLVE] Searching for @userID patterns...");
    for caps in at_pattern.captures_iter(text) {
        let user_id_str = caps[1].to_string();
        println!("[MENTION_RESOLVE] Found @userID: @{} -> ID: {}", &caps[0], user_id_str);
        if !user_ids.contains(&user_id_str) {
            user_ids.push(user_id_str);
        }
    }

    if user_ids.is_empty() {
        println!("[MENTION_RESOLVE] No valid user IDs found in text");
        return result;
    }

    println!("[MENTION_RESOLVE] Found {} unique user IDs to resolve: {:?}", user_ids.len(), user_ids);

    // Resolve each user ID to username with timeout and error handling
    for user_id_str in &user_ids {
        // Basic validation - ensure it's a reasonable length
        if user_id_str.is_empty() || user_id_str.len() > 20 {
            println!("[MENTION_RESOLVE] Skipping invalid user ID format: {}", user_id_str);
            continue;
        }

        if let Ok(user_id_num) = user_id_str.parse::<u64>() {
            let user_id = UserId(user_id_num);
            
            // Try cache first (fastest)
            if let Some(user) = ctx.cache.user(user_id) {
                println!("[MENTION_RESOLVE] Found user {} in cache", user.name);
                id_to_username.insert(user_id_str.clone(), user.name.clone());
                continue;
            }

            // Try API with timeout (5 seconds per user)
            let username = match timeout(Duration::from_secs(5), ctx.http.get_user(user_id_num)).await {
                Ok(Ok(user)) => {
                    println!("[MENTION_RESOLVE] Found user {} via API", user.name);
                    user.name.clone()
                }
                Ok(Err(e)) => {
                    println!("[MENTION_RESOLVE] API error for user ID {}: {}", user_id_str, e);
                    // Better fallback: use a more descriptive name
                    format!("UnknownUser_{}", &user_id_str[user_id_str.len().saturating_sub(4)..])
                }
                Err(_) => {
                    println!("[MENTION_RESOLVE] Timeout for user ID {}", user_id_str);
                    // Better fallback: use a more descriptive name
                    format!("UnknownUser_{}", &user_id_str[user_id_str.len().saturating_sub(4)..])
                }
            };
            
            id_to_username.insert(user_id_str.clone(), username);
            
            // Small delay to avoid rate limiting
            tokio::time::sleep(Duration::from_millis(100)).await;
        } else {
            println!("[MENTION_RESOLVE] Failed to parse user ID as u64: {}", user_id_str);
            // Even if parsing fails, create a fallback entry
            let fallback_name = format!("InvalidUser_{}", &user_id_str[user_id_str.len().saturating_sub(4)..]);
            id_to_username.insert(user_id_str.clone(), fallback_name);
        }
    }

    println!("[MENTION_RESOLVE] Resolved usernames: {:?}", id_to_username);

    // Replace Discord mention format: <@123456789> or <@!123456789> with username (no @)
    println!("[MENTION_RESOLVE] Replacing Discord mention patterns...");
    for (user_id, username) in &id_to_username {
        let mention_patterns = vec![
            format!("<@{}>", user_id),
            format!("<@!{}>", user_id),
        ];
        
        println!("[MENTION_RESOLVE] Checking for user ID: {} -> username: {}", user_id, username);
        for pattern in mention_patterns {
            println!("[MENTION_RESOLVE] Looking for pattern: '{}' in result", pattern);
            if result.contains(&pattern) {
                println!("[MENTION_RESOLVE] ✅ Found pattern '{}' in text", pattern);
                let replacement = username.clone(); // No @ symbol
                println!("[MENTION_RESOLVE] Replacing '{}' with '{}'", pattern, replacement);
                result = result.replace(&pattern, &replacement);
                println!("[MENTION_RESOLVE] Result after replacement: '{}'", result);
            } else {
                println!("[MENTION_RESOLVE] ❌ Pattern '{}' not found in text", pattern);
            }
        }
    }

    // Replace @userID format: @123456789 with username (no @)
    println!("[MENTION_RESOLVE] Replacing @userID patterns...");
    for (user_id, username) in &id_to_username {
        // Use regex for more precise replacement to avoid partial matches
        let at_regex = Regex::new(&format!(r"@({})(?:\b|$)", regex::escape(user_id))).unwrap();
        let replacement = username.clone(); // No @ symbol
        if at_regex.is_match(&result) {
            println!("[MENTION_RESOLVE] Replacing @{} with '{}'", user_id, replacement);
            result = at_regex.replace_all(&result, replacement).into_owned();
            println!("[MENTION_RESOLVE] Result after @ replacement: '{}'", result);
        }
    }

    // Final cleanup: catch any remaining user IDs that might have been missed
    println!("[MENTION_RESOLVE] === FINAL CLEANUP ===");
    for (user_id, username) in &id_to_username {
        // Look for any remaining instances of the user ID (without @ or <>)
        if result.contains(user_id) {
            println!("[MENTION_RESOLVE] Found remaining user ID '{}' in result, replacing with '{}'", user_id, username);
            result = result.replace(user_id, username);
        }
    }
    
    println!("[MENTION_RESOLVE] === MENTION RESOLUTION COMPLETE ===");
    println!("[MENTION_RESOLVE] Final processed text: '{}'", result);
    
    // Verify that replacements actually happened
    if result != text {
        println!("[MENTION_RESOLVE] ✅ SUCCESS: Text was modified from '{}' to '{}'", text, result);
    } else {
        println!("[MENTION_RESOLVE] ⚠️ WARNING: Text was not modified despite having user IDs");
        println!("[MENTION_RESOLVE] Original: '{}'", text);
        println!("[MENTION_RESOLVE] Result: '{}'", result);
        println!("[MENTION_RESOLVE] Resolved usernames: {:?}", id_to_username);
    }
    
    result
}

pub async fn handle_lm_request(
    ctx: &Context,
    msg: &Message,
    input: &str,
    original_prompt: Option<&str>, // Add optional original prompt parameter
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("[DEBUG][HANDLE_LM] === STARTING LM REQUEST ===");
    println!("[DEBUG][HANDLE_LM] User: {} (ID: {})", msg.author.name, msg.author.id);
    println!("[DEBUG][HANDLE_LM] Input received: '{}'", input);
    if let Some(orig) = original_prompt {
        println!("[DEBUG][HANDLE_LM] Original prompt: '{}'", orig);
    } else {
        println!("[DEBUG][HANDLE_LM] No original prompt provided");
    }
    println!("[DEBUG][HANDLE_LM] Message attachments: {}", msg.attachments.len());
    println!("[DEBUG][HANDLE_LM] Has referenced message: {}", msg.referenced_message.is_some());
    
    // Resolve user IDs to usernames in the input
    let processed_input = resolve_user_mentions(ctx, input).await;
    println!("[DEBUG][HANDLE_LM] Processed input with resolved mentions: '{}'", processed_input);
    
    // Check if this is a vision request
    if processed_input.starts_with("-v") || processed_input.starts_with("--vision") {
        println!("[DEBUG][HANDLE_LM] === VISION REQUEST DETECTED ===");
        println!("[DEBUG][HANDLE_LM] Delegating to vision handling");
        
        let vision_prompt = if processed_input.starts_with("-v") {
            let after_flag = if processed_input.starts_with("-v ") {
                &processed_input[3..] // "-v "
            } else {
                &processed_input[2..] // "-v"
            };
            after_flag.trim().to_string()
        } else {
            let after_flag = if processed_input.starts_with("--vision ") {
                &processed_input[9..] // "--vision "
            } else {
                &processed_input[8..] // "--vision"
            };
            after_flag.trim().to_string()
        };
        
        println!("[DEBUG][HANDLE_LM] Extracted vision prompt: '{}'", vision_prompt);
        
        if vision_prompt.is_empty() {
            println!("[DEBUG][HANDLE_LM] Vision prompt is empty, returning error");
            msg.reply(ctx, "Please provide a prompt for vision analysis! Usage: `^lm -v <prompt>` with image attached.").await?;
            return Ok(());
        }

        // Enhanced attachment detection with more debugging
        println!("[DEBUG][HANDLE_LM] === ATTACHMENT DETECTION ===");
        println!("[DEBUG][HANDLE_LM] Current message attachments: {}", msg.attachments.len());
        
        let attachment_to_process = if !msg.attachments.is_empty() {
            println!("[DEBUG][HANDLE_LM] Found {} attachments in current message", msg.attachments.len());
            for (i, att) in msg.attachments.iter().enumerate() {
                println!("[DEBUG][HANDLE_LM] Attachment {}: {} ({})", i, att.filename, att.content_type.as_deref().unwrap_or("unknown"));
            }
            Some(&msg.attachments[0])
        } else {
            println!("[DEBUG][HANDLE_LM] No attachments in current message");
            if let Some(referenced_msg) = &msg.referenced_message {
                println!("[DEBUG][HANDLE_LM] Checking referenced message from user: {}", referenced_msg.author.name);
                println!("[DEBUG][HANDLE_LM] Referenced message attachments: {}", referenced_msg.attachments.len());
                
                if !referenced_msg.attachments.is_empty() {
                    println!("[DEBUG][HANDLE_LM] Found {} attachments in referenced message", referenced_msg.attachments.len());
                    for (i, att) in referenced_msg.attachments.iter().enumerate() {
                        println!("[DEBUG][HANDLE_LM] Referenced attachment {}: {} ({})", i, att.filename, att.content_type.as_deref().unwrap_or("unknown"));
                    }
                    Some(&referenced_msg.attachments[0])
                } else {
                    println!("[DEBUG][HANDLE_LM] No attachments found in referenced message");
                    None
                }
            } else {
                println!("[DEBUG][HANDLE_LM] No referenced message found");
                None
            }
        };

        let attachment = match attachment_to_process {
            Some(att) => {
                println!("[DEBUG][HANDLE_LM] Using attachment: {} ({})", att.filename, att.content_type.as_deref().unwrap_or("unknown"));
                att
            },
            None => {
                println!("[DEBUG][HANDLE_LM] No image attachments found in current or referenced message");
                msg.reply(ctx, "Please attach an image for vision analysis, or reply to a message with an image attachment.").await?;
                return Ok(());
            }
        };

        let content_type = attachment.content_type.as_deref().unwrap_or("");
        println!("[DEBUG][HANDLE_LM] Checking content type: '{}'", content_type);
        
        if !content_type.starts_with("image/") {
            println!("[DEBUG][HANDLE_LM] Attachment is not an image, returning error");
            msg.reply(ctx, "Attached file is not an image. Please attach a valid image file.").await?;
            return Ok(());
        }

        println!("[DEBUG][HANDLE_LM] === CALLING VISION HANDLER ===");
        println!("[DEBUG][HANDLE_LM] Calling vision handler for attachment: {}", attachment.filename);
        return crate::commands::vis::handle_vision_request(ctx, msg, &vision_prompt, attachment).await;
    }
    
    // Regular LM handling with RAG support
    println!("[DEBUG][HANDLE_LM] === REGULAR LM REQUEST ===");
    println!("[DEBUG][HANDLE_LM] Processing as regular LM request");
    
    let prompt = processed_input.clone();
    println!("[DEBUG][HANDLE_LM] Using processed prompt: '{}'", prompt);
    
    // Process attachments for RAG if any
    println!("[DEBUG][HANDLE_LM] === RAG ATTACHMENT PROCESSING ===");
    let mut processed_documents = Vec::new();
    if !msg.attachments.is_empty() {
        println!("[DEBUG][HANDLE_LM] Found {} attachments, processing for document analysis", msg.attachments.len());
        
        match process_attachments(&msg.attachments, ctx).await {
            Ok(docs) => {
                processed_documents = docs;
                println!("[DEBUG][HANDLE_LM] Successfully processed {} documents", processed_documents.len());
                for (i, doc) in processed_documents.iter().enumerate() {
                    println!("[DEBUG][HANDLE_LM] Document {}: {} ({} chars, type: {})", 
                        i + 1, doc.filename, doc.content.len(), doc.content_type);
                }
            }
            Err(e) => {
                eprintln!("[DEBUG][HANDLE_LM] Failed to process attachments: {}", e);
                msg.reply(ctx, &format!("⚠️ Failed to process some attachments: {}\n\nContinuing with text-only analysis.", e)).await?;
            }
        }
    } else {
        println!("[DEBUG][HANDLE_LM] No attachments found for RAG processing");
    }
    
    // Create RAG-enhanced prompt if documents were processed
    println!("[DEBUG][HANDLE_LM] === PROMPT ENHANCEMENT ===");
    let final_prompt = if !processed_documents.is_empty() {
        let enhanced = create_rag_prompt(&prompt, &processed_documents);
        println!("[DEBUG][HANDLE_LM] Created RAG-enhanced prompt ({} chars)", enhanced.len());
        enhanced
    } else {
        println!("[DEBUG][HANDLE_LM] Using original prompt (no RAG enhancement)");
        prompt.to_string()
    };

    // Record user prompt in per-user context history (store processed prompt, not RAG-enhanced)
    println!("[DEBUG][HANDLE_LM] === CONTEXT RECORDING ===");
    {
        let mut data_map = ctx.data.write().await;
        
        // Scoped for lm_map
        {
            let lm_map = data_map.get_mut::<LmContextMap>().expect("LM context map not initialized");
            let context = lm_map.entry(msg.author.id).or_insert_with(crate::UserContext::new);
            
            // Log current context state before adding message
            println!("[DEBUG][HANDLE_LM] Current context state: {}", context.get_context_info());
            
            // Force cleanup if context is getting too large
            if context.needs_cleanup() {
                println!("[DEBUG][HANDLE_LM] Context is large, forcing cleanup before adding new message");
                context.force_cleanup();
            }
            
            // Use processed prompt for context (with resolved mentions)
            let context_prompt = &processed_input;
            println!("[DEBUG][HANDLE_LM] Recording processed prompt in context: '{}'", context_prompt);
            context.add_user_message(ChatMessage { role: "user".to_string(), content: context_prompt.to_string() });
            
            // Log context state after adding message
            println!("[DEBUG][HANDLE_LM] Context after adding user message: {}", context.get_context_info());
            
            // Check if context needs cleanup
            if context.needs_cleanup() {
                println!("[DEBUG][HANDLE_LM] Context is getting large, may need cleanup soon");
            }
        }
    }

    println!("[DEBUG][HANDLE_LM] === CONFIGURATION LOADING ===");
    let config = crate::commands::search::load_lm_config().await?;
    println!("[DEBUG][HANDLE_LM] Loaded LM config - Model: {}, URL: {}", config.default_model, config.base_url);
    
    let base_system_prompt = load_system_prompt().await?;
    println!("[DEBUG][HANDLE_LM] Loaded system prompt ({} chars)", base_system_prompt.len());
    
    println!("[DEBUG][HANDLE_LM] === MESSAGE BUILDING ===");
    let mut messages = Vec::new();
    messages.push(ChatMessage { role: "system".to_string(), content: base_system_prompt });
    println!("[DEBUG][HANDLE_LM] Added system message");
    
    {
        let data_map = ctx.data.read().await;
        if let Some(lm_map) = data_map.get::<LmContextMap>() {
            if let Some(context) = lm_map.get(&msg.author.id) {
                // Safety check: force cleanup if context is too large
                if context.needs_cleanup() {
                    println!("[DEBUG][HANDLE_LM] Context is large, will force cleanup before loading messages");
                }
                
                let conversation_messages = context.get_conversation_messages();
                println!("[DEBUG][HANDLE_LM] Loading {} conversation messages from context", conversation_messages.len());
                for (i, entry) in conversation_messages.iter().enumerate() {
                    // Process mentions in historical messages to ensure no user IDs are sent to the AI
                    let processed_content = resolve_user_mentions(ctx, &entry.content).await;
                    let processed_message = ChatMessage {
                        role: entry.role.clone(),
                        content: processed_content,
                    };
                    messages.push(processed_message);
                    println!("[DEBUG][HANDLE_LM] Added processed context message {}: {} ({} chars)", 
                        i + 1, entry.role, entry.content.len());
                }
                println!("[DEBUG][HANDLE_LM] Loaded and processed {} context messages for user {}", 
                    conversation_messages.len(), msg.author.name);
            } else {
                println!("[DEBUG][HANDLE_LM] No context found for user {}", msg.author.name);
            }
        } else {
            println!("[DEBUG][HANDLE_LM] LM context map not found");
        }
    }
    
    messages.push(ChatMessage { role: "user".to_string(), content: final_prompt.clone() });
    println!("[DEBUG][HANDLE_LM] Added final user message: {} chars", final_prompt.len());
    
    let multimodal_messages = convert_to_multimodal(messages);
    println!("[DEBUG][HANDLE_LM] Converted to {} multimodal messages", multimodal_messages.len());
    
    // Log which model is being used for LM command
    println!("[DEBUG][HANDLE_LM] === API PREPARATION ===");
    println!("[DEBUG][HANDLE_LM] Using model '{}' for chat", config.default_model);
    if !processed_documents.is_empty() {
        println!("[DEBUG][HANDLE_LM] Using document-enhanced prompt with {} documents", processed_documents.len());
    }
    
    println!("[DEBUG][HANDLE_LM] === SENDING INITIAL MESSAGE ===");
    let mut initial_msg = msg.channel_id.send_message(&ctx.http, |m| {
        let content = if !processed_documents.is_empty() {
            format!("**AI Response (Document Analysis - Part 1):**\n```\n\n```")
        } else {
            "**AI Response (Part 1):**\n```\n\n```".to_string()
        };
        m.content(content)
    }).await?;
    println!("[DEBUG][HANDLE_LM] Initial Discord message sent successfully");
    
    println!("[DEBUG][HANDLE_LM] === STARTING STREAMING ===");
    let _stats = stream_chat_response(multimodal_messages, &config.default_model, &config, ctx, &mut initial_msg).await?;
    println!("[DEBUG][HANDLE_LM] Streaming completed successfully");
    
    // Record response in history (similar to lm)
    println!("[DEBUG][HANDLE_LM] === RECORDING RESPONSE ===");
    let response_content = initial_msg.content.clone();
    println!("[DEBUG][HANDLE_LM] Response content length: {} chars", response_content.len());
    
    // Check for empty response content
    if response_content.trim().is_empty() || response_content.len() < 10 {
        eprintln!("[DEBUG][HANDLE_LM] ERROR: Final Discord message has insufficient content");
        eprintln!("[DEBUG][HANDLE_LM] Response content: '{}'", response_content);
        eprintln!("[DEBUG][HANDLE_LM] Content length: {} chars", response_content.len());
        return Err("API response resulted in empty or insufficient content - this indicates a problem with the streaming or API".into());
    }
    
    {
        let mut data_map = ctx.data.write().await;
        let lm_map = data_map.get_mut::<LmContextMap>().expect("LM context map not initialized");
        if let Some(context) = lm_map.get_mut(&msg.author.id) {
            // Log current context state before adding assistant message
            println!("[DEBUG][HANDLE_LM] Current context state before adding assistant message: {}", context.get_context_info());
            
            // Force cleanup if context is getting too large
            if context.needs_cleanup() {
                println!("[DEBUG][HANDLE_LM] Context is large, forcing cleanup before adding assistant message");
                context.force_cleanup();
            }
            
            context.add_assistant_message(ChatMessage { role: "assistant".to_string(), content: response_content });
            
            // Log context state after adding assistant message
            println!("[DEBUG][HANDLE_LM] Context after adding assistant message: {}", context.get_context_info());
            
            // Check if context needs cleanup
            if context.needs_cleanup() {
                println!("[DEBUG][HANDLE_LM] Context is getting large after adding assistant message");
            }
        }
    }
    
    println!("[DEBUG][HANDLE_LM] === LM REQUEST COMPLETED ===");
    Ok(())
}

#[command]
#[aliases("llm", "ai", "chat")]
/// Main ^lm command handler
/// Handles user prompts, vision analysis, document RAG, and AI-enhanced search
/// Supports:
///   - ^lm <prompt> (AI chat)
///   - ^lm -v <prompt> (vision analysis)
///   - ^lm -s <query> (AI-enhanced search)
///   - ^lm --test (API connectivity test)
///   - ^lm --clear (clear context)
pub async fn lm(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    println!("[DEBUG][LM] === LM COMMAND STARTED ===");
    println!("[DEBUG][LM] User: {} (ID: {})", msg.author.name, msg.author.id);
    println!("[DEBUG][LM] Channel: {} (ID: {})", msg.channel_id, msg.channel_id.0);
    
    let mut input = args.message().trim().to_string();
    println!("[DEBUG][LM] Raw input: '{}'", input);
    
    // Start typing indicator
    println!("[DEBUG][LM] Starting typing indicator");
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    
    // IMPORTANT: Check for vision flag BEFORE processing reply logic
    // This ensures we detect -v flag even in replies
    let is_vision_request = input.starts_with("-v") || input.starts_with("--vision");
    let original_input = input.clone(); // Store original input for vision processing
    println!("[DEBUG][LM] Vision request detected: {}", is_vision_request);
    println!("[DEBUG][LM] Original input stored: '{}'", original_input);
    
    // Check if this is a reply and handle it appropriately
    if let Some(referenced_message) = &msg.referenced_message {
        println!("[DEBUG][LM] === REPLY DETECTED ===");
        println!("[DEBUG][LM] Reply to message from: {} (ID: {})", referenced_message.author.name, referenced_message.author.id);
        println!("[DEBUG][LM] Referenced message content: '{}'", referenced_message.content);
        
        // Only modify input for non-vision requests
        if !is_vision_request {
            println!("[DEBUG][LM] Processing as non-vision reply");
            // If the reply has no content, use the referenced message content as the prompt
            if input.is_empty() {
                input = referenced_message.content.clone();
                println!("[DEBUG][LM] Empty reply - using referenced message as prompt: '{}'", input);
            } else {
                // If the reply has content, combine it with the referenced message
                input = format!("Original message: {}\n\nYour response: {}", referenced_message.content, input);
                println!("[DEBUG][LM] Combined reply with referenced message");
            }
        } else {
            println!("[DEBUG][LM] Vision request in reply - keeping original input: '{}'", original_input);
        }
    } else {
        println!("[DEBUG][LM] No reply detected - direct message");
    }
    
    println!("[DEBUG][LM] Final processed input: '{}'", input);
    
    if input.is_empty() {
        println!("[DEBUG][LM] Input is empty after processing, sending usage message");
        msg.reply(ctx, "Please provide a prompt! Usage: `^lm <your prompt>` or `^lm -s <search query>`\n\nYou can also reply to a message with `^lm` to respond to that message.").await?;
        return Ok(());
    }

    // Skip context logic for search and test flags
    if input.starts_with("-s ") || input.starts_with("--search ") {
        println!("[DEBUG][LM] === SEARCH REQUEST DETECTED ===");
        // Extract search query
        let search_query = if input.starts_with("-s ") {
            &input[3..]
        } else {
            &input[9..] // "--search "
        };
        println!("[DEBUG][LM] Extracted search query: '{}'", search_query);

        if search_query.trim().is_empty() {
            println!("[DEBUG][LM] Search query is empty, sending error");
            msg.reply(ctx, "Please provide a search query! Usage: `^lm -s <search query>`").await?;
            return Ok(());
        }

        // Load LM Studio configuration for AI-enhanced search
        println!("[DEBUG][LM] Loading LM config for search");
        let config = match load_lm_config().await {
            Ok(config) => {
                println!("[DEBUG][LM] LM config loaded successfully");
                config
            },
            Err(e) => {
                eprintln!("[DEBUG][LM] Failed to load LM Studio configuration for search: {}", e);
                msg.reply(ctx, &format!("LM Studio configuration error: {}\n\nMake sure `lmapiconf.txt` exists and contains all required settings. Check `example_lmapiconf.txt` for reference.", e)).await?;
                return Ok(());
            }
        };

        // Send initial search message
        println!("[DEBUG][LM] Sending initial search message");
        let mut search_msg = match msg.channel_id.send_message(&ctx.http, |m| {
            m.content("Refining search query...")
        }).await {
            Ok(message) => {
                println!("[DEBUG][LM] Initial search message sent successfully");
                message
            },
            Err(e) => {
                eprintln!("[DEBUG][LM] Failed to send initial search message: {}", e);
                msg.reply(ctx, "Failed to send message!").await?;
                return Ok(());
            }
        };

        // AI-Enhanced Search Flow
        println!("[DEBUG][LM] Starting AI-enhanced search");
        match perform_ai_enhanced_search(search_query, &config, &mut search_msg, ctx).await {
            Ok(()) => {
                println!("[DEBUG][LM] AI-enhanced search completed successfully for query: '{}'", search_query);
            }
            Err(e) => {
                eprintln!("[DEBUG][LM] AI-enhanced search failed: {}", e);
                let error_msg = format!("**Search Failed**\n\nQuery: `{}`\nError: {}\n\nCheck your SerpAPI configuration in lmapiconf.txt", search_query, e);
                let _ = search_msg.edit(&ctx.http, |m| {
                    m.content(&error_msg)
                }).await;
            }
        }

        return Ok(());
    }

    if input.starts_with("--test") || input == "-t" {
        println!("[DEBUG][LM] === TEST REQUEST DETECTED ===");
        // Load LM Studio configuration for connectivity test
        println!("[DEBUG][LM] Loading LM config for test");
        let config = match load_lm_config().await {
            Ok(config) => {
                println!("[DEBUG][LM] LM config loaded successfully");
                config
            },
            Err(e) => {
                eprintln!("[DEBUG][LM] Failed to load LM Studio configuration for test: {}", e);
                msg.reply(ctx, &format!("LM Studio configuration error: {}\n\nMake sure `lmapiconf.txt` exists and contains all required settings. Check `example_lmapiconf.txt` for reference.", e)).await?;
                return Ok(());
            }
        };

        // Send initial test message
        println!("[DEBUG][LM] Sending initial test message");
        let mut test_msg = match msg.channel_id.send_message(&ctx.http, |m| {
            m.content("Testing API connectivity to remote server...")
        }).await {
            Ok(message) => {
                println!("[DEBUG][LM] Initial test message sent successfully");
                message
            },
            Err(e) => {
                eprintln!("[DEBUG][LM] Failed to send initial test message: {}", e);
                msg.reply(ctx, "Failed to send message!").await?;
                return Ok(());
            }
        };

        // Perform connectivity test
        println!("[DEBUG][LM] Starting connectivity test");
        match crate::commands::search::test_api_connectivity(&config).await {
            Ok(success_message) => {
                println!("[DEBUG][LM] Connectivity test successful");
                let final_message = format!("**Connectivity Test Results**\n\n{:?}\n\n**Configuration:**\n- Base URL: {}\n- Default Model: {}\n- Default Reason Model: {}\n- Timeout: {}s\n- Max Tokens: {}\n- Temperature: {}\n- Vision Model: {}\n",
                    success_message, config.base_url, config.default_model, config.default_reason_model, config.timeout, config.default_max_tokens, config.default_temperature, config.default_vision_model
                );
                
                if let Err(e) = test_msg.edit(&ctx.http, |m| {
                    m.content(&final_message)
                }).await {
                    eprintln!("[DEBUG][LM] Failed to update test message: {}", e);
                }
            }
            Err(e) => {
                println!("[DEBUG][LM] Connectivity test failed: {}", e);
                let error_message = format!("**Connectivity Test Failed**\n\n**Error:** {}\n\n**Troubleshooting:**\n• Check if LM Studio/Ollama is running on `{}`\n• Verify the model `{}` is loaded\n• Check firewall settings\n• Ensure the server is accessible from this network\n\n**Configuration:**\n• Server: `{}`\n• Default Model: `{}`\n• Timeout: `{}s`", 
                    e, config.base_url, config.default_model, config.base_url, config.default_model, config.timeout);
                
                if let Err(edit_error) = test_msg.edit(&ctx.http, |m| {
                    m.content(&error_message)
                }).await {
                    eprintln!("[DEBUG][LM] Failed to update test message with error: {}", edit_error);
                }
            }
        }

        return Ok(());
    }

    // Check if this is a clear context request
    if input.starts_with("--clear") || input == "-c" {
        println!("[DEBUG][LM] === CLEAR CONTEXT REQUEST DETECTED ===");
        let mut data_map = ctx.data.write().await;
        let lm_map = data_map.get_mut::<LmContextMap>().expect("LM context map not initialized");
        
        let had_context = if let Some(context) = lm_map.get_mut(&msg.author.id) {
            let message_count = context.total_messages();
            println!("[DEBUG][LM] Clearing context for user {} (had {} messages)", msg.author.name, message_count);
            context.clear();
            message_count > 0
        } else {
            println!("[DEBUG][LM] No context found for user {}", msg.author.name);
            false
        };
        
        if had_context {
            println!("[DEBUG][LM] Context cleared successfully");
            msg.reply(ctx, "**LM Chat Context Cleared** ✅\nYour conversation history has been reset (50 user messages + 50 assistant messages).").await?;
        } else {
            println!("[DEBUG][LM] No context to clear");
            msg.reply(ctx, "**No LM Context Found** ℹ️\nYou don't have any active conversation history to clear.").await?;
        }
        return Ok(());
    }

    // Handle vision flag - use original input to preserve the flag
    if is_vision_request {
        println!("[DEBUG][LM] === VISION REQUEST IN MAIN COMMAND ===");
        println!("[DEBUG][LM] Vision flag detected in original input: '{}'", original_input);
        
        let vision_prompt = if original_input.starts_with("-v") {
            let after_flag = if original_input.starts_with("-v ") {
                &original_input[3..] // "-v "
            } else {
                &original_input[2..] // "-v"
            };
            after_flag.trim().to_string()
        } else {
            let after_flag = if original_input.starts_with("--vision ") {
                &original_input[9..] // "--vision "
            } else {
                &original_input[8..] // "--vision"
            };
            after_flag.trim().to_string()
        };
        
        println!("[DEBUG][LM] Extracted vision prompt: '{}'", vision_prompt);

        if vision_prompt.is_empty() {
            println!("[DEBUG][LM] Vision prompt is empty, returning error");
            msg.reply(ctx, "Please provide a prompt for vision analysis! Usage: `^lm -v <prompt>` with image attached.").await?;
            return Ok(());
        }

        // Check for attachments in current message first
        println!("[DEBUG][LM] Checking for attachments in vision request");
        let attachment_to_process = if !msg.attachments.is_empty() {
            println!("[DEBUG][LM] Found {} attachments in current message", msg.attachments.len());
            Some(&msg.attachments[0])
        } else if let Some(referenced_msg) = &msg.referenced_message {
            println!("[DEBUG][LM] No local attachments, checking referenced message...");
            if !referenced_msg.attachments.is_empty() {
                println!("[DEBUG][LM] Found {} attachments in referenced message", referenced_msg.attachments.len());
                Some(&referenced_msg.attachments[0])
            } else {
                println!("[DEBUG][LM] No attachments found in referenced message");
                None
            }
        } else {
            println!("[DEBUG][LM] No attachments found and no referenced message");
            None
        };

        let attachment = match attachment_to_process {
            Some(att) => {
                println!("[DEBUG][LM] Using attachment: {} ({})", att.filename, att.content_type.as_deref().unwrap_or("unknown"));
                att
            },
            None => {
                println!("[DEBUG][LM] No image attachments found in current or referenced message");
                msg.reply(ctx, "Please attach an image for vision analysis, or reply to a message with an image attachment.").await?;
                return Ok(());
            }
        };

        let content_type = attachment.content_type.as_deref().unwrap_or("");
        println!("[DEBUG][LM] Found attachment: {} (content_type: {})", attachment.filename, content_type);

        if !content_type.starts_with("image/") {
            println!("[DEBUG][LM] Attachment is not an image, returning error");
            msg.reply(ctx, "Attached file is not an image. Please attach a valid image file.").await?;
            return Ok(());
        }

        println!("[DEBUG][LM] Calling vis::handle_vision_request...");
        if let Err(e) = crate::commands::vis::handle_vision_request(ctx, msg, &vision_prompt, attachment).await {
            println!("[DEBUG][LM] Vision request failed with error: {}", e);
            msg.reply(ctx, format!("Vision analysis error: {}", e)).await?;
        } else {
            println!("[DEBUG][LM] Vision request completed successfully");
        }

        return Ok(());
    }

    // For regular AI chat functionality, delegate to handle_lm_request to avoid double posting
    // This ensures only one initial message is sent per request
    println!("[DEBUG][LM] === DELEGATING TO HANDLE_LM_REQUEST ===");
    println!("[DEBUG][LM] Delegating regular chat request to handle_lm_request");
    if let Err(e) = handle_lm_request(ctx, msg, &input, Some(&input)).await {
        eprintln!("[DEBUG][LM] handle_lm_request failed: {}", e);
        msg.reply(ctx, format!("LM error: {}", e)).await?;
    } else {
        println!("[DEBUG][LM] handle_lm_request completed successfully");
    }

    println!("[DEBUG][LM] === LM COMMAND COMPLETED ===");
    Ok(())
}

#[command]
#[aliases("clearlm", "resetlm")]
/// Command to clear the user's LM chat context
/// Removes all conversation history for the user
pub async fn clearcontext(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    // Clear the user's LM chat context robustly
    let mut data_map = ctx.data.write().await;
    let lm_map = data_map.get_mut::<LmContextMap>().expect("LM context map not initialized");
    
    let user_id = msg.author.id;
    let had_context = if let Some(context) = lm_map.get_mut(&user_id) {
        let message_count = context.total_messages();
        let context_info = context.get_context_info();
        println!("[clearcontext] Clearing context for user {}: {}", user_id, context_info);
        context.clear();
        message_count > 0
    } else {
        false
    };
    
    println!("[clearcontext] Cleared context for user {} (had_context={})", user_id, had_context);
    
    if had_context {
        msg.reply(ctx, "**LM Chat Context Cleared** ✅\nYour conversation history with the AI has been fully reset (250 user messages + 250 assistant messages). The next message you send will start a brand new context.").await?;
    } else {
        msg.reply(ctx, "**No Context Found** ℹ️\nYou don't have any active conversation history to clear. Start a conversation with `^lm <your message>`.\n\nIf you believe context is still being used, please report this as a bug.").await?;
    }
    
    Ok(())
}

// Process Discord attachments for RAG (Retrieval-Augmented Generation)
// Downloads, extracts, and returns content from supported file types
async fn process_attachments(
    attachments: &[Attachment],
    _ctx: &Context,
) -> Result<Vec<ProcessedDocument>, Box<dyn std::error::Error + Send + Sync>> {
    let mut processed_docs = Vec::new();
    
    for attachment in attachments {
        let content_type = attachment.content_type.as_deref().unwrap_or("unknown");
        let size = attachment.size as usize;
        
        println!("[RAG] Processing attachment: {} ({} bytes, MIME: {})", 
            attachment.filename, size, content_type);
        
        // Check if the attachment is a supported format
        if !is_supported_format(content_type, &attachment.filename) {
            println!("[RAG] Skipping unsupported format: {}", content_type);
            continue;
        }
        
        // Download the attachment
        let temp_file = format!("temp_{}_{}", Uuid::new_v4(), attachment.filename);
        let temp_path = Path::new(&temp_file);
        
        println!("[RAG] Downloading attachment to: {}", temp_file);
        
        // Download the file
        let response = reqwest::get(&attachment.url).await?;
        let bytes = response.bytes().await?;
        
        // Write to temporary file
        let mut file = std::fs::File::create(temp_path)?;
        file.write_all(&bytes)?;
        drop(file); // Close the file
        
        // Process the document based on its type
        let content = match extract_document_content(temp_path, content_type).await {
            Ok(content) => content,
            Err(e) => {
                println!("[RAG] Failed to extract content from {}: {}", attachment.filename, e);
                // Clean up temp file
                let _ = std::fs::remove_file(temp_path);
                continue;
            }
        };
        
        // Clean up temp file
        let _ = std::fs::remove_file(temp_path);
        
        if !content.trim().is_empty() {
            processed_docs.push(ProcessedDocument {
                filename: attachment.filename.clone(),
                content: content.clone(),
                content_type: content_type.to_string(),
                size: size as usize,
            });
            println!("[RAG] Successfully processed {}: {} characters", 
                attachment.filename, content.len());
        }
    }
    
    Ok(processed_docs)
}

// Check if a file format is supported for processing (RAG)
// Returns true if the file is a supported type/extension
fn is_supported_format(content_type: &str, filename: &str) -> bool {
    let supported_types = [
        "text/plain", "text/markdown", "text/csv", "text/html",
        "application/pdf", "application/json", "application/xml",
        "image/jpeg", "image/png", "image/gif", "image/webp"
    ];
    
    let supported_extensions = [
        ".txt", ".md", ".csv", ".html", ".htm", ".json", ".xml",
        ".pdf", ".jpg", ".jpeg", ".png", ".gif", ".webp"
    ];
    
    // Check MIME type
    if supported_types.iter().any(|&t| content_type.starts_with(t)) {
        return true;
    }
    
    // Check file extension as fallback
    let lower_filename = filename.to_lowercase();
    supported_extensions.iter().any(|ext| lower_filename.ends_with(ext))
}

// Extract content from different document types (RAG)
// Handles text, PDF, and image files (placeholder for images)
async fn extract_document_content(
    file_path: &Path,
    content_type: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    match content_type {
        "text/plain" | "text/markdown" | "text/csv" | "text/html" | "application/json" | "application/xml" => {
            // Text-based files
            let content = std::fs::read_to_string(file_path)?;
            Ok(content)
        },
        "application/pdf" => {
            // PDF files
            extract_pdf_content(file_path).await
        },
        content_type if content_type.starts_with("image/") => {
            // Image files - for now, just return a placeholder
            // In a full implementation, you'd use OCR or image analysis
            Ok(format!("[Image file: {} - Content analysis not yet implemented]", 
                file_path.file_name().unwrap_or_default().to_string_lossy()))
        },
        _ => {
            // Try to read as text anyway
            match std::fs::read_to_string(file_path) {
                Ok(content) => Ok(content),
                Err(_) => Err("Unsupported file format".into())
            }
        }
    }
}

// Extract text content from PDF files (RAG)
async fn extract_pdf_content(file_path: &Path) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use pdf_extract::extract_text;
    
    let content = extract_text(file_path)?;
    Ok(content)
}

// Convert regular ChatMessage to MultimodalChatMessage (for vision/RAG)
fn convert_to_multimodal(messages: Vec<ChatMessage>) -> Vec<MultimodalChatMessage> {
    messages.into_iter().map(|msg| MultimodalChatMessage {
        role: msg.role,
        content: vec![MessageContent::Text {
            content_type: "text".to_string(),
            text: msg.content,
        }],
    }).collect()
}

// Create RAG-enhanced prompt with document context
// Formats user prompt and document content for AI analysis
fn create_rag_prompt(user_prompt: &str, documents: &[ProcessedDocument]) -> String {
    if documents.is_empty() {
        return user_prompt.to_string();
    }
    
    let mut context = String::new();
    context.push_str("**Document Context:**\n\n");
    
    for (i, doc) in documents.iter().enumerate() {
        context.push_str(&format!("**Document {}: {}**\n", i + 1, doc.filename));
        context.push_str(&format!("Type: {}\n", doc.content_type));
        context.push_str(&format!("Size: {} characters\n\n", doc.content.len()));
        
        // Truncate very long documents to prevent token overflow
        let content = if doc.content.len() > 8000 {
            format!("{}... [Content truncated due to length]", &doc.content[..8000])
        } else {
            doc.content.clone()
        };
        
        context.push_str(&format!("Content:\n{}\n\n", content));
    }
    
    format!("{}\n\n**User Question:** {}\n\nPlease analyze the provided documents and answer the user's question based on the content.", 
        context, user_prompt)
}

// Main streaming function that handles real-time response with Discord message editing for chat
// Streams the AI's chat response, chunking and updating Discord messages as needed
pub async fn stream_chat_response(
    messages: Vec<MultimodalChatMessage>,
    model: &str,
    config: &LMConfig,
    ctx: &Context,
    initial_msg: &mut Message,
) -> Result<StreamingStats, Box<dyn std::error::Error + Send + Sync>> {
    println!("[DEBUG][STREAMING] === STARTING STREAM CHAT RESPONSE ===");
    println!("[DEBUG][STREAMING] Model: {}", model);
    println!("[DEBUG][STREAMING] Messages count: {}", messages.len());
    println!("[DEBUG][STREAMING] Base URL: {}", config.base_url);
    println!("[DEBUG][STREAMING] Timeout: {} seconds", config.timeout);
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    println!("[DEBUG][STREAMING] HTTP client created");
        
    let chat_request = ChatRequest {
        model: model.to_string(),
        messages,
        temperature: config.default_temperature,
        max_tokens: config.default_max_tokens,
        stream: true,
    };
    println!("[DEBUG][STREAMING] Chat request created - Temperature: {}, Max tokens: {}, Stream: {}", 
        chat_request.temperature, chat_request.max_tokens, chat_request.stream);

    let api_url = format!("{}/v1/chat/completions", config.base_url);
    println!("[DEBUG][STREAMING] API URL: {}", api_url);

    // First, test basic connectivity to the server with enhanced error handling
    println!("[DEBUG][STREAMING] === TESTING BASIC CONNECTIVITY ===");
    match client.get(&config.base_url).send().await {
        Ok(response) => {
            println!("[DEBUG][STREAMING] Basic connectivity test successful - Status: {}", response.status());
        }
        Err(e) => {
            println!("[DEBUG][STREAMING] Basic connectivity test failed: {}", e);
            
            // Check if this is a Windows permission error
            let error_msg = format!("{}", e);
            if error_msg.contains("os error 10013") || error_msg.contains("access permissions") {
                return Err(format!(
                    "Windows Network Permission Error (10013): Cannot connect to {}.\n\n**Common Solutions:**\n\
                    1. **Windows Firewall:** Allow the application through Windows Defender Firewall\n\
                    2. **Network Access:** Ensure the AI server at {} is running and accessible\n\
                    3. **Port Access:** Check if port 11434 is blocked by antivirus or firewall\n\
                    4. **Local Network:** Try using localhost (127.0.0.1) instead of {} if running locally\n\
                    5. **Administrator:** Try running as administrator if needed\n\n\
                    **Original error:** {}", 
                    config.base_url, config.base_url, config.base_url.replace("http://", ""), e
                ).into());
            } else if error_msg.contains("timeout") || error_msg.contains("timed out") {
                return Err(format!(
                    "Connection Timeout: Cannot reach AI server at {}.\n\n**Check:**\n\
                    1. AI server is running and accessible\n\
                    2. Network connection is stable\n\
                    3. Server is not overloaded\n\n\
                    **Original error:** {}", 
                    config.base_url, e
                ).into());
            } else if error_msg.contains("refused") || error_msg.contains("connection refused") {
                return Err(format!(
                    "Connection Refused: AI server at {} is not accepting connections.\n\n**Check:**\n\
                    1. AI server (LM Studio/Ollama) is running\n\
                    2. Server is listening on the correct port\n\
                    3. No other application is using the port\n\n\
                    **Original error:** {}", 
                    config.base_url, e
                ).into());
            } else {
                return Err(format!("Cannot reach remote server {}: {}", config.base_url, e).into());
            }
        }
    }

    // Now attempt the actual streaming API call
    println!("[DEBUG][STREAMING] === MAKING STREAMING API REQUEST ===");
    let response = match client
        .post(&api_url)
        .json(&chat_request)
        .send()
        .await
    {
        Ok(resp) => {
            println!("[DEBUG][STREAMING] API request sent successfully - Status: {}", resp.status());
            resp
        }
        Err(e) => {
            println!("[DEBUG][STREAMING] API request failed: {}", e);
            
            // Enhanced error handling for API requests
            let error_msg = format!("{}", e);
            if error_msg.contains("os error 10013") || error_msg.contains("access permissions") {
                return Err(format!(
                    "Windows Network Permission Error (10013): Cannot connect to AI API at {}.\n\n**Solutions:**\n\
                    1. **Windows Firewall:** Add firewall exception for this application\n\
                    2. **Run as Administrator:** Try running the bot as administrator\n\
                    3. **Check AI Server:** Ensure LM Studio/Ollama is running at {}\n\
                    4. **Port Access:** Verify port 11434 isn't blocked\n\
                    5. **Network Config:** Try localhost (127.0.0.1) if running locally\n\n\
                    **Original error:** {}", 
                    api_url, config.base_url, e
                ).into());
            } else {
                return Err(format!("Streaming API request to {} failed: {}", api_url, e).into());
            }
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
        println!("[DEBUG][STREAMING] API returned error status {}: {}", status, error_text);
        return Err(format!("Streaming API request failed: HTTP {} - {}", status, error_text).into());
    }

    println!("[DEBUG][STREAMING] === BUFFERING COMPLETE RESPONSE ===");
    let mut stream = response.bytes_stream();
    
    let mut raw_response = String::new();
    let mut chunk_count = 0;
    let mut line_buffer = String::new();
    let mut received_any_content = false;
    let mut stream_complete = false;

    println!("[DEBUG][STREAMING] Starting to buffer response from API...");

    // STEP 1: Buffer the complete response from the API
    while let Some(chunk) = stream.next().await {
        if stream_complete {
            println!("[DEBUG][STREAMING] Stream marked as complete, stopping buffering");
            break;
        }
        
        match chunk {
            Ok(bytes) => {
                chunk_count += 1;
                if chunk_count == 1 {
                    println!("[DEBUG][STREAMING] Received first chunk ({} bytes)", bytes.len());
                } else if chunk_count % 10 == 0 {
                    println!("[DEBUG][STREAMING] Buffered {} chunks, total response: {} chars", chunk_count, raw_response.len());
                }
                
                line_buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(i) = line_buffer.find('\n') {
                    let line = line_buffer.drain(..=i).collect::<String>();
                    let line = line.trim();

                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str.trim() == "[DONE]" {
                            println!("[DEBUG][STREAMING] Received [DONE] signal, marking stream complete");
                            stream_complete = true;
                            break;
                        }

                        if let Ok(response_chunk) = serde_json::from_str::<ChatResponse>(json_str) {
                            for choice in response_chunk.choices {
                                if let Some(finish_reason) = choice.finish_reason {
                                    if finish_reason == "stop" {
                                        println!("[DEBUG][STREAMING] Received finish_reason=stop, marking stream complete");
                                        stream_complete = true;
                                        break;
                                    }
                                }

                                if let Some(delta) = choice.delta {
                                    if let Some(content) = delta.content {
                                        received_any_content = true;
                                        raw_response.push_str(&content);
                                        println!("[DEBUG][STREAMING] Added content chunk: '{}' (total: {} chars)", 
                                            content, raw_response.len());
                                    } else {
                                        println!("[DEBUG][STREAMING] Delta has no content field");
                                    }
                                } else {
                                    println!("[DEBUG][STREAMING] Choice has no delta field");
                                }
                            }
                        } else {
                            if !json_str.trim().is_empty() {
                                println!("[DEBUG][STREAMING] Failed to parse JSON chunk: {}", json_str);
                            }
                        }
                    }
                }
                
                // Break out of the outer loop if stream is complete
                if stream_complete {
                    println!("[DEBUG][STREAMING] Breaking out of chunk processing loop");
                    break;
                }
            }
            Err(e) => {
                eprintln!("[DEBUG][STREAMING] Stream error: {}", e);
                return Err(e.into());
            }
        }
    }

    println!("[DEBUG][STREAMING] === BUFFERING COMPLETE ===");
    println!("[DEBUG][STREAMING] Buffered {} chunks, total response: {} chars", chunk_count, raw_response.len());
    println!("[DEBUG][STREAMING] Raw response content: '{}'", raw_response);
    
    if !received_any_content {
        eprintln!("[DEBUG][STREAMING] No content received from API stream");
        return Err("No content received from API stream".into());
    }
    
    // Check for zero character response
    if raw_response.is_empty() {
        eprintln!("[DEBUG][STREAMING] ERROR: API returned 0 characters in response");
        eprintln!("[DEBUG][STREAMING] Chunk count: {}", chunk_count);
        eprintln!("[DEBUG][STREAMING] Received any content flag: {}", received_any_content);
        return Err("API returned 0 characters in response - this indicates a problem with the API or model".into());
    }

    // STEP 2: Stream the buffered content to Discord
    println!("[DEBUG][STREAMING] === STREAMING TO DISCORD ===");
    let mut message_state = MessageState {
        current_content: String::new(),
        current_message: initial_msg.clone(),
        message_index: 1,
        char_limit: config.max_discord_message_length - config.response_format_padding,
    };
    println!("[DEBUG][STREAMING] Message state initialized - Char limit: {}", message_state.char_limit);
    
    // Split the response into chunks for Discord streaming
    let chunk_size = 100; // Characters per Discord update
    let mut chars_processed = 0;
    
    while chars_processed < raw_response.len() {
        let end_pos = std::cmp::min(chars_processed + chunk_size, raw_response.len());
        let chunk = &raw_response[chars_processed..end_pos];
        
        println!("[DEBUG][STREAMING] Streaming chunk {} chars to Discord", chunk.len());
        println!("[DEBUG][STREAMING] Chunk content: '{}'", chunk);
        println!("[DEBUG][STREAMING] Current state content before update: {} chars", message_state.current_content.len());
        println!("[DEBUG][STREAMING] Current state content before update: '{}'", message_state.current_content);
        
        if let Err(e) = update_chat_message(&mut message_state, chunk, ctx, config).await {
            eprintln!("[DEBUG][STREAMING] Failed to update Discord message: {}", e);
            return Err(e);
        }
        
        println!("[DEBUG][STREAMING] Current state content after update: {} chars", message_state.current_content.len());
        println!("[DEBUG][STREAMING] Current state content after update: '{}'", message_state.current_content);
        
        chars_processed = end_pos;
        
        // Small delay to make streaming visible
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Finalize the message
    println!("[DEBUG][STREAMING] === FINALIZING DISCORD MESSAGE ===");
    println!("[DEBUG][STREAMING] About to finalize with buffered content length: {} chars", raw_response.len());
    println!("[DEBUG][STREAMING] Final message state content: {} chars", message_state.current_content.len());
    println!("[DEBUG][STREAMING] Final message state content: '{}'", message_state.current_content);
    
    // Check if we have content to finalize
    if raw_response.is_empty() {
        eprintln!("[DEBUG][STREAMING] ERROR: Cannot finalize - no content was buffered from API");
        return Err("No content was buffered from API - cannot finalize empty message".into());
    }
    
    // Check if the message state has content (this should catch streaming issues)
    if message_state.current_content.trim().is_empty() {
        eprintln!("[DEBUG][STREAMING] ERROR: Message state has no content despite buffered response");
        eprintln!("[DEBUG][STREAMING] Raw response length: {} chars", raw_response.len());
        eprintln!("[DEBUG][STREAMING] Message state content length: {} chars", message_state.current_content.len());
        return Err("Message state has no content despite buffered response - streaming to Discord failed".into());
    }
    
    if let Err(e) = finalize_chat_message(&mut message_state, "", ctx, config).await {
        eprintln!("[DEBUG][STREAMING] Failed to finalize Discord message: {}", e);
        return Err(e);
    }

    let stats = StreamingStats {
        total_characters: raw_response.len(),
        message_count: message_state.message_index,
    };

    println!("[DEBUG][STREAMING] === STREAMING COMPLETED ===");
    println!("[DEBUG][STREAMING] Final stats - Total chars: {}, Messages: {}", 
        stats.total_characters, stats.message_count);
    Ok(stats)
}

// Helper function to update Discord message with new content for chat
// Handles chunking and message creation if content exceeds Discord's limit
#[allow(unused_variables)]
pub async fn update_chat_message(
    state: &mut MessageState,
    new_content: &str,
    ctx: &Context,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("[DEBUG][UPDATE] Updating Discord message with {} chars", new_content.len());
    
    println!("[DEBUG][UPDATE] New content to add: '{}' ({} chars)", new_content, new_content.len());
    
    // First, add the new content to the state
    if state.current_content.is_empty() {
        println!("[DEBUG][UPDATE] State content was empty, setting to new content");
        state.current_content = new_content.to_string();
    } else {
        println!("[DEBUG][UPDATE] State content was not empty, appending new content");
        state.current_content.push_str(new_content);
    }
    
    println!("[DEBUG][UPDATE] State content after adding: '{}' ({} chars)", state.current_content, state.current_content.len());
    
    // Then create the formatted content for Discord
    let potential_content = format!("**AI Response (Part {}):**\n```\n{}\n```", 
        state.message_index, state.current_content);
    
    println!("[DEBUG][UPDATE] Formatted content for Discord: '{}' ({} chars)", potential_content, potential_content.len());

    // Check if we need to create a new message
    if potential_content.len() > state.char_limit {
        println!("[DEBUG][UPDATE] Content exceeds limit ({} > {}), creating new message", 
            potential_content.len(), state.char_limit);
        
        // Finalize current message
        let final_content = format!("**AI Response (Part {}):**\n```\n{}\n```", 
            state.message_index, state.current_content);
        let edit_result = state.current_message.edit(&ctx.http, |m| {
            m.content(final_content)
        }).await;
        if let Err(e) = edit_result {
            eprintln!("[ERROR][UPDATE] Failed to finalize message part {}: {}", state.message_index, e);
        } else {
            println!("[DEBUG][UPDATE] Finalized message part {}", state.message_index);
        }

        // Create new message
        state.message_index += 1;
        // Reset current_content for the new message
        state.current_content = new_content.to_string();
        let new_msg_content = format!("**AI Response (Part {}):**\n```\n{}\n```", 
            state.message_index, state.current_content);
        let send_result = state.current_message.channel_id.send_message(&ctx.http, |m| {
            m.content(new_msg_content)
        }).await;
        match send_result {
            Ok(new_message) => {
                println!("[DEBUG][UPDATE] Created new message part {}", state.message_index);
                state.current_message = new_message;
            }
            Err(e) => {
                eprintln!("[ERROR][UPDATE] Failed to create new message part {}: {}", state.message_index, e);
            }
        }
    } else {
        // Update current message
        println!("[DEBUG][UPDATE] Updating existing message part {}", state.message_index);
        let edit_result = state.current_message.edit(&ctx.http, |m| {
            m.content(&potential_content)
        }).await;
        if let Err(e) = edit_result {
            eprintln!("[ERROR][UPDATE] Failed to update existing message part {}: {}", state.message_index, e);
        }
    }

    Ok(())
}

// Helper function to finalize message content at the end of streaming for chat
// Ensures all remaining content is posted and marks the message as complete
#[allow(unused_variables)]
pub async fn finalize_chat_message(
    state: &mut MessageState,
    remaining_content: &str,
    ctx: &Context,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("[DEBUG][FINALIZE] Finalizing message with {} chars", remaining_content.len());
    println!("[DEBUG][FINALIZE] Current state content: {} chars", state.current_content.len());
    
    // Check for zero content error condition - this should catch cases where API returned content but it wasn't streamed properly
    if remaining_content.is_empty() && state.current_content.trim().is_empty() {
        eprintln!("[DEBUG][FINALIZE] ERROR: Attempting to finalize message with 0 total characters");
        eprintln!("[DEBUG][FINALIZE] Remaining content: '{}' ({} chars)", remaining_content, remaining_content.len());
        eprintln!("[DEBUG][FINALIZE] State content: '{}' ({} chars)", state.current_content, state.current_content.len());
        eprintln!("[DEBUG][FINALIZE] This indicates either:");
        eprintln!("[DEBUG][FINALIZE] 1. No content was received from the API");
        eprintln!("[DEBUG][FINALIZE] 2. Content was received but not properly streamed to Discord");
        eprintln!("[DEBUG][FINALIZE] 3. The update_chat_message function failed to populate current_content");
        return Err("Cannot finalize message with 0 characters - this indicates no content was received from the API or streaming failed".into());
    }
    
    // Add any remaining content if provided
    if !remaining_content.trim().is_empty() {
        update_chat_message(state, remaining_content, ctx, config).await?;
    }
    
    // Check if we have any content to finalize (either from remaining_content or existing state)
    if state.current_content.trim().is_empty() {
        println!("[DEBUG][FINALIZE] No content to finalize");
        return Ok(());
    }
    
    // Mark the final message as complete
    let final_display = if state.message_index == 1 {
        format!("**AI Response Complete**\n```\n{}\n```", state.current_content)
    } else {
        format!("**AI Response Complete (Part {}/{})**\n```\n{}\n```", 
            state.message_index, state.message_index, state.current_content)
    };

    println!("[DEBUG][FINALIZE] Marking message as complete - Part {}", state.message_index);
    let edit_result = state.current_message.edit(&ctx.http, |m| {
        m.content(final_display)
    }).await;
    if let Err(e) = edit_result {
        eprintln!("[ERROR][FINALIZE] Failed to finalize Discord message part {}: {}", state.message_index, e);
    }

    Ok(())
} 

// Helper function to load system prompt from file using multi-path fallback
// Loads system_prompt.txt from multiple locations, returns prompt string or error
async fn load_system_prompt() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let prompt_paths = [
        "system_prompt.txt",
        "../system_prompt.txt",
        "../../system_prompt.txt",
        "src/system_prompt.txt",
    ];
    
    for path in &prompt_paths {
        match fs::read_to_string(path) {
            Ok(content) => {
                // Remove BOM if present
                let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
                println!("LM command: Loaded system prompt from {}", path);
                return Ok(content.trim().to_string());
            }
            Err(_) => continue,
        }
    }
    
    Err("system_prompt.txt file not found in any expected location (., .., ../.., src/)".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_chat_message_structure() {
        // Test that ChatMessage can be created properly
        let system_message = ChatMessage {
            role: "system".to_string(),
            content: "You are a helpful AI assistant.".to_string(),
        };
        
        let user_message = ChatMessage {
            role: "user".to_string(),
            content: "What is machine learning?".to_string(),
        };
        
        assert_eq!(system_message.role, "system");
        assert_eq!(user_message.role, "user");
    }
    
    #[tokio::test]
    async fn debug_prompt_loading() {
        println!("=== DEBUG: Testing prompt loading ===");
        
        // Test system prompt loading
        match load_system_prompt().await {
            Ok(prompt) => {
                println!("System prompt loaded successfully:");
                println!("Length: {} characters", prompt.len());
                println!("Content preview: {}", &prompt[..prompt.len().min(200)]);
            }
            Err(e) => {
                println!("Failed to load system prompt: {}", e);
            }
        }
    }
    
    #[test]
    fn test_mention_patterns() {
        use regex::Regex;
        
        // Test Discord mention pattern
        let mention_pattern = Regex::new(r"<@!?(\d+)>").unwrap();
        let test_cases = vec![
            "<@123456789>",
            "<@!123456789>",
            "@123456789",
            "Hello @123456789 there",
            "<@342476479017254913>", // The specific user ID from the user's example
        ];
        
        for case in test_cases {
            if case.starts_with('<') {
                if let Some(caps) = mention_pattern.captures(case) {
                    println!("✅ Discord mention pattern matched: '{}' -> ID: {}", case, &caps[1]);
                } else {
                    println!("❌ Discord mention pattern failed: '{}'", case);
                }
            } else {
                // Test @ pattern
                let at_pattern = Regex::new(r"@(\d+)(?:\b|$)").unwrap();
                if let Some(caps) = at_pattern.captures(case) {
                    println!("✅ @ pattern matched: '{}' -> ID: {}", case, &caps[1]);
                } else {
                    println!("❌ @ pattern failed: '{}'", case);
                }
            }
        }
    }
    
    #[test]
    fn test_mention_replacement_logic() {
        // Test the replacement logic that should be used in resolve_user_mentions
        let test_text = "Hello <@123456789012345678> how are you?";
let user_id = "123456789012345678";
        let username = "TestUser";
        
        // Test Discord mention replacement
        let mut result = test_text.to_string();
        let mention_patterns = vec![
            format!("<@{}>", user_id),
            format!("<@!{}>", user_id),
        ];
        
        for pattern in mention_patterns {
            if result.contains(&pattern) {
                println!("✅ Found pattern '{}' in text", pattern);
                result = result.replace(&pattern, username);
                println!("✅ Replaced with '{}'", username);
            } else {
                println!("❌ Pattern '{}' not found in text", pattern);
            }
        }
        
        println!("Final result: '{}'", result);
        assert_eq!(result, "Hello TestUser how are you?");
    }
}