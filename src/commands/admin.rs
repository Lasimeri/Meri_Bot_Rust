// admin.rs - Administrative commands for bot management
// This module contains commands that only the bot owner can use
// Includes restart functionality and other administrative tasks

use serenity::{
    client::Context,
    framework::standard::{macros::command, macros::group, Args, CommandResult},
    model::channel::Message,
};
use std::env;
use std::process::Command;
use std::time::Duration;
use crate::commands::search::{load_lm_config, get_http_client};

#[command]
#[aliases("reboot", "restartbot")]
/// Restart the bot (owner only)
/// This command will gracefully shut down the bot and restart it
/// Only the bot owner can use this command
pub async fn restart(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    // Get the bot owner ID from configuration
    let bot_owner_id = env::var("BOT_OWNER_ID").unwrap_or_else(|_| {
        // Fallback to bot user ID if owner ID not set
        env::var("BOT_USER_ID").unwrap_or_else(|_| "1385309017881968761".to_string())
    });
    
    // Check if the user is the bot owner
    if msg.author.id.to_string() != bot_owner_id {
        msg.reply(ctx, "❌ **Access Denied**\nThis command can only be used by the bot owner.").await?;
        return Ok(());
    }
    
    // Send confirmation message
    let mut confirmation_msg = msg.reply(ctx, "🔄 **Bot Restart Initiated**\n\nSaving contexts and shutting down gracefully...\nThe bot will restart automatically.").await?;
    
    // Log the restart request
    println!("[ADMIN] Bot restart requested by owner {} ({})", msg.author.name, msg.author.id);
    
    // Save contexts to disk before restart
    {
        let data = ctx.data.read().await;
        let lm_contexts = data.get::<crate::LmContextMap>().cloned().unwrap_or_default();
        let reason_contexts = data.get::<crate::ReasonContextMap>().cloned().unwrap_or_default();
        let global_lm_context = data.get::<crate::GlobalLmContextMap>().cloned().unwrap_or_else(|| crate::UserContext::new());
        
        println!("[ADMIN] Saving contexts before restart...");
        if let Err(e) = crate::save_contexts_to_disk(&lm_contexts, &reason_contexts, &global_lm_context).await {
            eprintln!("[ADMIN] Failed to save contexts before restart: {}", e);
        } else {
            println!("[ADMIN] Contexts saved successfully before restart");
        }
    }
    
    // Update the confirmation message
    confirmation_msg.edit(&ctx.http, |m| {
        m.content("✅ **Contexts Saved**\n\n🔄 **Restarting Bot...**\n\nThe bot is now restarting. Please wait a moment for it to come back online.")
    }).await?;
    
    // Small delay to ensure the message is sent
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Restart the bot process
    restart_bot_process().await?;
    
    Ok(())
}

/// Restart the bot process
/// This function will restart the current executable
async fn restart_bot_process() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Get the current executable path
    let current_exe = std::env::current_exe()?;
    let exe_path = current_exe.to_string_lossy();
    
    println!("[ADMIN] Restarting bot process: {}", exe_path);
    
    // Get the current working directory
    let current_dir = std::env::current_dir()?;
    
    // Create the restart command
    let mut restart_cmd = Command::new(&*exe_path);
    restart_cmd.current_dir(current_dir);
    
    // Add any command line arguments that were passed to the original process
    let args: Vec<String> = std::env::args().skip(1).collect();
    for arg in args {
        restart_cmd.arg(arg);
    }
    
    // Spawn the new process
    match restart_cmd.spawn() {
        Ok(_) => {
            println!("[ADMIN] New bot process started successfully");
            
            // Give the new process a moment to start
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            
            // Exit the current process
            println!("[ADMIN] Exiting current process");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("[ADMIN] Failed to restart bot process: {}", e);
            return Err(format!("Failed to restart bot: {}", e).into());
        }
    }
}

