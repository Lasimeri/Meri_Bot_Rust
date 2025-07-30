// help.rs - Help Command Module
// Provides comprehensive help information for all bot commands and features

use serenity::{
    client::Context,
    framework::standard::{macros::command, macros::group, CommandResult},
    model::channel::Message,
};

#[command]
#[aliases("h", "commands", "info")]
/// Display help information for all available commands
pub async fn help(ctx: &Context, msg: &Message) -> CommandResult {
    let help_text = r#"**🤖 Meri Bot - Command Help**

**📝 Basic Commands:**
• `^ping` - Test bot connectivity
• `^echo <message>` - Echo back your message
• `^help` - Show this help message

**🧠 AI & Language Model Commands:**
• `^lm <prompt>` - AI chat with personal context
• `<@Bot> <prompt>` - AI chat with global shared context
• `^lm --seed <number> <prompt>` - Reproducible AI responses
• `^lm -v <prompt>` - Vision analysis (attach image)
• `^lm -s <query>` - AI-enhanced web search
• `^lm --test` - Test API connectivity
• `^lm --models` - List available models in LM Studio
• `^lm --load-model` - Validate model configuration
• `^lm --clear` - Clear your personal chat context
• `^lm --clear-global` - Clear shared global context
• `^clearcontext` - Clear your personal LM chat context
• `^clearreasoncontext` - Clear your personal reasoning context

**🔍 Analysis Commands:**
• `^reason <prompt>` - Advanced reasoning and analysis
• `^sum <text>` - Text summarization
• `^sum -f <file>` - Summarize uploaded document
• `^vis <prompt>` - Visual analysis (attach image)

**💡 Usage Examples:**
• `^lm What is the weather like?` - Personal AI chat
• `<@Bot> Tell me a joke` - Shared AI chat
• `^lm --seed 42 What is the meaning of life?` - Reproducible AI response
• `^lm -v Describe this image` - Vision analysis
• `^reason Analyze this problem: 2+2=?` - Reasoning
• `^sum Summarize this text: [your text]` - Summarization

**🔗 Context Management:**
• Personal context (`^lm`) - Private conversations per user
• Global context (`<@Bot>`) - Shared conversations across all users
• Both contexts persist across bot restarts

**📋 Special Features:**
• Reply to messages for context-aware responses
• Attach files for document analysis
• Attach images for vision analysis
• Streaming responses for real-time interaction

**❓ Need More Help?**
Use `^lmhelp` for AI commands or `^reasonhelp` for analysis commands."#;

    msg.reply(ctx, help_text).await?;
    Ok(())
}

#[command]
#[aliases("lmhelp", "aihelp")]
/// Display detailed help for LM/AI commands
pub async fn lmhelp(ctx: &Context, msg: &Message) -> CommandResult {
    let lm_help_text = r#"**🧠 AI & Language Model Commands**

**📝 Basic AI Chat:**
• `^lm <prompt>` - Start a personal AI conversation
• `<@Bot> <prompt>` - Start a shared AI conversation (global context)

**🎲 Reproducible Responses:**
• `^lm --seed <number> <prompt>` - Get deterministic AI responses
• Perfect for testing, debugging, and reproducible experiments
• Same input + same seed = same output every time

**🖼️ Vision Analysis:**
• `^lm -v <prompt>` - Analyze attached images
• `<@Bot> -v <prompt>` - Analyze images with global context
• Supports: JPG, PNG, GIF, WebP formats

**🔍 AI-Enhanced Search:**
• `^lm -s <query>` - Search the web with AI refinement
• `<@Bot> -s <query>` - Search with global context
• Provides summarized, relevant results

**📄 Document Analysis (RAG):**
• `^lm <prompt>` + attach file - Analyze documents
• Supported formats: PDF, TXT, DOC, DOCX, RTF
• Extracts text and provides AI analysis

**⚙️ Utility Commands:**
• `^lm --test` - Test API connectivity and configuration
• `^lm --models` - List available models in LM Studio
• `^lm --load-model` - Validate model configuration
• `^lm --clear` - Clear your personal conversation history
• `^lm --clear-global` - Clear shared conversation history
• `^clearcontext` - Clear your personal LM chat context

**💡 Advanced Features:**
• **Reply Context**: Reply to any message for context-aware responses
• **Streaming**: Real-time response generation
• **Context Persistence**: Conversations saved across bot restarts

**🔄 Context Types:**
• **Personal Context** (`^lm`): Private per-user conversations
• **Global Context** (`<@Bot>`): Shared across all users
• **Independent**: Personal and global contexts don't interfere

**📋 Example Usage:**
```
^lm Hello! How are you today?
<@Bot> What were we just talking about?
^lm --seed 42 What is the meaning of life?
^lm -v Describe this image in detail
^lm -s latest AI developments
^lm --clear
```"#;

    msg.reply(ctx, lm_help_text).await?;
    Ok(())
}

#[command]
#[aliases("reasonhelp", "analysishelp")]
/// Display detailed help for reasoning and analysis commands
pub async fn reasonhelp(ctx: &Context, msg: &Message) -> CommandResult {
    let reason_help_text = r#"**🔍 Reasoning & Analysis Commands**

**🧠 Advanced Reasoning:**
• `^reason <prompt>` - Deep reasoning and analysis
• Uses specialized reasoning models for complex problem-solving
• Provides step-by-step thinking processes
• `^clearreasoncontext` - Clear your personal reasoning context

**📊 Text Summarization:**
• `^sum <text>` - Summarize provided text
• `^sum -f <file>` - Summarize uploaded document
• Supports multiple document formats

**🖼️ Visual Analysis:**
• `^vis <prompt>` - Analyze images and visual content
• Supports: JPG, PNG, GIF, WebP formats
• Provides detailed visual descriptions and analysis

**💡 Use Cases:**
• **Problem Solving**: Complex mathematical or logical problems
• **Text Analysis**: Summarize long documents or articles
• **Visual Understanding**: Analyze images, diagrams, or screenshots
• **Research**: Deep analysis of topics or concepts

**📋 Example Usage:**
```
^reason Solve this math problem: 15x + 7 = 22
^sum Summarize this article: [paste article text]
^sum -f [attach document]
^vis Describe what you see in this image
^clearreasoncontext
```"#;

    msg.reply(ctx, reason_help_text).await?;
    Ok(())
}

// ============================================================================
// COMMAND GROUP
// ============================================================================

#[group]
#[commands(help, lmhelp, reasonhelp)]
pub struct Help;

impl Help {
    pub const fn new() -> Self {
        Help
    }
} 