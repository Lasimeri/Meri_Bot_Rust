// slash.rs - Slash Commands Module
// This module implements slash commands for the Discord bot, allowing commands to work in private messages
// and providing a better user experience with Discord's native slash command interface.
//
// Key Features:
// - Slash command registration and handling
// - Support for all existing bot commands as slash commands
// - Private message support
// - Better user experience with Discord's native interface
// - Automatic command registration on bot startup

use serenity::{
    client::Context,
    model::application::{
        command::{Command, CommandOptionType},
        interaction::{application_command::ApplicationCommandInteraction, InteractionResponseType},
    },
};
use crate::LmContextMap;
use crate::ReasonContextMap;

// ============================================================================
// SLASH COMMAND HANDLER
// ============================================================================

/// Handle slash command interactions
pub async fn handle_slash_command(ctx: &Context, interaction: &ApplicationCommandInteraction) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let command_name = &interaction.data.name;
    
    match command_name.as_str() {
        "ping" => handle_ping_slash(ctx, interaction).await?,
        "echo" => handle_echo_slash(ctx, interaction).await?,
        "lm" => handle_lm_slash(ctx, interaction).await?,
        "reason" => handle_reason_slash(ctx, interaction).await?,
        "sum" => handle_sum_slash(ctx, interaction).await?,
        "rank" => handle_rank_slash(ctx, interaction).await?,
        "help" => handle_help_slash(ctx, interaction).await?,
        "clearcontext" => handle_clearcontext_slash(ctx, interaction).await?,
        "clearreasoncontext" => handle_clearreasoncontext_slash(ctx, interaction).await?,
        _ => {
            let response_text = format!("Unknown slash command: {}", command_name);
            interaction
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| message.content(response_text))
                })
                .await?;
        }
    }
    
    Ok(())
}

// ============================================================================
// INDIVIDUAL SLASH COMMAND HANDLERS
// ============================================================================

/// Handle /ping slash command
async fn handle_ping_slash(ctx: &Context, interaction: &ApplicationCommandInteraction) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let start_time = std::time::Instant::now();
    
    interaction
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    let elapsed = start_time.elapsed();
                    let ping_ms = elapsed.as_millis();
                    message.content(format!("Pong! Response time: {}ms", ping_ms))
                })
        })
        .await?;
    
    Ok(())
}

/// Handle /echo slash command
async fn handle_echo_slash(ctx: &Context, interaction: &ApplicationCommandInteraction) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let text = interaction
        .data
        .options
        .iter()
        .find(|option| option.name == "message")
        .and_then(|option| option.value.as_ref())
        .and_then(|value| value.as_str())
        .unwrap_or("No message provided");
    
    interaction
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| message.content(text))
        })
        .await?;
    
    Ok(())
}

/// Handle /lm slash command
async fn handle_lm_slash(ctx: &Context, interaction: &ApplicationCommandInteraction) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let prompt = interaction
        .data
        .options
        .iter()
        .find(|option| option.name == "prompt")
        .and_then(|option| option.value.as_ref())
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    
    if prompt.is_empty() {
        interaction
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Please provide a prompt! Usage: `/lm prompt:your question here`")
                    })
            })
            .await?;
        return Ok(());
    }
    
    // Send initial response
    interaction
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content("🤖 Processing your AI request...")
                })
        })
        .await?;
    
    // Get the interaction token for follow-up messages
    let token = interaction.token.clone();
    let http = ctx.http.clone();
    let _user_id = interaction.user.id;
    
    // Process the LM request in a separate task
    tokio::spawn(async move {
        // Create a mock message for the existing LM logic
        // This is a workaround since slash commands don't have Message objects
        let response_content = format!("🤖 **AI Response**\n\n**Your Question:** {}\n\n**AI Response:** This is a placeholder response from the slash command. The actual LM integration with your existing AI logic will be implemented in the next update.\n\n*Note: This slash command is currently using placeholder responses. For full AI functionality, use the prefix commands (^lm) or mention the bot directly.*", prompt);
        
        // Send follow-up message
        if let Err(e) = http
            .create_followup_message(&token, &serde_json::json!({
                "content": response_content
            }))
            .await
        {
            eprintln!("Failed to send LM response: {}", e);
        }
    });
    
    Ok(())
}

