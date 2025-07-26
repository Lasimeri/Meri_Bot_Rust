// vis.rs - Vision Analysis and Image/GIF Processing Module
// This module implements vision analysis for the bot, supporting image and GIF attachments for AI models.
// It handles image downloading, GIF frame extraction, base64 encoding, and streaming vision responses.
//
// Key Features:
// - Processes image and GIF attachments for vision models
// - Converts GIFs to PNG (first frame) for compatibility
// - Encodes images as base64 data URIs for multimodal AI
// - Streams vision model responses to Discord
// - Handles errors and provides user feedback
//
// Used by: lm.rs (vision command), main.rs (user ID mention vision)

use serenity::{client::Context, model::channel::Message};
use crate::commands::lm::{MultimodalChatMessage, MessageContent, ImageUrl, StreamingStats, MessageState, update_chat_message, finalize_chat_message};
use crate::commands::search::LMConfig;
use reqwest;
use std::path::Path;
use std::io::{Write, Cursor};
use base64::{Engine as _, engine::general_purpose};
use uuid::Uuid;
use futures_util::StreamExt;

use image::{ImageFormat, ImageError};

/// Enhanced image processing with GIF support
/// Downloads image attachment, processes GIFs (extracts first frame), and encodes as base64
/// Returns (base64_image, content_type) tuple for multimodal AI
pub async fn process_image_attachment(attachment: &serenity::model::channel::Attachment) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    let temp_file = format!("temp_image_{}", Uuid::new_v4());
    let temp_path = Path::new(&temp_file);
    
    println!("[GIF_VISION] Processing attachment: {} ({})", attachment.filename, attachment.content_type.as_deref().unwrap_or("unknown"));
    
    let response = reqwest::get(&attachment.url).await?;
    let bytes = response.bytes().await?;
    
    println!("[GIF_VISION] Downloaded {} bytes", bytes.len());
    
    // Write to temporary file for processing
    let mut file = std::fs::File::create(temp_path)?;
    file.write_all(&bytes)?;
    drop(file); // Close the file
    
    let content_type = attachment.content_type.clone().unwrap_or_else(|| {
        mime_guess::from_path(&attachment.filename).first_or(mime_guess::mime::IMAGE_JPEG).to_string()
    });
    
    // Check if this is a GIF file
    let is_gif = content_type == "image/gif" || 
                 attachment.filename.to_lowercase().ends_with(".gif");
    
    let (processed_bytes, final_content_type) = if is_gif {
        println!("[GIF_VISION] Detected GIF file, processing for vision compatibility...");
        match process_gif_file(temp_path).await {
            Ok((gif_bytes, gif_content_type)) => {
                println!("[GIF_VISION] Successfully processed GIF - converted to {}", gif_content_type);
                (gif_bytes, gif_content_type)
            }
            Err(e) => {
                println!("[GIF_VISION] Failed to process GIF, falling back to raw bytes: {}", e);
                // Fallback to original bytes if GIF processing fails
                let image_bytes = std::fs::read(temp_path)?;
                (image_bytes, content_type)
            }
        }
    } else {
        // For non-GIF images, use original processing
        let image_bytes = std::fs::read(temp_path)?;
        (image_bytes, content_type)
    };
    
    let base64_image = general_purpose::STANDARD.encode(&processed_bytes);
    
    // Clean up temp file
    std::fs::remove_file(temp_path)?;
    
    println!("[GIF_VISION] Final processing complete - {} bytes encoded to base64", processed_bytes.len());
    
    Ok((base64_image, final_content_type))
}

/// Process GIF files for vision model compatibility
/// Extracts first frame from animated GIFs and converts to PNG (base64)
async fn process_gif_file(file_path: &Path) -> Result<(Vec<u8>, String), Box<dyn std::error::Error + Send + Sync>> {
    println!("[GIF_VISION] Loading GIF file for processing...");
    
    // Read the file
    let gif_bytes = std::fs::read(file_path)?;
    
    // Try to load as image using the image crate
    match image::load_from_memory_with_format(&gif_bytes, ImageFormat::Gif) {
        Ok(img) => {
            println!("[GIF_VISION] Successfully loaded GIF image");
            
            // Convert to PNG for better vision model compatibility
            let mut png_bytes = Vec::new();
            {
                let mut cursor = Cursor::new(&mut png_bytes);
                img.write_to(&mut cursor, ImageFormat::Png)?;
            }
            
            println!("[GIF_VISION] Converted GIF to PNG format ({} bytes)", png_bytes.len());
            Ok((png_bytes, "image/png".to_string()))
        }
        Err(ImageError::Unsupported(_)) => {
            println!("[GIF_VISION] Image crate doesn't support this GIF format, trying alternative approach...");
            
            // Try to decode using gif-specific handling
            match decode_gif_first_frame(&gif_bytes).await {
                Ok((frame_bytes, content_type)) => {
                    println!("[GIF_VISION] Successfully extracted first frame from animated GIF");
                    Ok((frame_bytes, content_type))
                }
                Err(e) => {
                    println!("[GIF_VISION] Failed to decode GIF frames: {}", e);
                    // Last resort: return original bytes
                    Ok((gif_bytes, "image/gif".to_string()))
                }
            }
        }
        Err(e) => {
            println!("[GIF_VISION] Failed to load GIF with image crate: {}", e);
            Err(format!("GIF processing failed: {}", e).into())
        }
    }
}

