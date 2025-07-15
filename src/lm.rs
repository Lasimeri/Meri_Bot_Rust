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
use crate::search::{
    load_lm_config, perform_ai_enhanced_search, LMConfig, ChatMessage
};
use crate::LmContextMap; // TypeMap key defined in main.rs
use mime_guess::MimeGuess;
use std::io::Write;
use uuid::Uuid;
use serenity::model::id::UserId;

// Structure to track streaming statistics
#[derive(Debug)]
pub struct StreamingStats {
    pub total_characters: usize,
    pub message_count: usize,
}

// Structure to track current message state during streaming
pub struct MessageState {
    pub current_content: String,
    pub current_message: Message,
    pub message_index: usize,
    pub char_limit: usize,
}

// Enhanced ChatMessage structure for multimodal content
#[derive(Serialize, Deserialize, Clone)]
pub struct MultimodalChatMessage {
    pub role: String,
    pub content: Vec<MessageContent>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum MessageContent {
    Text { #[serde(rename = "type")] content_type: String, text: String },
    Image { #[serde(rename = "type")] content_type: String, image_url: ImageUrl },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ImageUrl {
    pub url: String,
}

// Document processing result
#[derive(Debug)]
struct ProcessedDocument {
    pub filename: String,
    pub content: String,
    pub content_type: String,
    pub size: usize,
}

// API Request/Response structures for streaming
#[derive(Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<MultimodalChatMessage>,
    pub temperature: f32,
    pub max_tokens: i32,
    pub stream: bool,
}

#[derive(Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize)]
pub struct Choice {
    pub delta: Option<Delta>,
    pub finish_reason: Option<String>,
}

#[derive(Deserialize)]
pub struct Delta {
    pub content: Option<String>,
}

#[command]
#[aliases("llm", "ai", "chat")]
pub async fn lm(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let mut input = args.message().trim().to_string();
    
    // Start typing indicator
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    
    // IMPORTANT: Check for vision flag BEFORE processing reply logic
    // This ensures we detect -v flag even in replies
    let is_vision_request = input.starts_with("-v") || input.starts_with("--vision");
    let original_input = input.clone(); // Store original input for vision processing
    
    // Check if this is a reply and handle it appropriately
    if let Some(referenced_message) = &msg.referenced_message {
        println!("LM command: Detected reply to message from {}", referenced_message.author.name);
        
        // Only modify input for non-vision requests
        if !is_vision_request {
            // If the reply has no content, use the referenced message content as the prompt
            if input.is_empty() {
                input = referenced_message.content.clone();
                println!("LM command: Using referenced message content as prompt: {}", input);
            } else {
                // If the reply has content, combine it with the referenced message
                input = format!("Original message: {}\n\nYour response: {}", referenced_message.content, input);
                println!("LM command: Combined referenced message with reply content");
            }
        } else {
            println!("LM command: Vision request detected in reply - keeping original input: '{}'", original_input);
        }
    }
    
    if input.is_empty() {
        msg.reply(ctx, "Please provide a prompt! Usage: `^lm <your prompt>` or `^lm -s <search query>`\n\nYou can also reply to a message with `^lm` to respond to that message.").await?;
        return Ok(());
    }

    // Skip context logic for search and test flags
    if input.starts_with("-s ") || input.starts_with("--search ") {
        // Extract search query
        let search_query = if input.starts_with("-s ") {
            &input[3..]
        } else {
            &input[9..] // "--search "
        };

        if search_query.trim().is_empty() {
            msg.reply(ctx, "Please provide a search query! Usage: `^lm -s <search query>`").await?;
            return Ok(());
        }

        // Load LM Studio configuration for AI-enhanced search
        let config = match load_lm_config().await {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Failed to load LM Studio configuration for search: {}", e);
                msg.reply(ctx, &format!("LM Studio configuration error: {}\n\nMake sure `lmapiconf.txt` exists and contains all required settings. Check `example_lmapiconf.txt` for reference.", e)).await?;
                return Ok(());
            }
        };

        // Send initial search message
        let mut search_msg = match msg.channel_id.send_message(&ctx.http, |m| {
            m.content("Refining search query...")
        }).await {
            Ok(message) => message,
            Err(e) => {
                eprintln!("Failed to send initial search message: {}", e);
                msg.reply(ctx, "Failed to send message!").await?;
                return Ok(());
            }
        };

        // AI-Enhanced Search Flow
        match perform_ai_enhanced_search(search_query, &config, &mut search_msg, ctx).await {
            Ok(()) => {
                println!("AI-enhanced search completed successfully for query: '{}'", search_query);
            }
            Err(e) => {
                eprintln!("AI-enhanced search failed: {}", e);
                let error_msg = format!("**Search Failed**\n\nQuery: `{}`\nError: {}\n\nCheck your SerpAPI configuration in lmapiconf.txt", search_query, e);
                let _ = search_msg.edit(&ctx.http, |m| {
                    m.content(&error_msg)
                }).await;
            }
        }

        return Ok(());
    }

    if input.starts_with("--test") || input == "-t" {
        // Load LM Studio configuration for connectivity test
        let config = match load_lm_config().await {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Failed to load LM Studio configuration for test: {}", e);
                msg.reply(ctx, &format!("LM Studio configuration error: {}\n\nMake sure `lmapiconf.txt` exists and contains all required settings. Check `example_lmapiconf.txt` for reference.", e)).await?;
                return Ok(());
            }
        };

        // Send initial test message
        let mut test_msg = match msg.channel_id.send_message(&ctx.http, |m| {
            m.content("Testing API connectivity to remote server...")
        }).await {
            Ok(message) => message,
            Err(e) => {
                eprintln!("Failed to send initial test message: {}", e);
                msg.reply(ctx, "Failed to send message!").await?;
                return Ok(());
            }
        };

        // Perform connectivity test
        match crate::search::test_api_connectivity(&config).await {
            Ok(success_message) => {
                let final_message = format!("**Connectivity Test Results**\n\n{}\n\n**Configuration:**\n• Server: `{}`\n• Default Model: `{}`\n• Reasoning Model: `{}`\n• Timeout: `{}s`", 
                    success_message, config.base_url, config.default_model, config.default_reason_model, config.timeout);
                
                if let Err(e) = test_msg.edit(&ctx.http, |m| {
                    m.content(&final_message)
                }).await {
                    eprintln!("Failed to update test message: {}", e);
                }
            }
            Err(e) => {
                let error_message = format!("**Connectivity Test Failed**\n\n**Error:** {}\n\n**Troubleshooting:**\n• Check if LM Studio/Ollama is running on `{}`\n• Verify the model `{}` is loaded\n• Check firewall settings\n• Ensure the server is accessible from this network\n\n**Configuration:**\n• Server: `{}`\n• Default Model: `{}`\n• Timeout: `{}s`", 
                    e, config.base_url, config.default_model, config.base_url, config.default_model, config.timeout);
                
                if let Err(edit_error) = test_msg.edit(&ctx.http, |m| {
                    m.content(&error_message)
                }).await {
                    eprintln!("Failed to update test message with error: {}", edit_error);
                }
            }
        }

        return Ok(());
    }