/// Handle /reason slash command
async fn handle_reason_slash(ctx: &Context, interaction: &ApplicationCommandInteraction) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let question = interaction
        .data
        .options
        .iter()
        .find(|option| option.name == "question")
        .and_then(|option| option.value.as_ref())
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    
    if question.is_empty() {
        interaction
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Please provide a question! Usage: `/reason question:your reasoning question`")
                    })
            })
            .await?;
        return Ok(());
    }
    
    // Send initial response
    interaction
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content("🧠 Processing your reasoning request...")
                })
        })
        .await?;
    
    // Get the interaction token for follow-up messages
    let token = interaction.token.clone();
    let http = ctx.http.clone();
    
    // Process the reasoning request in a separate task
    tokio::spawn(async move {
        // Here you would call your existing reasoning logic
        // For now, we'll send a simple response
        let response_content = format!("🧠 **Reasoning Analysis**\n\nQuestion: {}\n\nThis is a placeholder response. The actual reasoning integration will be implemented here.", question);
        
        // Send follow-up message
        if let Err(e) = http
            .create_followup_message(&token, &serde_json::json!({
                "content": response_content
            }))
            .await
        {
            eprintln!("Failed to send reasoning response: {}", e);
        }
    });
    
    Ok(())
}

/// Handle /sum slash command
async fn handle_sum_slash(ctx: &Context, interaction: &ApplicationCommandInteraction) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = interaction
        .data
        .options
        .iter()
        .find(|option| option.name == "url")
        .and_then(|option| option.value.as_ref())
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    
    if url.is_empty() {
        interaction
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Please provide a URL! Usage: `/sum url:https://example.com`")
                    })
            })
            .await?;
        return Ok(());
    }
    
    // Send initial response
    interaction
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content("📄 Processing summarization request...")
                })
        })
        .await?;
    
    // Get the interaction token for follow-up messages
    let token = interaction.token.clone();
    let http = ctx.http.clone();
    
    // Process the summarization request in a separate task
    tokio::spawn(async move {
        // Here you would call your existing summarization logic
        // For now, we'll send a simple response
        let response_content = format!("📄 **Content Summarization**\n\nURL: {}\n\nThis is a placeholder response. The actual summarization integration will be implemented here.", url);
        
        // Send follow-up message
        if let Err(e) = http
            .create_followup_message(&token, &serde_json::json!({
                "content": response_content
            }))
            .await
        {
            eprintln!("Failed to send summarization response: {}", e);
        }
    });
    
    Ok(())
}

/// Handle /rank slash command
async fn handle_rank_slash(ctx: &Context, interaction: &ApplicationCommandInteraction) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = interaction
        .data
        .options
        .iter()
        .find(|option| option.name == "url")
        .and_then(|option| option.value.as_ref())
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    
    let analysis_type = interaction
        .data
        .options
        .iter()
        .find(|option| option.name == "analysis_type")
        .and_then(|option| option.value.as_ref())
        .and_then(|value| value.as_str())
        .unwrap_or("comprehensive")
        .to_string();
    
    if url.is_empty() {
        interaction
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Please provide a URL! Usage: `/rank url:https://example.com`")
                    })
            })
            .await?;
        return Ok(());
    }
    
    // Send initial response
    interaction
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content("📊 Processing ranking analysis...")
                })
        })
        .await?;
    
    // Get the interaction token for follow-up messages
    let token = interaction.token.clone();
    let http = ctx.http.clone();
    
    // Process the ranking request in a separate task
    tokio::spawn(async move {
        // Here you would call your existing ranking logic
        // For now, we'll send a simple response
        let response_content = format!("📊 **Content Ranking Analysis**\n\nURL: {}\nAnalysis Type: {}\n\nThis is a placeholder response. The actual ranking integration will be implemented here.", url, analysis_type);
        
        // Send follow-up message
        if let Err(e) = http
            .create_followup_message(&token, &serde_json::json!({
                "content": response_content
            }))
            .await
        {
            eprintln!("Failed to send ranking response: {}", e);
        }
    });
    
    Ok(())
}

/// Handle /help slash command
async fn handle_help_slash(ctx: &Context, interaction: &ApplicationCommandInteraction) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let help_text = r#"**🤖 Meri Bot - Slash Commands Help**

**📝 Basic Commands:**
• `/ping` - Test bot connectivity
• `/echo message:text` - Echo back your message
• `/help` - Show this help message

**🧠 AI & Language Model Commands:**
• `/lm prompt:your question` - AI chat with personal context *(placeholder)*
• `/reason question:your reasoning question` - Advanced reasoning and analysis *(placeholder)*
• `/clearcontext` - Clear your personal chat context
• `/clearreasoncontext` - Clear your personal reasoning context

**🔍 Analysis Commands:**
• `/sum url:https://example.com` - Text summarization *(placeholder)*
• `/rank url:https://example.com` - Content ranking and analysis *(placeholder)*

**💡 Usage Examples:**
• `/lm prompt:What is the weather like?`
• `/reason question:Analyze this problem: 2+2=?`
• `/sum url:https://example.com`
• `/rank url:https://example.com analysis_type:usability`

**🔗 Additional Features:**
• Mention the bot with `<@Bot> <prompt>` for global context chat
• Use prefix commands (^lm, ^reason, ^sum) for full AI functionality
• Reply to messages for context-aware responses
• Attach files for document analysis
• Attach images for vision analysis

