use serenity::{
    client::Context,
    framework::standard::{
        macros::command,
        Args, CommandResult,
    },
    model::channel::Message,
};
use std::env;

#[command]
pub async fn help(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    let prefix = env::var("PREFIX").unwrap_or_else(|_| "!".to_string());
    
    let response = format!(
        "**🤖 Meri Bot - Discord AI Assistant**\n\n\
        **📋 Basic Commands:**\n\
        • `{0}ping` - Test bot connectivity with response time\n\
        • `{0}echo <text>` - Echo your message\n\
        • `{0}help` - Show this help\n\n\
        **🖼️ Profile Picture:**\n\
        • `{0}ppfp @user` - Show user's profile picture\n\
        • **Aliases:** `{0}avatar`, `{0}pfp`, `{0}profilepic`\n\n\
        **🤖 AI Chat (LM Studio/Ollama):**\n\
        • `{0}lm <prompt>` - AI chat with real-time streaming\n\
        • `{0}lm -s <query>` - AI-enhanced web search with embedded links (SerpAPI only)\n\
        • `{0}lm --test` - Test connectivity to remote API server\n\
        • **Aliases:** `{0}llm`, `{0}ai`, `{0}chat`\n\n\
        **🧠 AI Reasoning:**\n\
        • `{0}reason <question>` - Specialized reasoning with real-time streaming and thinking tag filtering\n\
        • `{0}reason -s <query>` - Reasoning-enhanced analytical search with buffered chunking and <think> tag filtering (posts content in 2000-character chunks)\n\
        • **Aliases:** `{0}reasoning`\n\n\
        **🔍 Search Features:**\n\
        • **SerpAPI Integration:** Official search API with AI enhancement\n\
        • **AI Mode:** Direct search → SerpAPI → AI summary with embedded links\n\
        • Real-time progress updates and smart formatting\n\n\
        **⚡ Advanced Features:**\n\
        • Real-time streaming responses (0.8s updates)\n\
        • Smart message chunking for long responses\n\
        • Thinking tag filtering for reasoning\n\
        • Buffered chunking for analytical search\n\
        • Multi-path configuration loading\n\
        • Comprehensive error handling\n\n\
        **🛠️ Setup:**\n\
        • **Required:** `botconfig.txt` with Discord token\n\
        • **AI Features:** `lmapiconf.txt` with LM Studio/Ollama config and SerpAPI key\n\
        • **Optional:** Custom prompts for search and reasoning\n\n\
        **🚀 Quick Test:**\n\
        `{0}ping` (shows response time) → `{0}lm -s rust tutorial` → `{0}lm Hello!` → `{0}reason Why is the sky blue?`\n\n\
        **📚 Full setup guide:** Check README.md for detailed instructions!", 
        prefix
    );
    
    msg.reply(ctx, &response).await?;
    Ok(())
} 