/// Decode the first frame from an animated GIF (fallback method)
/// Returns PNG bytes and content type
async fn decode_gif_first_frame(gif_bytes: &[u8]) -> Result<(Vec<u8>, String), Box<dyn std::error::Error + Send + Sync>> {
    println!("[GIF_VISION] Attempting to decode first frame from animated GIF...");
    
    // Create a temporary file for gif processing
    let temp_gif = format!("temp_gif_{}.gif", Uuid::new_v4());
    std::fs::write(&temp_gif, gif_bytes)?;
    
    // Try to open and process the GIF
    let result = match image::open(&temp_gif) {
        Ok(img) => {
            println!("[GIF_VISION] Opened GIF successfully, extracting frame...");
            
            // Convert to PNG
            let mut png_bytes = Vec::new();
            {
                let mut cursor = Cursor::new(&mut png_bytes);
                img.write_to(&mut cursor, ImageFormat::Png)?;
            }
            
            println!("[GIF_VISION] Extracted and converted frame to PNG ({} bytes)", png_bytes.len());
            Ok((png_bytes, "image/png".to_string()))
        }
        Err(e) => {
            println!("[GIF_VISION] Failed to open GIF file: {}", e);
            Err(format!("Could not decode GIF: {}", e).into())
        }
    };
    
    // Clean up temp file
    let _ = std::fs::remove_file(&temp_gif);
    
    result
}

/// Prepare multimodal message with image (for vision model)
/// Builds a multimodal message with text prompt and base64-encoded image
pub fn create_vision_message(prompt: &str, base64_image: &str, content_type: &str) -> Vec<MultimodalChatMessage> {
    println!("[GIF_VISION] Creating vision message with content type: {}", content_type);
    
    vec![
        MultimodalChatMessage {
            role: "system".to_string(),
            content: vec![MessageContent::Text {
                content_type: "text".to_string(),
                text: "You are a vision-capable AI assistant. You can analyze images including static images and frames from animated GIFs.".to_string(),
            }],
        },
        MultimodalChatMessage {
            role: "user".to_string(),
            content: vec![
                MessageContent::Text {
                    content_type: "text".to_string(),
                    text: prompt.to_string(),
                },
                MessageContent::Image {
                    content_type: "image_url".to_string(),
                    image_url: ImageUrl {
                        url: format!("data:{};base64,{}", content_type, base64_image),
                    },
                },
            ],
        },
    ]
}