**⚠️ Note:**
Slash commands are currently in development. For full AI functionality, use:
• Prefix commands: `^lm`, `^reason`, `^sum`, `^rank`
• Bot mentions: `<@Bot> <prompt>`

**❓ Need More Help?**
Use `/help` for general help or mention the bot for AI chat."#;

    interaction
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| message.content(help_text))
        })
        .await?;
    
    Ok(())
}

/// Handle /clearcontext slash command
async fn handle_clearcontext_slash(ctx: &Context, interaction: &ApplicationCommandInteraction) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let user_id = interaction.user.id;
    
    // Clear the user's LM context
    {
        let mut data = ctx.data.write().await;
        if let Some(lm_contexts) = data.get_mut::<LmContextMap>() {
            lm_contexts.remove(&user_id);
        }
    }
    
    interaction
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content("✅ Your personal chat context has been cleared!")
                })
        })
        .await?;
    
    Ok(())
}

/// Handle /clearreasoncontext slash command
async fn handle_clearreasoncontext_slash(ctx: &Context, interaction: &ApplicationCommandInteraction) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let user_id = interaction.user.id;
    
    // Clear the user's reasoning context
    {
        let mut data = ctx.data.write().await;
        if let Some(reason_contexts) = data.get_mut::<ReasonContextMap>() {
            reason_contexts.remove(&user_id);
        }
    }
    
    interaction
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content("✅ Your personal reasoning context has been cleared!")
                })
        })
        .await?;
    
    Ok(())
}

// ============================================================================
// SLASH COMMAND REGISTRATION
// ============================================================================

/// Register all slash commands with Discord
pub async fn register_slash_commands(http: &serenity::http::Http) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let commands = vec![
        // Basic commands
        Command::create_global_application_command(http, |command| {
            command
                .name("ping")
                .description("Test bot connectivity and response time")
        })
        .await?,
        
        Command::create_global_application_command(http, |command| {
            command
                .name("echo")
                .description("Echo back your message")
                .create_option(|option| {
                    option
                        .name("message")
                        .description("The message to echo")
                        .kind(CommandOptionType::String)
                        .required(true)
                })
        })
        .await?,
        
        // AI commands
        Command::create_global_application_command(http, |command| {
            command
                .name("lm")
                .description("AI chat with personal context")
                .create_option(|option| {
                    option
                        .name("prompt")
                        .description("Your question or prompt for the AI")
                        .kind(CommandOptionType::String)
                        .required(true)
                })
        })
        .await?,
        
        Command::create_global_application_command(http, |command| {
            command
                .name("reason")
                .description("Advanced reasoning and analysis")
                .create_option(|option| {
                    option
                        .name("question")
                        .description("Your reasoning question")
                        .kind(CommandOptionType::String)
                        .required(true)
                })
        })
        .await?,
        
        // Analysis commands
        Command::create_global_application_command(http, |command| {
            command
                .name("sum")
                .description("Summarize web content or documents")
                .create_option(|option| {
                    option
                        .name("url")
                        .description("URL to summarize")
                        .kind(CommandOptionType::String)
                        .required(true)
                })
        })
        .await?,
        
        Command::create_global_application_command(http, |command| {
            command
                .name("rank")
                .description("Rank and analyze web content")
                .create_option(|option| {
                    option
                        .name("url")
                        .description("URL to analyze")
                        .kind(CommandOptionType::String)
                        .required(true)
                })
                .create_option(|option| {
                    option
                        .name("analysis_type")
                        .description("Type of analysis to perform")
                        .kind(CommandOptionType::String)
                        .required(false)
                        .add_string_choice("Comprehensive", "comprehensive")
                        .add_string_choice("Usability", "usability")
                        .add_string_choice("Quality", "quality")
                        .add_string_choice("Accessibility", "accessibility")
                        .add_string_choice("SEO", "seo")
                        .add_string_choice("Performance", "performance")
                        .add_string_choice("Security", "security")
                })
        })
        .await?,
        
        // Utility commands
        Command::create_global_application_command(http, |command| {
            command
                .name("help")
                .description("Show help information for all commands")
        })
        .await?,
        
        Command::create_global_application_command(http, |command| {
            command
                .name("clearcontext")
                .description("Clear your personal chat context")
        })
        .await?,
        
        Command::create_global_application_command(http, |command| {
            command
                .name("clearreasoncontext")
                .description("Clear your personal reasoning context")
        })
        .await?,
    ];
    
    println!("✅ Registered {} slash commands with Discord", commands.len());
    println!("📋 Available slash commands:");
    for cmd in &commands {
        println!("DEBUG: cmd.description = {:?}", cmd.description);
    }
    
    Ok(())
} 