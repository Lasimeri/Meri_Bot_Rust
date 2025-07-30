// commands/mod.rs - Command Module Registry
// This file declares all command modules and provides a centralized registry
// for all bot commands, making them easily accessible from main.rs

pub mod admin;          // Administrative commands (owner only)
pub mod echo;           // Echo command for testing
pub mod help;           // Help system and command documentation
pub mod ping;           // Basic ping/pong functionality
pub mod lm;             // Language model integration (AI chat)
pub mod reason;         // Reasoning and analysis capabilities
pub mod agent;          // LLM Agent with function calling using js-code-sandbox
pub mod search;         // Web search and RAG (Retrieval-Augmented Generation) - Minimal placeholder
pub mod sum;            // Text summarization capabilities
pub mod rank;           // Content ranking and analysis capabilities
pub mod vis;            // Vision/visual analysis capabilities 
pub mod slash;          // Slash commands for Discord application commands 