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
        **⚠️ IMPORTANT: This bot only responds to direct user ID mentions!**\n\
        Use `<@Meri_>` to interact with the bot.\n\n\
        **Basic Commands:**\n\
        • `^ping` - Test bot connectivity with response time\n\
        • `^echo <text>` - Echo your message\n\
        • `^help` - Show this help message\n\n\
        **Profile Picture:**\n\
        • `^ppfp @user` - Show user's profile picture\n\
        • **Aliases:** `^avatar`, `^pfp`, `^profilepic`\n\n\
        **AI Chat (User ID Mention Only):**\n\
        • `<@Meri_> <prompt>` - AI chat with real-time streaming and **per-user conversation memory**\n\
        • `<@Meri_> <prompt>` + **attachments** - RAG-enhanced analysis of documents (PDF, TXT, images, etc.)\n\
        • `<@Meri_> -v <prompt>` + **image** - Vision analysis with AI (analyze images with custom prompts)\n\
        • **Reply Support:** Reply to any message with `<@Meri_> <question>` to ask about that specific message\n\
        • **Vision in Replies:** Reply to messages with images using `<@Meri_> -v <prompt>` to analyze the image\n\
        • **Supported file types:** PDF, TXT, MD, CSV, HTML, JSON, XML, JPG, PNG, GIF, WebP\n\
        • **RAG Features:** Document content extraction, context-aware analysis, multimodal support\n\
        • **User Tracking:** Automatic profile building, conversation history, interest detection\n\n\
        **Legacy Commands (^lm, ^reason, ^sum):**\n\
        • `^lm <prompt>` - AI chat with LM Studio/Ollama\n\
        • `^lm -s <query>` - AI-enhanced web search with embedded links (SerpAPI only)\n\
        • `^lm -v <prompt>` + **image** - Vision analysis (analyze attached images)\n\
        • `^lm --test` - Test connectivity to remote API server\n\
        • `^lm --clear` - Clear your conversation history\n\
        • `^clearcontext` - Clear your LM chat conversation history (dedicated command)\n\
        • **Aliases:** `^llm`, `^ai`, `^chat` (for lm) | `^clearlm`, `^resetlm` (for clearcontext)\n\n\
        **AI Reasoning:**\n\
        • `^reason <question>` - Specialized reasoning with real-time streaming, thinking tag filtering, and **per-user conversation memory** (using DeepSeek R1 model)\n\
        • `^reason -s <query>` - Reasoning-enhanced analytical search with buffered chunking and <think> tag filtering (posts content in 2000-character chunks)\n\
        • `^reason --clear` - Clear your reasoning conversation history\n\
        • **Aliases:** `^reasoning`\n\n\
        **Webpage Summarization:**\n\
        • `^sum <url>` - Summarize webpage content or YouTube videos using AI reasoning model\n\
        • **Aliases:** `^summarize`, `^webpage`\n\
        • **Features:** \n\
          - YouTube transcript extraction with yt-dlp\n\
          - HTML content extraction and cleaning\n\
          - RAG (map-reduce) summarization for long content\n\
          - Automatic reasoning tag filtering (<think> sections removed)\n\
          - 60-second timeout for reliable processing\n\
          - Streaming responses with progress updates\n\
        • **Examples:** `^sum https://youtube.com/watch?v=...`, `^sum https://example.com`\n\n\
        **Search Features:**\n\
        • **SerpAPI Integration:** Official search API with AI enhancement\n\
        • **AI Mode:** Direct search → SerpAPI → AI summary with embedded links\n\
        • Real-time progress updates and smart formatting\n\n\
        **Advanced Features:**\n\
        • Real-time streaming responses (0.8s updates)\n\
        • Smart message chunking for long responses\n\
        • Thinking tag filtering for reasoning\n\
        • Buffered chunking for analytical search\n\
        • Multi-path configuration loading\n\
        • Comprehensive error handling\n\
        • 60-second timeout for all AI requests\n\
        • YouTube transcript extraction and processing\n\
        • Vision analysis with image attachments\n\
        • Reply-based context awareness\n\n\
        **Setup:**\n\
        • **Required:** `botconfig.txt` with Discord token\n\
        • **AI Features:** `lmapiconf.txt` with LM Studio/Ollama config and SerpAPI key\n\
        • **YouTube Support:** yt-dlp installed for video transcript extraction\n\
        • **Optional:** Custom prompts for search and reasoning\n\n\
        **Quick Test Examples:**\n\
        • `^ping` (shows response time)\n\
        • `<@Meri_> Hello!` (basic AI chat)\n\
        • `<@Meri_> -v What's in this image?` (with image attached)\n\
        • Reply to a message with `<@Meri_> What does this mean?`\n\
        • `^lm -s rust tutorial` (legacy search)\n\
        • `^reason Why is the sky blue?` (legacy reasoning)\n\
        • `^sum https://youtube.com/watch?v=...` (legacy summarization)\n\n\
        **Full setup guide:** Check README.md for detailed instructions!"
    );
    
    msg.reply(ctx, &response).await?;
    Ok(())
} 