use serenity::model::id::UserId;
use crate::search::ChatMessage;
use crate::{UserProfile, SharedConversation, UserRelationships};
use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

/// Cross-user context manager
pub struct CrossUserContext {
    pub user_profiles: HashMap<UserId, UserProfile>,
    pub shared_conversations: HashMap<String, SharedConversation>,
    pub user_relationships: HashMap<UserId, UserRelationships>,
    pub conversation_histories: HashMap<UserId, Vec<ChatMessage>>,
}

impl CrossUserContext {
    pub fn new() -> Self {
        Self {
            user_profiles: HashMap::new(),
            shared_conversations: HashMap::new(),
            user_relationships: HashMap::new(),
            conversation_histories: HashMap::new(),
        }
    }

    /// Update or create user profile
    pub fn update_user_profile(
        &mut self,
        user_id: UserId,
        username: &str,
        message_content: &str,
        conversation_style: &str,
    ) {
        let now = Utc::now();
        
        // Extract keywords first to avoid borrow checker issues
        let keywords = self.extract_keywords(&message_content.to_lowercase());
        
        let profile = self.user_profiles.entry(user_id).or_insert_with(|| UserProfile {
            user_id,
            username: username.to_string(),
            display_name: None,
            context_summary: String::new(),
            interests: Vec::new(),
            conversation_style: conversation_style.to_string(),
            last_interaction: now,
            total_messages: 0,
            preferred_topics: Vec::new(),
        });

        profile.last_interaction = now;
        profile.total_messages += 1;
        profile.username = username.to_string();
        profile.conversation_style = conversation_style.to_string();

        // Update interests based on message content
        for keyword in keywords {
            if !profile.interests.contains(&keyword) {
                profile.interests.push(keyword.clone());
            }
            if !profile.preferred_topics.contains(&keyword) {
                profile.preferred_topics.push(keyword.clone());
            }
        }

        // Keep lists manageable
        if profile.interests.len() > 10 {
            profile.interests.truncate(10);
        }
        if profile.preferred_topics.len() > 10 {
            profile.preferred_topics.truncate(10);
        }

        // Update context summary
        profile.context_summary = format!(
            "{} is interested in {}. They have sent {} messages and prefer {} style conversations.",
            profile.username,
            profile.interests.join(", "),
            profile.total_messages,
            profile.conversation_style
        );
    }



    /// Extract keywords from message content
    fn extract_keywords(&self, content: &str) -> Vec<String> {
        let common_topics = [
            "programming", "rust", "python", "javascript", "ai", "machine learning",
            "gaming", "music", "art", "science", "technology", "books", "movies",
            "travel", "food", "sports", "politics", "philosophy", "history",
            "math", "physics", "chemistry", "biology", "medicine", "business",
            "finance", "education", "environment", "space", "robotics"
        ];

        let mut keywords = Vec::new();
        for topic in &common_topics {
            if content.contains(topic) {
                keywords.push(topic.to_string());
            }
        }
        keywords
    }

    /// Create or update shared conversation thread
    pub fn update_shared_conversation(
        &mut self,
        thread_id: &str,
        participants: &[UserId],
        message: &ChatMessage,
        topic: &str,
    ) {
        let now = Utc::now();
        let conversation = self.shared_conversations.entry(thread_id.to_string()).or_insert_with(|| SharedConversation {
            thread_id: thread_id.to_string(),
            participants: participants.iter().cloned().collect(),
            messages: Vec::new(),
            topic: topic.to_string(),
            created_at: now,
            last_activity: now,
            is_active: true,
        });

        conversation.messages.push(message.clone());
        conversation.last_activity = now;
        conversation.participants.extend(participants.iter().cloned());

        // Keep conversation history manageable
        if conversation.messages.len() > 50 {
            conversation.messages.drain(0..conversation.messages.len() - 50);
        }
    }

    /// Update user relationships based on message content
    pub fn update_user_relationships(
        &mut self,
        user_id: UserId,
        mentioned_users: &[UserId],
        conversation_partners: &[UserId],
    ) {
        let relationships = self.user_relationships.entry(user_id).or_insert_with(|| UserRelationships {
            user_id,
            mentioned_users: HashSet::new(),
            conversation_partners: HashSet::new(),
            shared_topics: Vec::new(),
            relationship_notes: HashMap::new(),
        });

        relationships.mentioned_users.extend(mentioned_users.iter().cloned());
        relationships.conversation_partners.extend(conversation_partners.iter().cloned());
    }

