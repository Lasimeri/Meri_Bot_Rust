use serenity::{
    client::Context,
    framework::standard::{
        macros::command,
        Args, CommandResult,
    },
    model::channel::Message,
};

#[command]
pub async fn help(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    
    let response = format!(
        "**Meri Bot - Discord AI Assistant**\n\n\
        **⚠️ AI features require `<@Meri_>` user ID mentions**\n\n\
        **Basic Commands:**\n\
        • `^ping` - Test connectivity • `^echo <text>` - Echo message\n\
        • `^help` - Show help • `^ppfp @user` - Profile picture\n\n\
        **AI Chat (Primary):**\n\
        • `<@Meri_> <prompt>` - AI chat with memory\n\
        • `<@Meri_> <prompt>` + files - Analyze documents/images\n\
        • `<@Meri_> -v <prompt>` + image - Vision analysis\n\
        • Reply to messages with `<@Meri_> <question>` for context\n\
        • **Files:** PDF, TXT, MD, CSV, HTML, JSON, XML, JPG, PNG, GIF, WebP\n\n\
        **Legacy Commands:**\n\
        • `^lm <prompt>` - AI chat (aliases: `^llm`, `^ai`, `^chat`)\n\
        • `^lm -s <query>` - Web search • `^lm -v <prompt>` - Vision\n\
        • `^lm --test` - Test API • `^clearcontext` - Clear history\n\n\
        • `^reason <question>` - Reasoning AI (DeepSeek R1)\n\
        • `^reason -s <query>` - Reasoning search • `^reason --clear` - Clear\n\n\
        • `^sum <url>` - Summarize webpages/YouTube videos\n\
        • **Aliases:** `^summarize`, `^webpage`\n\n\
        **Features:**\n\
        • Real-time streaming • Per-user memory • Smart chunking\n\
        • Document analysis • Context-aware replies • 60s timeout\n\n\
        **Setup:** `botconfig.txt` (token), `lmapiconf.txt` (AI config), `yt-dlp`\n\n\
        **Examples:**\n\
        • `^ping` • `<@Meri_> Hello!` • `<@Meri_> -v What's this?` (with image)\n\
        • `^lm -s rust tutorial` • `^reason Why is sky blue?` • `^sum <url>`\n\n\
        **Full guide:** README.md"
    );
    
    msg.reply(ctx, &response).await?;
    Ok(())
} 