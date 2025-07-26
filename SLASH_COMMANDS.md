# Slash Commands Documentation

## Overview

Meri Bot now supports Discord slash commands! This allows you to use bot commands in private messages and provides a better user experience with Discord's native slash command interface.

## 🆕 Available Slash Commands

### Basic Commands
- `/ping` - Test bot connectivity and response time
- `/echo message:text` - Echo back your message
- `/help` - Show comprehensive help information

### AI & Language Model Commands
- `/lm prompt:your question` - AI chat with personal context *(currently placeholder)*
- `/reason question:your reasoning question` - Advanced reasoning and analysis *(currently placeholder)*
- `/clearcontext` - Clear your personal chat context
- `/clearreasoncontext` - Clear your personal reasoning context

### Analysis Commands
- `/sum url:https://example.com` - Text summarization *(currently placeholder)*
- `/rank url:https://example.com` - Content ranking and analysis *(currently placeholder)*

## 🎯 Usage Examples

### Basic Commands
```
/ping
/echo message:Hello, world!
/help
```

### AI Commands
```
/lm prompt:What is the weather like today?
/reason question:Analyze this problem: 2+2=?
/clearcontext
/clearreasoncontext
```

### Analysis Commands
```
/sum url:https://example.com
/rank url:https://example.com analysis_type:usability
```

## 🔧 Command Options

### `/rank` Analysis Types
When using `/rank`, you can specify the analysis type:
- `comprehensive` - Full analysis (default)
- `usability` - Usability-focused analysis
- `quality` - Content quality analysis
- `accessibility` - Accessibility analysis
- `seo` - SEO analysis
- `performance` - Performance analysis
- `security` - Security analysis

## ⚠️ Important Notes

### Current Status
- **Basic commands** (`/ping`, `/echo`, `/help`, `/clearcontext`, `/clearreasoncontext`) are fully functional
- **AI commands** (`/lm`, `/reason`, `/sum`, `/rank`) are currently placeholder implementations
- For full AI functionality, continue using:
  - Prefix commands: `^lm`, `^reason`, `^sum`, `^rank`
  - Bot mentions: `<@Bot> <prompt>`

### Private Message Support
- All slash commands work in private messages (DMs)
- No prefix required - just type `/` and select the command
- Commands are automatically registered when the bot starts

## 🚀 Future Updates

The following features are planned for future updates:
- Full AI integration for `/lm` command
- Complete reasoning functionality for `/reason` command
- Document summarization for `/sum` command
- Content ranking analysis for `/rank` command
- File attachment support for slash commands
- Vision analysis support for slash commands

## 🔗 Related Documentation

- [Main README](../README.md) - Complete bot documentation
- [Configuration Guide](../CONFIGURATION.md) - Setup and configuration
- [Command Reference](../PLANNING.md) - All available commands

## 💡 Tips

1. **Auto-completion**: Discord will show available commands when you type `/`
2. **Parameter hints**: Required parameters are marked with asterisks (*)
3. **Error handling**: Invalid commands will show helpful error messages
4. **Context clearing**: Use `/clearcontext` and `/clearreasoncontext` to reset your conversation history
5. **Help system**: Use `/help` anytime to see all available commands

## 🐛 Troubleshooting

### Commands not appearing
- Make sure the bot has the `applications.commands` scope when invited
- Commands may take up to 1 hour to appear globally
- Try restarting the bot to re-register commands

### Permission errors
- Ensure the bot has the necessary permissions in your server
- Check that the bot can send messages in the channel

### AI commands not working
- AI slash commands are currently placeholder implementations
- Use prefix commands (`^lm`, `^reason`) or bot mentions for full AI functionality
- Check the bot's configuration files for AI model settings 