    /// Generate cross-user context for enhanced prompts
    pub fn generate_cross_user_context(
        &self,
        current_user_id: UserId,
        mentioned_users: &[UserId],
        _conversation_history: &[ChatMessage],
    ) -> String {
        let mut context = String::new();
        
        // Add current user profile
        if let Some(profile) = self.user_profiles.get(&current_user_id) {
            context.push_str(&format!("**Current User Context:** {}\n\n", profile.context_summary));
        }

        // Add mentioned users context
        if !mentioned_users.is_empty() {
            context.push_str("**Mentioned Users Context:**\n");
            for user_id in mentioned_users {
                if let Some(profile) = self.user_profiles.get(user_id) {
                    context.push_str(&format!("- {}: {}\n", profile.username, profile.context_summary));
                }
            }
            context.push_str("\n");
        }

        // Add shared conversation context
        let shared_context = self.get_shared_conversation_context(current_user_id, _conversation_history);
        if !shared_context.is_empty() {
            context.push_str(&format!("**Shared Conversation Context:**\n{}\n\n", shared_context));
        }

        // Add relationship context
        if let Some(relationships) = self.user_relationships.get(&current_user_id) {
            if !relationships.conversation_partners.is_empty() {
                context.push_str("**User Relationships:**\n");
                for partner_id in &relationships.conversation_partners {
                    if let Some(partner_profile) = self.user_profiles.get(partner_id) {
                        context.push_str(&format!("- Has conversed with {} (interests: {})\n", 
                            partner_profile.username, 
                            partner_profile.interests.join(", ")));
                    }
                }
                context.push_str("\n");
            }
        }

        // Add conversation history for mentioned users
        if !mentioned_users.is_empty() {
            context.push_str("**Mentioned Users' Recent Conversations:**\n");
            for user_id in mentioned_users {
                if let Some(profile) = self.user_profiles.get(&user_id) {
                    context.push_str(&format!("- {}:\n", profile.username));
                    if let Some(history) = self.conversation_histories.get(&user_id) {
                        let recent: Vec<_> = history.iter().rev().take(5).collect();
                        for msg in recent.iter().rev() {
                            context.push_str(&format!("  - {}: {}\n", msg.role, &msg.content[..msg.content.len().min(100)]));
                        }
                    } else {
                        context.push_str("  No recent conversation history available.\n");
                    }
                }
            }
            context.push_str("\n");
        }

        context
    }

    /// Get shared conversation context
    fn get_shared_conversation_context(
        &self,
        user_id: UserId,
        conversation_history: &[ChatMessage],
    ) -> String {
        let mut context = String::new();
        
        // Find conversations this user has participated in
        for (thread_id, conversation) in &self.shared_conversations {
            if conversation.participants.contains(&user_id) && conversation.is_active {
                context.push_str(&format!("**Thread '{}':** {}\n", thread_id, conversation.topic));
                if !conversation.messages.is_empty() {
                    let recent_messages: Vec<_> = conversation.messages.iter()
                        .rev()
                        .take(3)
                        .collect();
                    for msg in recent_messages.iter().rev() {
                        context.push_str(&format!("- {}: {}\n", msg.role, &msg.content[..msg.content.len().min(100)]));
                    }
                }
                context.push_str("\n");
            }
        }
        
        context
    }

    /// Extract mentioned users from message content
    pub fn extract_mentioned_users(&self, content: &str) -> Vec<UserId> {
        let mut mentioned = Vec::new();
        let words: Vec<&str> = content.split_whitespace().collect();
        for word in words {
            if word.chars().next().map_or(false, |c| c.is_uppercase()) { // Assume usernames start with uppercase
                if let Some(profile) = self.get_user_by_name(word) {
                    mentioned.push(profile.user_id);
                }
            }
        }
        mentioned
    }

    /// Get user profile by username (case-insensitive)
    pub fn get_user_by_name(&self, username: &str) -> Option<&UserProfile> {
        let username_lower = username.to_lowercase();
        
        // First, try exact match
        if let Some(profile) = self.user_profiles.values().find(|profile| {
            profile.username.to_lowercase() == username_lower ||
            profile.display_name.as_ref().map(|name| name.to_lowercase()) == Some(username_lower.clone())
        }) {
            return Some(profile);
        }
        
        // If no exact match, try partial match
        self.user_profiles.values().find(|profile| {
            profile.username.to_lowercase().contains(&username_lower) ||
            profile.display_name.as_ref().map(|name| name.to_lowercase().contains(&username_lower)).unwrap_or(false)
        })
    }

    /// Get user profile by UserId
    pub fn get_user_by_id(&self, user_id: UserId) -> Option<&UserProfile> {
        self.user_profiles.get(&user_id)
    }

    /// Get conversation partners from recent history
    pub fn get_conversation_partners(&self, _user_id: UserId, _recent_messages: &[ChatMessage]) -> Vec<UserId> {
        // This would analyze recent messages to find conversation partners
        // For now, return an empty vector
        Vec::new()
    }
}

/// Enhanced system prompt generator with cross-user context
pub fn generate_enhanced_system_prompt(
    base_prompt: &str,
    cross_user_context: &str,
) -> String {
    if cross_user_context.is_empty() {
        base_prompt.to_string()
    } else {
        format!(
            "{}\n\n**Cross-User Context Information:**\n{}\n\nUse this context to provide more personalized and contextually aware responses. Reference users by name when appropriate and acknowledge shared conversation history.",
            base_prompt,
            cross_user_context
        )
    }
} 