#[command]
#[aliases("shutdown", "stopbot")]
/// Shutdown the bot (owner only)
/// This command will gracefully shut down the bot
/// Only the bot owner can use this command
pub async fn shutdown(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    // Get the bot owner ID from configuration
    let bot_owner_id = env::var("BOT_OWNER_ID").unwrap_or_else(|_| {
        // Fallback to bot user ID if owner ID not set
        env::var("BOT_USER_ID").unwrap_or_else(|_| "1385309017881968761".to_string())
    });
    
    // Check if the user is the bot owner
    if msg.author.id.to_string() != bot_owner_id {
        msg.reply(ctx, "❌ **Access Denied**\nThis command can only be used by the bot owner.").await?;
        return Ok(());
    }
    
    // Send confirmation message
    let mut confirmation_msg = msg.reply(ctx, "🛑 **Bot Shutdown Initiated**\n\nSaving contexts and shutting down gracefully...").await?;
    
    // Log the shutdown request
    println!("[ADMIN] Bot shutdown requested by owner {} ({})", msg.author.name, msg.author.id);
    
    // Save contexts to disk before shutdown
    {
        let data = ctx.data.read().await;
        let lm_contexts = data.get::<crate::LmContextMap>().cloned().unwrap_or_default();
        let reason_contexts = data.get::<crate::ReasonContextMap>().cloned().unwrap_or_default();
        let global_lm_context = data.get::<crate::GlobalLmContextMap>().cloned().unwrap_or_else(|| crate::UserContext::new());
        
        println!("[ADMIN] Saving contexts before shutdown...");
        if let Err(e) = crate::save_contexts_to_disk(&lm_contexts, &reason_contexts, &global_lm_context).await {
            eprintln!("[ADMIN] Failed to save contexts before shutdown: {}", e);
        } else {
            println!("[ADMIN] Contexts saved successfully before shutdown");
        }
    }
    
    // Update the confirmation message
    confirmation_msg.edit(&ctx.http, |m| {
        m.content("✅ **Contexts Saved**\n\n🛑 **Shutting Down Bot...**\n\nThe bot is now shutting down gracefully.")
    }).await?;
    
    // Small delay to ensure the message is sent
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Exit the process
    println!("[ADMIN] Exiting bot process");
    std::process::exit(0);
}

#[command]
#[aliases("adminhelp", "ahelp")]
/// Show admin command help (owner only)
/// Lists all available administrative commands
/// Only the bot owner can use this command
pub async fn adminhelp(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    // Get the bot owner ID from configuration
    let bot_owner_id = env::var("BOT_OWNER_ID").unwrap_or_else(|_| {
        // Fallback to bot user ID if owner ID not set
        env::var("BOT_USER_ID").unwrap_or_else(|_| "1385309017881968761".to_string())
    });
    
    // Check if the user is the bot owner
    if msg.author.id.to_string() != bot_owner_id {
        msg.reply(ctx, "❌ **Access Denied**\nThis command can only be used by the bot owner.").await?;
        return Ok(());
    }
    
    let help_text = "**🔧 Admin Commands**\n\n\
                    `^restart` - Restart the bot gracefully\n\
                    `^shutdown` - Shutdown the bot gracefully\n\
                    `^forcerestart` - Force restart the bot (immediate shutdown)\n\
                    `^leaveserver` - Make the bot leave the current server\n\
                    `^adminhelp` - Show this help message\n\n\
                    **Note:** These commands can only be used by the bot owner.";
    
    msg.reply(ctx, help_text).await?;
    
    Ok(())
}