    // Check if this is a clear context request
    if input.starts_with("--clear") || input == "-c" {
        let mut data_map = ctx.data.write().await;
        let lm_map = data_map.get_mut::<LmContextMap>().expect("LM context map not initialized");
        
        let had_context = if let Some(context) = lm_map.get_mut(&msg.author.id) {
            let message_count = context.total_messages();
            context.clear();
            message_count > 0
        } else {
            false
        };
        
        if had_context {
            msg.reply(ctx, "**LM Chat Context Cleared** ✅\nYour conversation history has been reset (50 user messages + 50 assistant messages).").await?;
        } else {
            msg.reply(ctx, "**No LM Context Found** ℹ️\nYou don't have any active conversation history to clear.").await?;
        }
        return Ok(());
    }

    // Handle vision flag - use original input to preserve the flag
    if is_vision_request {
        println!("[LM] Vision flag detected in original input: '{}'", original_input);
        
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
        
        println!("[LM] Extracted vision prompt: '{}'", vision_prompt);

        if vision_prompt.is_empty() {
            println!("[LM] Vision prompt is empty, returning error");
            msg.reply(ctx, "Please provide a prompt for vision analysis! Usage: `^lm -v <prompt>` with image attached.").await?;
            return Ok(());
        }

        // Check for attachments in current message first
        let attachment_to_process = if !msg.attachments.is_empty() {
            println!("[LM] Found {} attachments in current message", msg.attachments.len());
            Some(&msg.attachments[0])
        } else if let Some(referenced_msg) = &msg.referenced_message {
            println!("[LM] No local attachments, checking referenced message...");
            if !referenced_msg.attachments.is_empty() {
                println!("[LM] Found {} attachments in referenced message", referenced_msg.attachments.len());
                Some(&referenced_msg.attachments[0])
            } else {
                println!("[LM] No attachments found in referenced message");
                None
            }
        } else {
            println!("[LM] No attachments found and no referenced message");
            None
        };

        let attachment = match attachment_to_process {
            Some(att) => att,
            None => {
                println!("[LM] No image attachments found in current or referenced message");
                msg.reply(ctx, "Please attach an image for vision analysis, or reply to a message with an image attachment.").await?;
                return Ok(());
            }
        };

        let content_type = attachment.content_type.as_deref().unwrap_or("");
        println!("[LM] Found attachment: {} (content_type: {})", attachment.filename, content_type);

        if !content_type.starts_with("image/") {
            println!("[LM] Attachment is not an image, returning error");
            msg.reply(ctx, "Attached file is not an image. Please attach a valid image file.").await?;
            return Ok(());
        }

        println!("[LM] Calling vis::handle_vision_request...");
        if let Err(e) = crate::vis::handle_vision_request(ctx, msg, &vision_prompt, attachment).await {
            println!("[LM] Vision request failed with error: {}", e);
            msg.reply(ctx, format!("Vision analysis error: {}", e)).await?;
        } else {
            println!("[LM] Vision request completed successfully");
        }

        return Ok(());
    }