/// Stream vision response (adapted from stream_chat_response)
/// Streams the AI's vision response, chunking and updating Discord messages as needed
pub async fn stream_vision_response(
    messages: Vec<MultimodalChatMessage>,
    config: &LMConfig,
    ctx: &Context,
    initial_msg: &mut Message,
) -> Result<StreamingStats, Box<dyn std::error::Error + Send + Sync>> {
    println!("[VISION_STREAM] Starting vision response streaming");
    println!("[VISION_STREAM] Model to use: {}", config.default_vision_model);
    println!("[VISION_STREAM] API URL: {}/v1/chat/completions", config.base_url);
    
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(60)).build()?;
    
    let chat_request = crate::commands::lm::ChatRequest {
        model: config.default_vision_model.clone(),
        messages,
        temperature: config.default_temperature,
        max_tokens: config.default_max_tokens,
        stream: true,
        seed: config.default_seed,
    };
    
    println!("[VISION_STREAM] ChatRequest created:");
    println!("[VISION_STREAM]   - Model: {}", chat_request.model);
    println!("[VISION_STREAM]   - Temperature: {}", chat_request.temperature);
    println!("[VISION_STREAM]   - Max Tokens: {}", chat_request.max_tokens);
    println!("[VISION_STREAM]   - Stream: {}", chat_request.stream);
    println!("[VISION_STREAM]   - Message count: {}", chat_request.messages.len());
    
    let api_url = format!("{}/v1/chat/completions", config.base_url);
    println!("[VISION_STREAM] Making POST request to: {}", api_url);
    
    let response = client.post(&api_url).json(&chat_request).send().await?;
    
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
        println!("[VISION_STREAM] API error: {} - {}", status, error_text);
        
        // Provide specific error messages for common issues
        if status == 404 {
            if error_text.contains("model") && error_text.contains("not found") {
                return Err(format!("Vision model '{}' is not loaded in LM Studio.\n\n**To fix this:**\n1. Open LM Studio\n2. Go to 'My Models' tab\n3. Find '{}' and click 'Load'\n4. Wait for it to fully load\n5. Try the vision command again", config.default_vision_model, config.default_vision_model).into());
            } else if error_text.contains("No models loaded") {
                return Err(format!("No models are loaded in LM Studio.\n\n**To fix this:**\n1. Open LM Studio\n2. Go to 'My Models' tab\n3. Find and load the vision model: '{}'\n4. Wait for it to fully load\n5. Try the vision command again", config.default_vision_model).into());
            }
        }
        
        return Err(format!("Vision API error: {} - {}", status, error_text).into());
    }
    
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
    let update_interval = std::time::Duration::from_millis(800);
    let mut line_buffer = String::new();

    println!("[VISION_STREAM] Starting to process response stream...");

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                line_buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(i) = line_buffer.find('\n') {
                    let line = line_buffer.drain(..=i).collect::<String>();
                    let line = line.trim();

                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str.trim() == "[DONE]" {
                            if !content_buffer.is_empty() {
                                let _ = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await;
                            }
                            return Ok(StreamingStats { total_characters: raw_response.len(), message_count: message_state.message_index });
                        }

                        if let Ok(response_chunk) = serde_json::from_str::<crate::commands::lm::ChatResponse>(json_str) {
                            for choice in response_chunk.choices {
                                                                 if let Some(finish_reason) = choice.finish_reason {
                                     if finish_reason == "stop" {
                                         if !content_buffer.is_empty() {
                                             let _ = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await;
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
                        }
                    }
                }
            }
                         Err(e) => {
                 eprintln!("[VISION_STREAM] Stream error: {}", e);
                 if !content_buffer.is_empty() {
                     let _ = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await;
                 }
                 return Err(e.into());
             }
        }
    }

         // Final cleanup
     if !content_buffer.is_empty() {
         let _ = finalize_chat_message(&mut message_state, &content_buffer, ctx, config).await;
     }

    Ok(StreamingStats { total_characters: raw_response.len(), message_count: message_state.message_index })
} 

/// Main entry point for vision analysis requests
/// Handles downloading, processing, and streaming vision model responses for image/GIF attachments
pub async fn handle_vision_request(
    ctx: &Context,
    msg: &Message,
    prompt: &str,
    attachment: &serenity::model::channel::Attachment,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("[VISION] Starting vision request handling");
    println!("[VISION] Prompt: '{}'", prompt);
    println!("[VISION] Attachment: {} ({})", attachment.filename, attachment.content_type.as_deref().unwrap_or("unknown"));
    
    // Check if this is a GIF file for specialized user feedback
    let content_type = attachment.content_type.as_deref().unwrap_or("");
    let is_gif = content_type == "image/gif" || 
                 attachment.filename.to_lowercase().ends_with(".gif");
    
    // Create initial message with appropriate content for GIF vs regular image
    let initial_content = if is_gif {
        "**GIF Vision Analysis (Part 1):**\n```\nProcessing GIF file (extracting frame for analysis)...\n\n```"
    } else {
        "**Vision Analysis (Part 1):**\n```\n\n```"
    };
    
    let mut initial_msg = msg.channel_id.send_message(&ctx.http, |m| {
        m.content(initial_content)
    }).await?;
    
    let (base64_image, processed_content_type) = process_image_attachment(attachment).await?;
    println!("[VISION] Image processed - base64 length: {}, content_type: {}", base64_image.len(), processed_content_type);
    
    // Update message if GIF was converted
    if is_gif && processed_content_type != "image/gif" {
        let _ = initial_msg.edit(&ctx.http, |m| {
            m.content("**GIF Vision Analysis (Part 1):**\n```\nGIF converted to PNG for analysis...\n\n```")
        }).await;
    }
    
    let messages = create_vision_message(prompt, &base64_image, &processed_content_type);
    println!("[VISION] Created {} multimodal messages", messages.len());
    
    println!("[VISION] Loading LM config from lmapiconf.txt...");
    let config = crate::commands::search::load_lm_config().await?;
    println!("[VISION] Config loaded successfully:");
    println!("[VISION]   - Base URL: {}", config.base_url);
    println!("[VISION]   - Default Model: {}", config.default_model);
    println!("[VISION]   - Default Reason Model: {}", config.default_reason_model);
    println!("[VISION]   - Default Ranking Model: {}", config.default_ranking_model);
    println!("[VISION]   - Default Vision Model: {}", config.default_vision_model);
    println!("[VISION]   - Temperature: {}", config.default_temperature);
    println!("[VISION]   - Max Tokens: {}", config.default_max_tokens);
    
    println!("[VISION] About to call stream_vision_response with model: {}", config.default_vision_model);
    stream_vision_response(messages, &config, ctx, &mut initial_msg).await?;
    println!("[VISION] Vision request completed successfully");
    Ok(())
} 