#[command]
#[aliases("forcerestart", "reboot")]
/// Force restart the bot completely
/// This command will shut down the bot process and restart it
/// Only the bot owner can use this command
pub async fn forcerestart(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    // Get the bot owner ID from configuration
    let bot_owner_id = env::var("BOT_OWNER_ID").unwrap_or_else(|_| {
        // Fallback to bot user ID if owner ID not set
        env::var("BOT_USER_ID").unwrap_or_else(|_| "1385309017881968761".to_string())
    });
    
    // Check if the user is the bot owner
    if msg.author.id.to_string() != bot_owner_id {
        msg.reply(ctx, "❌ **Access Denied**\nThis command can only be used by the bot owner.").await?;
        return Ok(());
    }

    // Send confirmation message
    let mut confirmation_msg = msg.reply(ctx, "🔄 **Force Restart Initiated**\n\nShutting down bot process...\n\nThe bot will restart automatically and update this message when it comes back online.").await?;

    // Save restart message info for later update
    let restart_info = format!("{}|{}|{}", 
        msg.channel_id, 
        confirmation_msg.id, 
        chrono::Utc::now().timestamp()
    );
    
    if let Err(e) = std::fs::write("restart_message.txt", restart_info) {
        eprintln!("[FORCERESTART] Failed to save restart message info: {}", e);
    }

    // Log the restart attempt
    println!("[FORCERESTART] Bot owner {} initiated force restart", msg.author.name);
    println!("[FORCERESTART] Shutting down bot process...");

    // Give the confirmation message time to be sent
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Save contexts to disk before shutdown (optional, for safety)
    {
        let data = ctx.data.read().await;
        let lm_contexts = data.get::<crate::LmContextMap>().cloned().unwrap_or_default();
        let reason_contexts = data.get::<crate::ReasonContextMap>().cloned().unwrap_or_default();
        let global_lm_context = data.get::<crate::GlobalLmContextMap>().cloned().unwrap_or_else(|| crate::UserContext::new());
        
        println!("[FORCERESTART] Saving contexts before restart...");
        if let Err(e) = crate::save_contexts_to_disk(&lm_contexts, &reason_contexts, &global_lm_context).await {
            eprintln!("[FORCERESTART] Failed to save contexts before restart: {}", e);
        } else {
            println!("[FORCERESTART] Contexts saved successfully before restart");
        }
    }

    // Update the confirmation message
    confirmation_msg.edit(&ctx.http, |m| {
        m.content("✅ **Contexts Saved**\n\n🔄 **Force Restarting Bot...**\n\nThe bot is now shutting down and will restart.")
    }).await?;

    // Small delay to ensure the message is sent
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    println!("[FORCERESTART] Force exiting process...");
    
    // Call the restart batch file to restart the bot
    let restart_result = std::process::Command::new("cmd")
        .args(&["/C", "forcerestart.bat"])
        .current_dir(std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")))
        .spawn();
    
    match restart_result {
        Ok(_) => {
            println!("[FORCERESTART] Restart batch file executed successfully");
        }
        Err(e) => {
            eprintln!("[FORCERESTART] Failed to execute restart batch file: {}", e);
        }
    }
    
    // Exit the current process
    std::process::exit(0);
} 

#[command]
#[only_in(guilds)]
/// Test LM Studio/Ollama connectivity and diagnose issues
/// Usage: ^diagnose
pub async fn diagnose(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let start_time = std::time::Instant::now();
    
    // Initial status message
    let mut response = msg.reply(ctx, "🔍 **LM Studio Connectivity Diagnosis**\n\nTesting configuration and connectivity...").await?;
    
    // Test 1: Configuration Loading
    response.edit(ctx, |m| {
        m.content("🔍 **LM Studio Connectivity Diagnosis**\n\n**Step 1/5:** Loading configuration...")
    }).await?;
    
    let config = match load_lm_config().await {
        Ok(cfg) => {
            response.edit(ctx, |m| {
                m.content(format!(
                    "🔍 **LM Studio Connectivity Diagnosis**\n\n\
                    ✅ **Step 1/5:** Configuration loaded successfully\n\
                    • Base URL: `{}`\n\
                    • Timeout: {}s\n\
                    • Default Model: `{}`\n\
                    • Summarization Model: `{}`\n\n\
                    **Step 2/5:** Testing basic connectivity...",
                    cfg.base_url, cfg.timeout, cfg.default_model, cfg.default_summarization_model
                ))
            }).await?;
            cfg
        },
        Err(e) => {
            response.edit(ctx, |m| {
                m.content(format!(
                    "🔍 **LM Studio Connectivity Diagnosis**\n\n\
                    ❌ **Step 1/5:** Configuration loading failed\n\n\
                    **Error:** {}\n\n\
                    **Next Steps:**\n\
                    • Copy `example_lmapiconf.txt` to `lmapiconf.txt`\n\
                    • Configure all required settings\n\
                    • Ensure LM Studio is running",
                    e
                ))
            }).await?;
            return Ok(());
        }
    };
    
    // Test 2: Basic Server Connectivity
    let client = get_http_client().await;
    
    let basic_test = client
        .get(&config.base_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await;
    
    match basic_test {
        Ok(resp) => {
            response.edit(ctx, |m| {
                m.content(format!(
                    "🔍 **LM Studio Connectivity Diagnosis**\n\n\
                    ✅ **Step 1/5:** Configuration loaded\n\
                    ✅ **Step 2/5:** Basic connectivity OK (Status: {})\n\n\
                    **Step 3/5:** Testing API endpoints...",
                    resp.status()
                ))
            }).await?;
        },
        Err(e) => {
            let error_analysis = analyze_connection_error(&e);
            response.edit(ctx, |m| {
                m.content(format!(
                    "🔍 **LM Studio Connectivity Diagnosis**\n\n\
                    ✅ **Step 1/5:** Configuration loaded\n\
                    ❌ **Step 2/5:** Basic connectivity failed\n\n\
                    **Error:** {}\n\n\
                    **Analysis:** {}\n\n\
                    **Recommended Actions:**\n\
                    {}",
                    e, error_analysis.issue, error_analysis.solutions
                ))
            }).await?;
            return Ok(());
        }
    }
    
    // Test 3: API Endpoint
    let api_url = format!("{}/v1/chat/completions", config.base_url);
    let test_payload = serde_json::json!({
        "model": config.default_model,
        "messages": [{"role": "user", "content": "test"}],
        "max_tokens": 1,
        "temperature": 0.1
    });
    
    let api_test = client
        .post(&api_url)
        .json(&test_payload)
        .timeout(Duration::from_secs(30))
        .send()
        .await;
    
    let api_status = match api_test {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() || status == 400 || status == 422 {
                "✅ API endpoint responding"
            } else if status == 404 {
                "❌ API endpoint not found (404)"
            } else {
                "⚠️ API endpoint issues"
            }
        },
        Err(_) => "❌ API endpoint unreachable"
    };
    
    response.edit(ctx, |m| {
        m.content(format!(
            "🔍 **LM Studio Connectivity Diagnosis**\n\n\
            ✅ **Step 1/5:** Configuration loaded\n\
            ✅ **Step 2/5:** Basic connectivity OK\n\
            {} **Step 3/5:** {}\n\n\
            **Step 4/5:** Testing model availability...",
            if api_status.starts_with("✅") { "✅" } else if api_status.starts_with("⚠️") { "⚠️" } else { "❌" },
            api_status
        ))
    }).await?;
    
    // Test 4: Model Availability
    let models_url = format!("{}/v1/models", config.base_url);
    let models_test = client
        .get(&models_url)
        .timeout(Duration::from_secs(15))
        .send()
        .await;
    
    let (model_status, available_models) = match models_test {
        Ok(resp) => {
            if resp.status().is_success() {
                match resp.json::<serde_json::Value>().await {
                    Ok(json) => {
                        if let Some(data) = json.get("data") {
                            if let Some(models) = data.as_array() {
                                let model_names: Vec<String> = models
                                    .iter()
                                    .filter_map(|m| m.get("id")?.as_str().map(|s| s.to_string()))
                                    .collect();
                                
                                let default_available = model_names.contains(&config.default_model);
                                let summarization_available = model_names.contains(&config.default_summarization_model);
                                
                                                let status = if default_available && summarization_available {
                    "✅ All configured models available"
                } else if default_available || summarization_available {
                    "⚠️ Some configured models missing"
                } else {
                    "❌ Configured models not found"
                };
                                
                                (status.to_string(), Some(model_names))
                            } else {
                                ("❌ Invalid models response format".to_string(), None)
                            }
                        } else {
                            ("❌ No model data in response".to_string(), None)
                        }
                    },
                    Err(_) => ("❌ Could not parse models response".to_string(), None)
                }
            } else {
                ("❌ Models endpoint returned error".to_string(), None)
            }
        },
        Err(_) => ("❌ Models endpoint unreachable".to_string(), None)
    };
    
    response.edit(ctx, |m| {
        m.content(format!(
            "🔍 **LM Studio Connectivity Diagnosis**\n\n\
            ✅ **Step 1/5:** Configuration loaded\n\
            ✅ **Step 2/5:** Basic connectivity OK\n\
            {} **Step 3/5:** {}\n\
            {} **Step 4/5:** {}\n\n\
            **Step 5/5:** Running performance test...",
            if api_status.starts_with("✅") { "✅" } else if api_status.starts_with("⚠️") { "⚠️" } else { "❌" },
            api_status,
            if model_status.starts_with("✅") { "✅" } else if model_status.starts_with("⚠️") { "⚠️" } else { "❌" },
            model_status
        ))
    }).await?;
    
    // Test 5: Performance Test
    let perf_payload = serde_json::json!({
        "model": config.default_model,
        "messages": [{"role": "user", "content": "Hello! Please respond with exactly: OK"}],
        "max_tokens": 10,
        "temperature": 0.1
    });
    
    let perf_start = std::time::Instant::now();
    let perf_test = client
        .post(&api_url)
        .json(&perf_payload)
        .timeout(Duration::from_secs(60))
        .send()
        .await;
    let perf_time = perf_start.elapsed();
    
    let perf_status = match perf_test {
        Ok(resp) => {
            if resp.status().is_success() {
                match resp.json::<serde_json::Value>().await {
                    Ok(json) => {
                        if let Some(content) = json["choices"][0]["message"]["content"].as_str() {
                            format!("✅ Performance test OK ({:.2}s) - Response: \"{}\"", perf_time.as_secs_f32(), content.trim())
                        } else {
                            format!("⚠️ Performance test partial ({:.2}s) - Unexpected response format", perf_time.as_secs_f32())
                        }
                    },
                    Err(_) => {
                        format!("⚠️ Performance test partial ({:.2}s) - Could not parse response", perf_time.as_secs_f32())
                    }
                }
            } else {
                format!("❌ Performance test failed ({:.2}s) - HTTP {}", perf_time.as_secs_f32(), resp.status())
            }
        },
        Err(e) => {
            format!("❌ Performance test failed ({:.2}s) - {}", perf_time.as_secs_f32(), e)
        }
    };
    
    // Final comprehensive report
    let total_time = start_time.elapsed();
    let mut final_report = format!(
        "🔍 **LM Studio Connectivity Diagnosis Complete** ({:.2}s)\n\n\
        ✅ **Step 1/5:** Configuration loaded\n\
        ✅ **Step 2/5:** Basic connectivity OK\n\
        {} **Step 3/5:** {}\n\
        {} **Step 4/5:** {}\n\
        {} **Step 5/5:** {}\n\n",
        total_time.as_secs_f32(),
        if api_status.starts_with("✅") { "✅" } else if api_status.starts_with("⚠️") { "⚠️" } else { "❌" },
        api_status,
        if model_status.starts_with("✅") { "✅" } else if model_status.starts_with("⚠️") { "⚠️" } else { "❌" },
        model_status,
        if perf_status.starts_with("✅") { "✅" } else if perf_status.starts_with("⚠️") { "⚠️" } else { "❌" },
        perf_status
    );
    
    // Add available models info if we have it
    if let Some(models) = available_models {
        final_report.push_str(&format!(
            "**Available Models:**\n{}\n\n",
            if models.is_empty() {
                "• No models found".to_string()
            } else {
                models.iter()
                    .take(10) // Limit to 10 models to avoid Discord message limits
                    .map(|m| format!("• {}", m))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        ));
        
        if models.len() > 10 {
            final_report.push_str(&format!("• ... and {} more models\n\n", models.len() - 10));
        }
    }
    
    // Add recommendations based on test results
    final_report.push_str("**Recommendations:**\n");
    
    if api_status.starts_with("❌") {
        final_report.push_str("• ❌ **Critical:** Enable API server in LM Studio (Server tab → Start Server)\n");
    }
    
    if model_status.starts_with("❌") || model_status.starts_with("⚠️") {
        final_report.push_str("• ⚠️ **Important:** Load the required models in LM Studio\n");
        final_report.push_str(&format!("  - Default model: `{}`\n", config.default_model));
        final_report.push_str(&format!("  - Summarization model: `{}`\n", config.default_summarization_model));
    }
    
    if perf_status.starts_with("❌") {
        final_report.push_str("• 🐌 **Performance:** Consider using a smaller/faster model for better response times\n");
    } else if perf_time.as_secs_f32() > 10.0 {
        final_report.push_str("• 🐌 **Performance:** Response time is slow - consider optimizing your setup\n");
    }
    
    final_report.push_str("\n**Status:** ");
    if api_status.starts_with("✅") && model_status.starts_with("✅") && perf_status.starts_with("✅") {
        final_report.push_str("🟢 **All systems operational!**");
    } else if api_status.starts_with("❌") || model_status.starts_with("❌") {
        final_report.push_str("🔴 **Critical issues found - bot may not work properly**");
    } else {
        final_report.push_str("🟡 **Some issues found - bot should work but may have problems**");
    }
    
    response.edit(ctx, |m| m.content(&final_report)).await?;
    
    Ok(())
}

/// Analyze connection errors and provide specific guidance
struct ConnectionError {
    issue: String,
    solutions: String,
}

fn analyze_connection_error(error: &reqwest::Error) -> ConnectionError {
    let error_msg = format!("{}", error);
    
    if error_msg.contains("os error 10013") || error_msg.contains("access permissions") {
        ConnectionError {
            issue: "Windows network permission error (10013)".to_string(),
            solutions: "• Run the bot as Administrator\n• Add Windows Firewall exception\n• Try using 127.0.0.1 instead of localhost".to_string(),
        }
    } else if error_msg.contains("timeout") || error_msg.contains("timed out") {
        ConnectionError {
            issue: "Connection timeout - server not responding".to_string(),
            solutions: "• Check if LM Studio is running\n• Verify network connection\n• Ensure server isn't overloaded".to_string(),
        }
    } else if error_msg.contains("refused") || error_msg.contains("connection refused") {
        ConnectionError {
            issue: "Connection refused - server not accepting connections".to_string(),
            solutions: "• Start LM Studio application\n• Enable server in LM Studio (Server tab)\n• Check if correct port is used (1234 for LM Studio, 11434 for Ollama)".to_string(),
        }
    } else if error_msg.contains("dns") || error_msg.contains("name resolution") {
        ConnectionError {
            issue: "DNS resolution error - cannot resolve hostname".to_string(),
            solutions: "• Use IP address (127.0.0.1) instead of hostname\n• Check DNS settings\n• Verify the hostname in configuration".to_string(),
        }
    } else if error_msg.contains("ssl") || error_msg.contains("tls") || error_msg.contains("certificate") {
        ConnectionError {
            issue: "SSL/TLS certificate error".to_string(),
            solutions: "• Use http:// instead of https:// for local servers\n• Check certificate configuration\n• Update LM Studio if using HTTPS".to_string(),
        }
    } else {
        ConnectionError {
            issue: "General network connectivity issue".to_string(),
            solutions: "• Check network connection\n• Verify server URL in lmapiconf.txt\n• Ensure firewall allows connections\n• Try restarting LM Studio".to_string(),
        }
    }
}

#[command]
#[aliases("leave", "exit", "quit")]
/// Make the bot leave the current server (owner only)
/// This command will make the bot leave the server where it was used
/// Only the bot owner can use this command
pub async fn leaveserver(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    // Get the bot owner ID from configuration
    let bot_owner_id = env::var("BOT_OWNER_ID").unwrap_or_else(|_| {
        // Fallback to bot user ID if owner ID not set
        env::var("BOT_USER_ID").unwrap_or_else(|_| "1385309017881968761".to_string())
    });
    
    // Check if the user is the bot owner
    if msg.author.id.to_string() != bot_owner_id {
        msg.reply(ctx, "❌ **Access Denied**\nThis command can only be used by the bot owner.").await?;
        return Ok(());
    }
    
    // Get the guild (server) ID from the message
    let guild_id = match msg.guild_id {
        Some(id) => id,
        None => {
            msg.reply(ctx, "❌ **Error**\nThis command can only be used in a server, not in DMs.").await?;
            return Ok(());
        }
    };
    
    // Get the guild name for logging
    let guild_name = match guild_id.name(&ctx.cache) {
        Some(name) => name,
        None => "Unknown Server".to_string(),
    };
    
    // Send confirmation message
    let mut confirmation_msg = msg.reply(ctx, format!("🔄 **Leaving Server**\n\nPreparing to leave **{}**...\nThis action cannot be undone.", guild_name)).await?;
    
    // Log the leave request
    println!("[ADMIN] Bot leave server requested by owner {} ({}) for server: {} ({})", 
        msg.author.name, msg.author.id, guild_name, guild_id);
    
    // Small delay to ensure the message is sent
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    
    // Leave the server
    match guild_id.leave(&ctx.http).await {
        Ok(_) => {
            println!("[ADMIN] Successfully left server: {} ({})", guild_name, guild_id);
            
            // Try to update the confirmation message (may fail if we've already left)
            let _ = confirmation_msg.edit(&ctx.http, |m| {
                m.content(format!("✅ **Successfully Left Server**\n\nThe bot has left **{}**.\n\n👋 **Goodbye!**", guild_name))
            }).await;
        }
        Err(e) => {
            eprintln!("[ADMIN] Failed to leave server {} ({}): {}", guild_name, guild_id, e);
            
            // Try to send error message (may fail if we've already left)
            let _ = msg.reply(ctx, format!("❌ **Error Leaving Server**\n\nFailed to leave **{}**: {}", guild_name, e)).await;
        }
    }
    
    Ok(())
}

// ============================================================================
// COMMAND GROUP
// ============================================================================

#[group]
#[commands(restart, shutdown, adminhelp, forcerestart, diagnose, leaveserver)]
pub struct Admin;

impl Admin {
    pub const fn new() -> Self {
        Admin
    }
} 