    // Regular AI chat functionality
    let prompt = input;
    
    // Process attachments for RAG if any
    let mut processed_documents = Vec::new();
    if !msg.attachments.is_empty() {
        println!("[RAG] Found {} attachments, processing for document analysis", msg.attachments.len());
        
        match process_attachments(&msg.attachments, ctx).await {
            Ok(docs) => {
                processed_documents = docs;
                println!("[RAG] Successfully processed {} documents", processed_documents.len());
            }
            Err(e) => {
                eprintln!("[RAG] Failed to process attachments: {}", e);
                msg.reply(ctx, &format!("⚠️ Failed to process some attachments: {}\n\nContinuing with text-only analysis.", e)).await?;
            }
        }
    }
    
    // Create RAG-enhanced prompt if documents were processed
    let final_prompt = if !processed_documents.is_empty() {
        create_rag_prompt(&prompt, &processed_documents)
    } else {
        prompt.clone()
    };

    // Record user prompt in per-user context history (store original prompt, not RAG-enhanced)
    {
        let mut data_map = ctx.data.write().await;
        
        // Scoped for lm_map
        {
            let lm_map = data_map.get_mut::<LmContextMap>().expect("LM context map not initialized");
            let context = lm_map.entry(msg.author.id).or_insert_with(crate::UserContext::new);
            context.add_user_message(ChatMessage { role: "user".to_string(), content: prompt.to_string() });
            
            println!("[LM] User context updated: {} user messages, {} assistant messages", 
                context.user_messages.len(), context.assistant_messages.len());
        }
    }

    // Load LM Studio configuration
    let config = match load_lm_config().await {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load LM Studio configuration: {}", e);
            msg.reply(ctx, &format!("LM Studio configuration error: {}\n\nMake sure `lmapiconf.txt` exists and contains all required settings. Check `example_lmapiconf.txt` for reference.", e)).await?;
            return Ok(());
        }
    };

    // Load system prompt
    let base_system_prompt = match load_system_prompt().await {
        Ok(prompt) => prompt,
        Err(e) => {
            eprintln!("Failed to load system prompt: {}", e);
            msg.reply(ctx, "Failed to load system configuration!").await?;
            return Ok(());
        }
    };

    // Build message list including system prompt and per-user history
    let mut messages = Vec::new();
    messages.push(ChatMessage { role: "system".to_string(), content: base_system_prompt });
    {
        let data_map = ctx.data.read().await;
        if let Some(lm_map) = data_map.get::<LmContextMap>() {
            if let Some(context) = lm_map.get(&msg.author.id) {
                let conversation_messages = context.get_conversation_messages();
                for entry in conversation_messages.iter() {
                    messages.push(entry.clone());
                }
                println!("[LM] Loaded {} context messages for user {}", 
                    conversation_messages.len(), msg.author.name);
            }
        }
    }
    
    // Add the current user message with RAG-enhanced content
    messages.push(ChatMessage { role: "user".to_string(), content: final_prompt });

    // Convert to multimodal format
    let multimodal_messages = convert_to_multimodal(messages);

    // Log which model is being used for LM command
    println!("LM command: Using model '{}' for chat", config.default_model);
    if !processed_documents.is_empty() {
        println!("[RAG] Using document-enhanced prompt with {} documents", processed_documents.len());
    }

    // Stream the response with real-time Discord post editing
    let mut current_msg = msg.channel_id.send_message(&ctx.http, |m| {
        let content = if !processed_documents.is_empty() {
            format!("**AI Response (Document Analysis - Part 1):**\n```\n\n```")
        } else {
            "**AI Response (Part 1):**\n```\n\n```".to_string()
        };
        m.content(content)
    }).await?;

    // Ensure current_msg is in scope for this match
    match stream_chat_response(multimodal_messages, &config.default_model, &config, ctx, &mut current_msg).await {
        Ok(final_stats) => {
            println!("LM command: Streaming complete - {} total characters across {} messages", 
                final_stats.total_characters, final_stats.message_count);
            
            // Record AI response in per-user context history
            let response_content = current_msg.content.clone();
            {
                let mut data_map = ctx.data.write().await;
                
                // Scoped for lm_map
                {
                    let lm_map = data_map.get_mut::<LmContextMap>().expect("LM context map not initialized");
                    if let Some(context) = lm_map.get_mut(&msg.author.id) {
                        context.add_assistant_message(ChatMessage { 
                            role: "assistant".to_string(), 
                            content: response_content.clone()
                        });
                        
                        println!("[LM] AI response recorded: {} total messages in context", 
                            context.total_messages());
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to stream response from AI model: {}", e);
            let _ = current_msg.edit(&ctx.http, |m| {
                m.content("Failed to get response from AI model!")
            }).await;
        }
    }

    Ok(())
}

#[command]
#[aliases("clearlm", "resetlm")]
pub async fn clearcontext(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    // Clear the user's LM chat context robustly
    let mut data_map = ctx.data.write().await;
    let lm_map = data_map.get_mut::<LmContextMap>().expect("LM context map not initialized");
    
    let user_id = msg.author.id;
    let had_context = if let Some(context) = lm_map.get_mut(&user_id) {
        let message_count = context.total_messages();
        context.clear();
        message_count > 0
    } else {
        false
    };
    
    println!("[clearcontext] Cleared context for user {} (had_context={})", user_id, had_context);
    
    if had_context {
        msg.reply(ctx, "**LM Chat Context Cleared** ✅\nYour conversation history with the AI has been fully reset (50 user messages + 50 assistant messages). The next message you send will start a brand new context.").await?;
    } else {
        msg.reply(ctx, "**No Context Found** ℹ️\nYou don't have any active conversation history to clear. Start a conversation with `^lm <your message>`.\n\nIf you believe context is still being used, please report this as a bug.").await?;
    }
    
    Ok(())
}

// Process Discord attachments for RAG
async fn process_attachments(
    attachments: &[Attachment],
    ctx: &Context,
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

// Check if a file format is supported for processing
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

// Extract content from different document types
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

// Extract text content from PDF files
async fn extract_pdf_content(file_path: &Path) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use pdf_extract::extract_text;
    
    let content = extract_text(file_path)?;
    Ok(content)
}

// Convert regular ChatMessage to MultimodalChatMessage
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
pub async fn stream_chat_response(
    messages: Vec<MultimodalChatMessage>,
    model: &str,
    config: &LMConfig,
    ctx: &Context,
    initial_msg: &mut Message,
) -> Result<StreamingStats, Box<dyn std::error::Error + Send + Sync>> {
    println!("[STREAMING] Attempting connection to API server: {}", config.base_url);
    println!("[STREAMING] Using model: {}", model);
    println!("[STREAMING] Connect timeout: {} seconds", config.timeout);
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
        
    let chat_request = ChatRequest {
        model: model.to_string(),
        messages,
        temperature: config.default_temperature,
        max_tokens: config.default_max_tokens,
        stream: true,
    };

    let api_url = format!("{}/v1/chat/completions", config.base_url);
    println!("[STREAMING] Full API URL: {}", api_url);
    println!("[STREAMING] Request payload: model={}, max_tokens={}, temperature={}, stream=true", 
        chat_request.model, chat_request.max_tokens, chat_request.temperature);

    // First, test basic connectivity to the server with enhanced error handling
    println!("[STREAMING] Testing basic connectivity to {}...", config.base_url);
    match client.get(&config.base_url).send().await {
        Ok(response) => {
            println!("[STREAMING] Basic connectivity test successful - Status: {}", response.status());
        }
        Err(e) => {
            println!("[STREAMING] Basic connectivity test failed: {}", e);
            
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
    println!("[STREAMING] Making streaming API request to chat completions endpoint...");
    let response = match client
        .post(&api_url)
        .json(&chat_request)
        .send()
        .await
    {
        Ok(resp) => {
            println!("[STREAMING] API request sent successfully - Status: {}", resp.status());
            resp
        }
        Err(e) => {
            println!("[STREAMING] API request failed: {}", e);
            
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
        println!("[STREAMING] API returned error status {}: {}", status, error_text);
        return Err(format!("Streaming API request failed: HTTP {} - {}", status, error_text).into());
    }

    println!("[STREAMING] Starting to process response stream...");
    let mut stream = response.bytes_stream();
    let mut message_state = MessageState {
        current_content: String::new(),
        current_message: initial_msg.clone(),
        message_index: 1,
        char_limit: config.max_discord_message_length - config.response_format_padding,
    };
    
    let mut raw_response = String::new();
    let mut content_buffer = String::new();
    let mut last_update = std::time::Instant::now();
    let update_interval = std::time::Duration::from_millis(800); // Update every 0.8 seconds
    let mut chunk_count = 0;
    let mut line_buffer = String::new();

    println!("Starting real-time streaming for chat response...");

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                chunk_count += 1;
                if chunk_count == 1 {
                    println!("[STREAMING] Received first chunk ({} bytes)", bytes.len());
                } else if chunk_count % 10 == 0 {
                    println!("[STREAMING] Processed {} chunks, total response: {} chars", chunk_count, raw_response.len());
                }
                
                line_buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(i) = line_buffer.find('\n') {
                    let line = line_buffer.drain(..=i).collect::<String>();
                    let line = line.trim();

                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str.trim() == "[DONE]" {
                            println!("[STREAMING] Received [DONE] signal, finalizing response");
                            if !content_buffer.is_empty() {
                                if let Err(e) = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await {
                                    eprintln!("Failed to finalize message: {}", e);
                                }
                            }
                            return Ok(StreamingStats { total_characters: raw_response.len(), message_count: message_state.message_index });
                        }

                        if let Ok(response_chunk) = serde_json::from_str::<ChatResponse>(json_str) {
                            for choice in response_chunk.choices {
                                if let Some(finish_reason) = choice.finish_reason {
                                    if finish_reason == "stop" {
                                        println!("[STREAMING] Received finish_reason=stop, finalizing response");
                                        if !content_buffer.is_empty() {
                                            if let Err(e) = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await {
                                                eprintln!("Failed to finalize message: {}", e);
                                            }
                                        }
                                        return Ok(StreamingStats { total_characters: raw_response.len(), message_count: message_state.message_index });
                                    }
                                }

                                if let Some(delta) = choice.delta {
                                    if let Some(content) = delta.content {
                                        raw_response.push_str(&content);
                                        content_buffer.push_str(&content);

                                        if last_update.elapsed() >= update_interval && !content_buffer.is_empty() {
                                            if let Err(e) = update_chat_message(&mut message_state, &content_buffer, ctx, config).await {
                                                eprintln!("Failed to update Discord message: {}", e);
                                                return Err(e);
                                            } else {
                                                content_buffer.clear();
                                            }
                                            last_update = std::time::Instant::now();
                                        }
                                    }
                                }
                            }
                        } else {
                            if !json_str.trim().is_empty() {
                                println!("[STREAMING] Failed to parse JSON chunk: {}", json_str);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[STREAMING] Stream error: {}", e);
                if !content_buffer.is_empty() {
                    let _ = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await;
                }
                return Err(e.into());
            }
        }
    }

    println!("[STREAMING] Stream ended, processed {} chunks total", chunk_count);
    
    // Final cleanup - process any remaining content
    if !content_buffer.is_empty() {
        if let Err(e) = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await {
            eprintln!("Failed to finalize remaining content: {}", e);
        }
    }

    let stats = StreamingStats {
        total_characters: raw_response.len(),
        message_count: message_state.message_index,
    };

    Ok(stats)
}

// Helper function to update Discord message with new content for chat
#[allow(unused_variables)]
pub async fn update_chat_message(
    state: &mut MessageState,
    new_content: &str,
    ctx: &Context,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let potential_content = if state.current_content.is_empty() {
        format!("**AI Response (Part {}):**\n```\n{}\n```", 
            state.message_index, new_content)
    } else {
        format!("**AI Response (Part {}):**\n```\n{}{}\n```", 
            state.message_index, state.current_content, new_content)
    };

    // Check if we need to create a new message
    if potential_content.len() > state.char_limit {
        // Finalize current message
        let final_content = format!("**AI Response (Part {}):**\n```\n{}\n```", 
            state.message_index, state.current_content);
        
        state.current_message.edit(&ctx.http, |m| {
            m.content(final_content)
        }).await?;

        // Create new message
        state.message_index += 1;
        let new_msg_content = format!("**AI Response (Part {}):**\n```\n{}\n```", 
            state.message_index, new_content);
        
        let new_message = state.current_message.channel_id.send_message(&ctx.http, |m| {
            m.content(new_msg_content)
        }).await?;

        state.current_message = new_message;
        state.current_content = new_content.to_string();
    } else {
        // Update current message
        state.current_content.push_str(new_content);
        state.current_message.edit(&ctx.http, |m| {
            m.content(&potential_content)
        }).await?;
    }

    Ok(())
}

// Helper function to finalize message content at the end of streaming for chat
#[allow(unused_variables)]
pub async fn finalize_chat_message(
    state: &mut MessageState,
    remaining_content: &str,
    ctx: &Context,
    config: &LMConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if remaining_content.trim().is_empty() {
        return Ok(());
    }

    // Add any remaining content and finalize
    update_chat_message(state, remaining_content, ctx, config).await?;
    
    // Mark the final message as complete
    let final_display = if state.message_index == 1 {
        format!("**AI Response Complete**\n```\n{}\n```", state.current_content)
    } else {
        format!("**AI Response Complete (Part {}/{})**\n```\n{}\n```", 
            state.message_index, state.message_index, state.current_content)
    };

    state.current_message.edit(&ctx.http, |m| {
        m.content(final_display)
    }).await?;

    Ok(())
} 

// Helper function to load system prompt from file using multi-path fallback
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
}

pub async fn handle_lm_request(
    ctx: &Context,
    msg: &Message,
    input: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("[HANDLE_LM] Processing input: '{}'", input);
    
    // Check if this is a vision request
    if input.starts_with("-v") || input.starts_with("--vision") {
        println!("[HANDLE_LM] Detected vision request, delegating to vision handling");
        
        let vision_prompt = if input.starts_with("-v") {
            let after_flag = if input.starts_with("-v ") {
                &input[3..] // "-v "
            } else {
                &input[2..] // "-v"
            };
            after_flag.trim().to_string()
        } else {
            let after_flag = if input.starts_with("--vision ") {
                &input[9..] // "--vision "
            } else {
                &input[8..] // "--vision"
            };
            after_flag.trim().to_string()
        };
        
        println!("[HANDLE_LM] Extracted vision prompt: '{}'", vision_prompt);
        
        if vision_prompt.is_empty() {
            println!("[HANDLE_LM] Vision prompt is empty, returning error");
            msg.reply(ctx, "Please provide a prompt for vision analysis! Usage: `^lm -v <prompt>` with image attached.").await?;
            return Ok(());
        }

        // Enhanced attachment detection with more debugging
        println!("[HANDLE_LM] Checking for attachments...");
        println!("[HANDLE_LM] Current message attachments: {}", msg.attachments.len());
        
        let attachment_to_process = if !msg.attachments.is_empty() {
            println!("[HANDLE_LM] Found {} attachments in current message", msg.attachments.len());
            for (i, att) in msg.attachments.iter().enumerate() {
                println!("[HANDLE_LM] Attachment {}: {} ({})", i, att.filename, att.content_type.as_deref().unwrap_or("unknown"));
            }
            Some(&msg.attachments[0])
        } else {
            println!("[HANDLE_LM] No attachments in current message");
            if let Some(referenced_msg) = &msg.referenced_message {
                println!("[HANDLE_LM] Checking referenced message from user: {}", referenced_msg.author.name);
                println!("[HANDLE_LM] Referenced message attachments: {}", referenced_msg.attachments.len());
                
                if !referenced_msg.attachments.is_empty() {
                    println!("[HANDLE_LM] Found {} attachments in referenced message", referenced_msg.attachments.len());
                    for (i, att) in referenced_msg.attachments.iter().enumerate() {
                        println!("[HANDLE_LM] Referenced attachment {}: {} ({})", i, att.filename, att.content_type.as_deref().unwrap_or("unknown"));
                    }
                    Some(&referenced_msg.attachments[0])
                } else {
                    println!("[HANDLE_LM] No attachments found in referenced message");
                    None
                }
            } else {
                println!("[HANDLE_LM] No referenced message found");
                None
            }
        };

        let attachment = match attachment_to_process {
            Some(att) => {
                println!("[HANDLE_LM] Using attachment: {} ({})", att.filename, att.content_type.as_deref().unwrap_or("unknown"));
                att
            },
            None => {
                println!("[HANDLE_LM] No image attachments found in current or referenced message");
                msg.reply(ctx, "Please attach an image for vision analysis, or reply to a message with an image attachment.").await?;
                return Ok(());
            }
        };

        let content_type = attachment.content_type.as_deref().unwrap_or("");
        println!("[HANDLE_LM] Checking content type: '{}'", content_type);
        
        if !content_type.starts_with("image/") {
            println!("[HANDLE_LM] Attachment is not an image, returning error");
            msg.reply(ctx, "Attached file is not an image. Please attach a valid image file.").await?;
            return Ok(());
        }

        println!("[HANDLE_LM] Calling vision handler for attachment: {}", attachment.filename);
        return crate::vis::handle_vision_request(ctx, msg, &vision_prompt, attachment).await;
    }
    
    // Regular LM handling
    println!("[HANDLE_LM] Processing as regular LM request");
    let config = load_lm_config().await?;
    let base_system_prompt = load_system_prompt().await?;
    let mut messages = Vec::new();
    messages.push(ChatMessage { role: "system".to_string(), content: base_system_prompt });
    {
        let data_map = ctx.data.read().await;
        if let Some(lm_map) = data_map.get::<LmContextMap>() {
            if let Some(context) = lm_map.get(&msg.author.id) {
                let conversation_messages = context.get_conversation_messages();
                for entry in conversation_messages.iter() {
                    messages.push(entry.clone());
                }
            }
        }
    }
    messages.push(ChatMessage { role: "user".to_string(), content: input.to_string() });
    let multimodal_messages = convert_to_multimodal(messages);
    let mut initial_msg = msg.channel_id.send_message(&ctx.http, |m| {
        m.content("**AI Response (Part 1):**\n```\n\n```")
    }).await?;
    let _stats = stream_chat_response(multimodal_messages, &config.default_model, &config, ctx, &mut initial_msg).await?;
    // Record response in history (similar to lm)
    let response_content = initial_msg.content.clone();
    {
        let mut data_map = ctx.data.write().await;
        let lm_map = data_map.get_mut::<LmContextMap>().expect("LM context map not initialized");
        if let Some(context) = lm_map.get_mut(&msg.author.id) {
            context.add_assistant_message(ChatMessage { role: "assistant".to_string(), content: response_content });
        }
    }
    Ok(())
}

