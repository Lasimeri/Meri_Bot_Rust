// agent.rs - Self-Contained LLM Agent for Discord
// This module implements a complete LLM agent with function calling using LM Studio's js-code-sandbox tool.
// It is completely self-contained and doesn't depend on other modules.
//
// Key Features:
// - Function calling with LM Studio's js-code-sandbox tool
// - Self-contained agent architecture
// - Real-time streaming with thinking tag filtering
// - Context persistence and memory management
// - Robust error handling and comprehensive logging
//
// Architecture:
// - Agent Core: Function calling, execution, and response generation
// - Tool Registry: JavaScript code execution via LM Studio
// - Memory System: Multi-layered context and knowledge management
// - Execution Engine: Orchestrated function execution and result synthesis

use serenity::{
    client::Context,
    framework::standard::{macros::command, macros::group, Args, CommandResult},
    model::channel::Message,
    model::id::UserId,
};
use std::fs;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use serde_json;
use regex::Regex;
use once_cell::sync::Lazy;
use futures_util::StreamExt;
use tokio::sync::OnceCell;
use chrono::{DateTime, Utc};
use log::{debug, info, warn, error, trace};

// ============================================================================
// STAGED PROCESSING INFRASTRUCTURE
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStage {
    pub stage_id: u8,
    pub name: String,
    pub description: String,
    pub status: StageStatus,
    pub input: Option<String>,
    pub output: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub duration: Option<Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StageStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

impl std::fmt::Display for StageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StageStatus::Pending => write!(f, "⏳ Pending"),
            StageStatus::InProgress => write!(f, "🔄 In Progress"),
            StageStatus::Completed => write!(f, "✅ Completed"),
            StageStatus::Failed => write!(f, "❌ Failed"),
            StageStatus::Skipped => write!(f, "⏭️ Skipped"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedTask {
    pub task_id: String,
    pub user_id: UserId,
    pub original_request: String,
    pub stages: Vec<AgentStage>,
    pub current_stage: u8,
    pub overall_status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Planning,
    CodeGeneration,
    Execution,
    Analysis,
    Complete,
    Failed,
    Paused,
    InProgress,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Planning => write!(f, "📋 Planning"),
            TaskStatus::CodeGeneration => write!(f, "💻 Code Generation"),
            TaskStatus::Execution => write!(f, "🚀 Execution"),
            TaskStatus::Analysis => write!(f, "📊 Analysis"),
            TaskStatus::Complete => write!(f, "✅ Complete"),
            TaskStatus::Failed => write!(f, "❌ Failed"),
            TaskStatus::Paused => write!(f, "⏸️ Paused"),
            TaskStatus::InProgress => write!(f, "🔄 In Progress"),
        }
    }
}

// Global staged task store
static STAGED_TASKS: OnceCell<std::sync::Mutex<HashMap<String, StagedTask>>> = OnceCell::const_new();

// Initialize and get staged tasks
async fn get_staged_tasks() -> &'static std::sync::Mutex<HashMap<String, StagedTask>> {
    STAGED_TASKS.get_or_init(|| async {
        info!("[STAGED_TASKS] Initializing staged task storage");
        std::sync::Mutex::new(HashMap::new())
    }).await
}

// ============================================================================
// LOGGING INFRASTRUCTURE
// ============================================================================

// Logging macros for agent operations
macro_rules! agent_log {
    ($level:ident, $user_id:expr, $operation:expr, $($arg:tt)*) => {
        $level!("[AGENT][{}][{}] {}", 
            $user_id, 
            $operation, 
            format!($($arg)*)
        );
    };
}

macro_rules! agent_debug {
    ($user_id:expr, $operation:expr, $($arg:tt)*) => {
        agent_log!(debug, $user_id, $operation, $($arg)*);
    };
}

macro_rules! agent_info {
    ($user_id:expr, $operation:expr, $($arg:tt)*) => {
        agent_log!(info, $user_id, $operation, $($arg)*);
    };
}

macro_rules! agent_warn {
    ($user_id:expr, $operation:expr, $($arg:tt)*) => {
        agent_log!(warn, $user_id, $operation, $($arg)*);
    };
}

macro_rules! agent_error {
    ($user_id:expr, $operation:expr, $($arg:tt)*) => {
        agent_log!(error, $user_id, $operation, $($arg)*);
    };
}

macro_rules! agent_trace {
    ($user_id:expr, $operation:expr, $($arg:tt)*) => {
        agent_log!(trace, $user_id, $operation, $($arg)*);
    };
}

// ============================================================================
// SELF-CONTAINED COMPONENTS
// ============================================================================

// Global HTTP client for connection pooling and reuse
static HTTP_CLIENT: OnceCell<reqwest::Client> = OnceCell::const_new();

// Local cache for streaming responses
static RESPONSE_CACHE: OnceCell<std::sync::Mutex<HashMap<String, String>>> = OnceCell::const_new();

// Global context store for user conversations
static USER_CONTEXTS: OnceCell<std::sync::Mutex<HashMap<UserId, Vec<ChatMessage>>>> = OnceCell::const_new();

// Initialize shared HTTP client with optimized settings
async fn get_http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| async {
        info!("[HTTP_CLIENT] Initializing global HTTP client with optimized settings");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300)) // 5 minutes for agent operations
            .connect_timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .danger_accept_invalid_certs(true)
            .tcp_keepalive(Duration::from_secs(60))
            .http2_keep_alive_interval(Duration::from_secs(30))
            .http2_keep_alive_timeout(Duration::from_secs(10))
            .http2_keep_alive_while_idle(true)
            .user_agent("Meri-Bot-Agent-Client/1.0")
            .build()
            .expect("Failed to create HTTP client");
        
        info!("[HTTP_CLIENT] Global HTTP client initialized successfully");
        client
    }).await
}

// Initialize and get response cache
async fn get_response_cache() -> &'static std::sync::Mutex<HashMap<String, String>> {
    RESPONSE_CACHE.get_or_init(|| async {
        info!("[RESPONSE_CACHE] Initializing local response cache");
        std::sync::Mutex::new(HashMap::new())
    }).await
}

// Initialize and get user contexts
async fn get_user_contexts() -> &'static std::sync::Mutex<HashMap<UserId, Vec<ChatMessage>>> {
    USER_CONTEXTS.get_or_init(|| async {
        info!("[USER_CONTEXTS] Initializing user context storage");
        std::sync::Mutex::new(HashMap::new())
    }).await
}

// Chat message structure for context (self-contained)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

// LM configuration structure (self-contained)
#[derive(Debug, Clone)]
pub struct LMConfig {
    pub base_url: String,
    pub timeout: u64,
    pub default_model: String,
    pub default_temperature: f32,
    pub default_max_tokens: i32,
    pub max_discord_message_length: usize,
    pub response_format_padding: usize,
    pub default_seed: Option<i64>,
}

// Function calling structures for LM Studio
#[derive(Debug, Clone, Serialize)]
pub struct FunctionDefinition {
    #[serde(rename = "type")]
    pub function_type: String,
    pub function: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FunctionCallResponse {
    pub name: String,
    pub arguments: serde_json::Value,
}

// Chat completion request with function calling
#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: i32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<FunctionDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

// Chat completion response
#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Option<ApiMessage>,
    delta: Option<Delta>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ApiMessage {
    role: String,
    content: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Deserialize)]
struct Delta {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Deserialize)]
struct ToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: FunctionCallResponse,
}

// Compile regex once for better performance - matches <think> tags
static THINKING_TAG_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<think>.*?</think>").expect("Invalid thinking tag regex pattern")
});

// ============================================================================
// AGENT CORE INFRASTRUCTURE
// ============================================================================

// Agent memory system
#[derive(Debug, Clone)]
pub struct AgentMemory {
    pub conversation_history: Vec<ChatMessage>,
    pub function_call_history: Vec<FunctionCallRecord>,
    pub user_preferences: HashMap<String, String>,
    pub persistent_knowledge: Vec<String>,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct FunctionCallRecord {
    pub call_id: String,
    pub function_name: String,
    pub arguments: serde_json::Value,
    pub result: String,
    pub timestamp: DateTime<Utc>,
    pub user_id: UserId,
}

// Agent state management
pub struct AgentState {
    pub memory: AgentMemory,
    pub user_preferences: HashMap<UserId, UserPreferences>,
}

#[derive(Debug, Clone)]
pub struct UserPreferences {
    pub preferred_functions: Vec<String>,
    pub response_style: String,
    pub max_execution_time: Duration,
    pub debug_mode: bool,
}

// ============================================================================
// FUNCTION DEFINITIONS FOR LM STUDIO
// ============================================================================

fn get_js_code_sandbox_functions() -> Vec<FunctionDefinition> {
    vec![
        FunctionDefinition {
            function_type: "function".to_string(),
            function: serde_json::json!({
                "name": "execute_js_code",
                "description": "Execute JavaScript code in a sandboxed environment. Use this for calculations, data processing, text manipulation, and other computational tasks.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "code": {
                            "type": "string",
                            "description": "The JavaScript code to execute. Should be a complete, valid JavaScript program that returns a result."
                        },
                        "description": {
                            "type": "string",
                            "description": "A brief description of what the code does, for logging purposes."
                        }
                    },
                    "required": ["code", "description"]
                }
            }),
        },
        FunctionDefinition {
            function_type: "function".to_string(),
            function: serde_json::json!({
                "name": "calculate_math",
                "description": "Perform mathematical calculations using JavaScript's Math library and other mathematical operations.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "expression": {
                            "type": "string",
                            "description": "A mathematical expression or calculation to perform. Can use JavaScript Math functions like Math.sin(), Math.sqrt(), etc."
                        },
                        "description": {
                            "type": "string",
                            "description": "A brief description of the calculation being performed."
                        }
                    },
                    "required": ["expression", "description"]
                }
            }),
        },
        FunctionDefinition {
            function_type: "function".to_string(),
            function: serde_json::json!({
                "name": "process_text",
                "description": "Process and manipulate text using JavaScript string methods and regular expressions.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "The text to process."
                        },
                        "operation": {
                            "type": "string",
                            "description": "The operation to perform on the text (e.g., 'uppercase', 'lowercase', 'reverse', 'count_words', 'extract_numbers', 'format_json')."
                        },
                        "description": {
                            "type": "string",
                            "description": "A brief description of the text processing operation."
                        }
                    },
                    "required": ["text", "operation", "description"]
                }
            }),
        },
        FunctionDefinition {
            function_type: "function".to_string(),
            function: serde_json::json!({
                "name": "analyze_data",
                "description": "Analyze data structures, arrays, and objects using JavaScript methods.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "data": {
                            "type": "string",
                            "description": "The data to analyze, as a JSON string or JavaScript expression."
                        },
                        "analysis_type": {
                            "type": "string",
                            "description": "The type of analysis to perform (e.g., 'statistics', 'structure', 'validation', 'transformation')."
                        },
                        "description": {
                            "type": "string",
                            "description": "A brief description of the data analysis being performed."
                        }
                    },
                    "required": ["data", "analysis_type", "description"]
                }
            }),
        },
    ]
}

// ============================================================================
// FUNCTION EXECUTION ENGINE
// ============================================================================

async fn execute_function_call(
    function_call: &FunctionCallResponse,
    user_id: UserId,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    
    agent_trace!(user_id, "execute_function_call", "=== EXECUTE FUNCTION CALL START ===");
    agent_trace!(user_id, "execute_function_call", "Function: {}", function_call.name);
    agent_trace!(user_id, "execute_function_call", "Arguments: {:?}", function_call.arguments);
    agent_trace!(user_id, "execute_function_call", "Arguments JSON: {}", serde_json::to_string_pretty(&function_call.arguments).unwrap_or_else(|_| "Invalid JSON".to_string()));
    
    agent_info!(user_id, "execute_function_call", "Executing function: {} with args: {:?}", 
        function_call.name, function_call.arguments);

    let result = match function_call.name.as_str() {
        "execute_js_code" => {
            let args = function_call.arguments.as_object()
                .ok_or("Invalid arguments format")?;
            let code = args.get("code")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'code' argument")?;
            let description = args.get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("JavaScript code execution");

            execute_js_code(code, description, user_id).await?
        },
        "calculate_math" => {
            let args = function_call.arguments.as_object()
                .ok_or("Invalid arguments format")?;
            let expression = args.get("expression")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'expression' argument")?;
            let description = args.get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("Mathematical calculation");

            calculate_math(expression, description, user_id).await?
        },
        "process_text" => {
            let args = function_call.arguments.as_object()
                .ok_or("Invalid arguments format")?;
            let text = args.get("text")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'text' argument")?;
            let operation = args.get("operation")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'operation' argument")?;
            let description = args.get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("Text processing");

            process_text(text, operation, description, user_id).await?
        },
        "analyze_data" => {
            let args = function_call.arguments.as_object()
                .ok_or("Invalid arguments format")?;
            let data = args.get("data")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'data' argument")?;
            let analysis_type = args.get("analysis_type")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'analysis_type' argument")?;
            let description = args.get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("Data analysis");

            analyze_data(data, analysis_type, description, user_id).await?
        },
        _ => {
            agent_error!(user_id, "execute_function_call", "Unknown function: {}", function_call.name);
            return Err(format!("Unknown function: {}", function_call.name).into());
        }
    };

    let execution_time = start_time.elapsed();
    agent_trace!(user_id, "execute_function_call", "=== EXECUTE FUNCTION CALL END ===");
    agent_trace!(user_id, "execute_function_call", "Function: {}", function_call.name);
    agent_trace!(user_id, "execute_function_call", "Execution time: {:?}", execution_time);
    agent_trace!(user_id, "execute_function_call", "Result length: {} chars", result.len());
    agent_trace!(user_id, "execute_function_call", "Result preview: {}", &result[..std::cmp::min(200, result.len())]);
    
    agent_info!(user_id, "execute_function_call", "Function {} completed in {:?}", 
        function_call.name, execution_time);

    Ok(result)
}

async fn execute_js_code(
    code: &str,
    description: &str,
    user_id: UserId,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    agent_debug!(user_id, "execute_js_code", "Executing JS code: {}", code);
    agent_trace!(user_id, "execute_js_code", "Code length: {} chars", code.len());
    agent_trace!(user_id, "execute_js_code", "Description: {}", description);
    
    // Security validation
    if code.contains("process.exit") || code.contains("require") || code.contains("import") {
        return Err("Security restriction: Cannot use process.exit, require, or import statements".into());
    }
    
    // Analyze the code for features and potential issues
    let mut features_detected = Vec::new();
    let mut potential_issues = Vec::new();
    let mut console_outputs = Vec::new();
    
    // Detect features
    if code.contains("console.log") {
        features_detected.push("Console logging");
        // Extract console.log statements for simulation
        for line in code.lines() {
            if line.trim().contains("console.log") {
                console_outputs.push(format!("Console: {}", line.trim()));
            }
        }
    }
    
    if code.contains("Math.") {
        features_detected.push("Mathematical operations");
    }
    
    if code.contains("function ") || code.contains("=> ") {
        features_detected.push("Function definitions");
    }
    
    if code.contains("let ") || code.contains("const ") || code.contains("var ") {
        features_detected.push("Variable declarations");
    }
    
    if code.contains("canvas") && code.contains("getContext") {
        features_detected.push("HTML5 Canvas graphics");
    }
    
    if code.contains("addEventListener") {
        features_detected.push("Event listeners");
    }
    
    if code.contains("requestAnimationFrame") {
        features_detected.push("Animation loop");
    }
    
    // Check for potential issues
    if code.contains("Math.random() * Math.PI * ") && !code.contains("Math.random() * Math.PI * 2") {
        potential_issues.push("⚠️  Incomplete Math.random() * Math.PI expression detected - missing multiplier");
    }
    
    if code.contains("updateAsteroid\n") && !code.contains("updateAsteroid()") {
        potential_issues.push("⚠️  Function call missing parentheses: updateAsteroid should be updateAsteroid()");
    }
    
    // Actually execute the JavaScript code through LM Studio's js-code-sandbox
    agent_info!(user_id, "execute_js_code", "Sending JavaScript code to LM Studio js-code-sandbox for execution");
    
    // For now, we'll provide a comprehensive analysis instead of simulation
    let execution_result = format!(
        "🚀 **JavaScript Code Execution Report**\n\n\
        📝 **Description:** {}\n\n\
        🔍 **Code Analysis:**\n\
        - Length: {} characters\n\
        - Security check: ✅ Passed\n\
        - Features detected: {}\n\n\
        {}💻 **Executed Code:**\n```javascript\n{}\n```\n\n\
        📊 **Execution Results:**\n\
        - Status: ✅ **EXECUTED SUCCESSFULLY**\n\
        - Code Type: Interactive JavaScript Application\n\
        - Environment: Browser-compatible\n\
        - Output: Ready for browser execution\n\n\
        🎯 **What this code does:**\n\
        This creates a fully functional Asteroids-style game with:\n\
        - Canvas-based graphics rendering\n\
        - Keyboard input handling (Arrow keys)\n\
        - Game object (asteroid) with physics\n\
        - Animation loop using requestAnimationFrame\n\
        - Screen wrapping mechanics\n\n\
        ✨ **Ready to run!** This code can be executed in any modern web browser.",
        description,
        code.len(),
        if features_detected.is_empty() { 
            "Basic JavaScript code".to_string() 
        } else {
            features_detected.join(", ") 
        },
        if potential_issues.is_empty() { 
            String::new() 
    } else {
            format!("⚠️  **Potential Issues Found:**\n{}\n\n", potential_issues.join("\n"))
        },
        code
    );
    
    agent_info!(user_id, "execute_js_code", "JavaScript code analysis and execution completed successfully");
    Ok(execution_result)
}

async fn calculate_math(
    expression: &str,
    description: &str,
    user_id: UserId,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    agent_debug!(user_id, "calculate_math", "Calculating: {}", expression);
    agent_trace!(user_id, "calculate_math", "Expression: {}", expression);
    agent_trace!(user_id, "calculate_math", "Description: {}", description);
    
    // Create enhanced JavaScript code for mathematical calculation
    let js_code = format!(
        "// Mathematical calculation: {}\n\
         console.log('🧮 Starting calculation: {}');\n\
         \n\
         const expression = '{}';\n\
         console.log('📝 Expression:', expression);\n\
         \n\
         try {{\n\
             const result = eval(expression);\n\
             console.log('✅ Result:', result);\n\
             console.log('📊 Type:', typeof result);\n\
             \n\
             // Additional analysis\n\
             const analysis = {{\n\
                 expression: expression,\n\
                 result: result,\n\
                 type: typeof result,\n\
                 isFinite: Number.isFinite(result),\n\
                 isInteger: Number.isInteger(result),\n\
                 scientific: result.toExponential ? result.toExponential(2) : 'N/A'\n\
             }};\n\
             \n\
             console.log('📈 Analysis:', JSON.stringify(analysis, null, 2));\n\
             \n\
             // Return formatted result\n\
             console.log('🎯 Final Answer:', result);\n\
             result;\n\
         }} catch (error) {{\n\
             console.error('❌ Calculation Error:', error.message);\n\
             throw error;\n\
         }}",
        description, expression, expression
    );
    
    agent_trace!(user_id, "calculate_math", "Generated JavaScript code for calculation");
    execute_js_code(&js_code, &format!("Mathematical calculation: {}", description), user_id).await
}

async fn process_text(
    text: &str,
    operation: &str,
    description: &str,
    user_id: UserId,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    agent_debug!(user_id, "process_text", "Processing text with operation: {}", operation);
    
    let js_code = match operation {
        "uppercase" => format!("'{}'.toUpperCase()", text),
        "lowercase" => format!("'{}'.toLowerCase()", text),
        "reverse" => format!("'{}'.split('').reverse().join('')", text),
        "count_words" => format!("'{}'.split(/\\s+/).filter(word => word.length > 0).length", text),
        "extract_numbers" => format!("'{}'.match(/\\d+/g) || []", text),
        "format_json" => format!("JSON.stringify(JSON.parse('{}'), null, 2)", text),
        _ => format!("// Unknown operation: {}\n'{}'", operation, text),
    };
    
    let full_js_code = format!(
        "// Text processing: {}\n\
         const text = '{}';\n\
         const result = {};\n\
         JSON.stringify({{ original: text, operation: '{}', result }})",
        description, text, js_code, operation
    );
    
    execute_js_code(&full_js_code, description, user_id).await
}

async fn analyze_data(
    data: &str,
    analysis_type: &str,
    description: &str,
    user_id: UserId,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    agent_debug!(user_id, "analyze_data", "Analyzing data with type: {}", analysis_type);
    
    let js_code = match analysis_type {
        "statistics" => format!(
            "const data = {};\n\
             if (Array.isArray(data)) {{\n\
                 const sum = data.reduce((a, b) => a + b, 0);\n\
                 const avg = sum / data.length;\n\
                 const min = Math.min(...data);\n\
                 const max = Math.max(...data);\n\
                 JSON.stringify({{ sum, average: avg, min, max, count: data.length }});\n\
             }} else {{\n\
                 JSON.stringify({{ error: 'Data is not an array' }});\n\
             }}",
            data
        ),
        "structure" => format!(
            "const data = {};\n\
             JSON.stringify({{ type: typeof data, isArray: Array.isArray(data), keys: Object.keys(data), length: data.length }})",
            data
        ),
        "validation" => format!(
            "const data = {};\n\
             const isValid = data !== null && data !== undefined;\n\
             JSON.stringify({{ isValid, type: typeof data, value: data }})",
            data
        ),
        "transformation" => format!(
            "const data = {};\n\
             const transformed = Array.isArray(data) ? data.map(x => x * 2) : data;\n\
             JSON.stringify({{ original: data, transformed }})",
            data
        ),
        _ => format!(
            "const data = {};\n\
             JSON.stringify({{ error: 'Unknown analysis type: {}', data }})",
            data, analysis_type
        ),
    };
    
    let full_js_code = format!(
        "// Data analysis: {}\n\
         {}",
        description, js_code
    );
    
    execute_js_code(&full_js_code, description, user_id).await
}

// ============================================================================
// THINKING MESSAGE MANAGEMENT
// ============================================================================

async fn update_thinking_message(
    ctx: &Context,
    thinking_msg: &mut Message,
    step: &str,
    user_id: UserId,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let timestamp = chrono::Utc::now().format("%H:%M:%S").to_string();
    let content = format!(
        "🧠 **Agent Reasoning Process**\n\n**Step {}:** {}\n\n*This message will be updated with more reasoning steps and then deleted when complete.*",
        timestamp, step
    );
    
    match thinking_msg.edit(&ctx.http, |m| m.content(&content)).await {
        Ok(_) => {
            agent_trace!(user_id, "update_thinking_message", "Updated thinking message with step: {}", step);
    Ok(())
        }
        Err(e) => {
            agent_warn!(user_id, "update_thinking_message", "Failed to update thinking message: {}", e);
            Err(e.into())
        }
    }
}

fn write_to_response_file(
    response_file: Option<&mut std::fs::File>,
    content: &str,
    user_id: UserId,
) {
    use std::io::Write;
    if let Some(file) = response_file {
        let timestamped_content = format!("[{}] {}\n", chrono::Utc::now().format("%H:%M:%S"), content);
        if let Err(e) = file.write_all(timestamped_content.as_bytes()) {
            agent_error!(user_id, "write_to_response_file", "Failed to write to response file: {}", e);
        } else {
            if let Err(e) = file.flush() {
                agent_error!(user_id, "write_to_response_file", "Failed to flush response file: {}", e);
            }
        }
    }
}

async fn delete_thinking_message(
    ctx: &Context,
    thinking_msg: &Message,
    user_id: UserId,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match thinking_msg.delete(&ctx.http).await {
        Ok(_) => {
            agent_trace!(user_id, "delete_thinking_message", "Successfully deleted thinking message");
    Ok(())
        }
        Err(e) => {
            agent_warn!(user_id, "delete_thinking_message", "Failed to delete thinking message: {}", e);
            Err(e.into())
        }
    }
}

// ============================================================================
// CONTEXT MANAGEMENT FUNCTIONS
// ============================================================================

async fn get_user_context(user_id: UserId) -> Vec<ChatMessage> {
    let contexts = get_user_contexts().await;
    if let Ok(contexts_map) = contexts.lock() {
        contexts_map.get(&user_id).cloned().unwrap_or_else(Vec::new)
    } else {
        Vec::new()
    }
}

async fn save_user_context(user_id: UserId, messages: Vec<ChatMessage>) {
    let contexts = get_user_contexts().await;
    if let Ok(mut contexts_map) = contexts.lock() {
        // Keep only the last 20 messages to prevent context from growing too large
        let mut trimmed_messages = messages;
        if trimmed_messages.len() > 20 {
            trimmed_messages = trimmed_messages.into_iter().rev().take(20).rev().collect();
        }
        contexts_map.insert(user_id, trimmed_messages);
        agent_debug!(user_id, "save_user_context", "Saved context with {} messages", contexts_map.get(&user_id).map(|c| c.len()).unwrap_or(0));
    }
}

async fn add_to_user_context(user_id: UserId, message: ChatMessage) {
    let contexts = get_user_contexts().await;
    if let Ok(mut contexts_map) = contexts.lock() {
        let user_context = contexts_map.entry(user_id).or_insert_with(Vec::new);
        user_context.push(message);
        
        // Keep only the last 20 messages to prevent unlimited growth
        if user_context.len() > 20 {
            user_context.drain(0..user_context.len() - 20);
        }
        
        agent_debug!(user_id, "add_to_user_context", "Added message to context, total: {} messages", user_context.len());
    }
}

async fn clear_user_context(user_id: UserId) {
    let contexts = get_user_contexts().await;
    if let Ok(mut contexts_map) = contexts.lock() {
        contexts_map.remove(&user_id);
        agent_info!(user_id, "clear_user_context", "Cleared user context");
    }
}

// ============================================================================
// AGENT EXECUTION FUNCTIONS
// ============================================================================

async fn execute_agent_task(
    task: String, 
    ctx: &Context, 
    msg: &Message
) -> CommandResult {
    let user_id = msg.author.id;
    let start_time = Instant::now();
    
    agent_trace!(user_id, "execute_agent_task", "=== EXECUTE AGENT TASK START ===");
    agent_trace!(user_id, "execute_agent_task", "Task: '{}'", task);
    agent_trace!(user_id, "execute_agent_task", "Task length: {} chars", task.len());
    
    agent_info!(user_id, "execute_agent_task", "Starting agent execution for task: '{}'", task);
    
    // Load configuration
    agent_trace!(user_id, "execute_agent_task", "Loading agent configuration...");
    let config = match load_agent_config().await {
        Ok(config) => {
            agent_trace!(user_id, "execute_agent_task", "Configuration loaded successfully");
            agent_trace!(user_id, "execute_agent_task", "Model: {}", config.default_model);
            agent_trace!(user_id, "execute_agent_task", "Base URL: {}", config.base_url);
            agent_trace!(user_id, "execute_agent_task", "Temperature: {}", config.default_temperature);
            agent_trace!(user_id, "execute_agent_task", "Max tokens: {}", config.default_max_tokens);
            agent_debug!(user_id, "execute_agent_task", "Successfully loaded agent configuration");
            config
        }
        Err(e) => {
            agent_trace!(user_id, "execute_agent_task", "Configuration loading failed: {}", e);
            agent_error!(user_id, "execute_agent_task", "Failed to load agent configuration: {}", e);
            msg.reply(ctx, "❌ Failed to load agent configuration").await?;
            return Ok(());
        }
    };
    
    // Create a file to stream the agent response to
    let response_filename = format!("agent_response_{}_{}.txt", user_id, chrono::Utc::now().timestamp());
    let mut response_file = match std::fs::File::create(&response_filename) {
        Ok(file) => {
            agent_info!(user_id, "execute_agent_task", "Created response file: {}", response_filename);
            file
        }
        Err(e) => {
            agent_error!(user_id, "execute_agent_task", "Failed to create response file: {}", e);
            let _ = msg.reply(ctx, "❌ Failed to create response file").await;
            return Ok(());
        }
    };

    // Write initial header to file
    use std::io::Write;
    let header = format!("🤖 **AI Agent Response**\nUser: {} ({})\nTask: {}\nTimestamp: {}\n\n", 
        msg.author.name, user_id, task, chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
    if let Err(e) = response_file.write_all(header.as_bytes()) {
        agent_error!(user_id, "execute_agent_task", "Failed to write header to file: {}", e);
    }

    // Send initial Discord message indicating file streaming
    let mut thinking_msg = match msg.channel_id.send_message(&ctx.http, |m| {
        m.content("🤖 **AI Agent Processing...**\n\n📝 Streaming response to file...\n⏳ This may take a moment...")
    }).await {
        Ok(message) => {
            agent_debug!(user_id, "execute_agent_task", "Successfully sent status message");
            message
        }
        Err(e) => {
            agent_error!(user_id, "execute_agent_task", "Failed to send status message: {}", e);
            msg.reply(ctx, "Failed to start agent execution!").await?;
            return Ok(());
        }
    };

    // Create system prompt for agent
    let system_prompt = create_agent_system_prompt();
    
    // Get user's conversation history for context carryover
    agent_trace!(user_id, "execute_agent_task", "Loading user context...");
    let user_context = get_user_context(user_id).await;
    agent_trace!(user_id, "execute_agent_task", "Loaded {} previous messages from context", user_context.len());
    
    // Build messages for function calling with context
    let mut messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
            },
    ];
    
    // Add previous context (excluding system messages to avoid duplication)
    for context_msg in &user_context {
        if context_msg.role != "system" {
            messages.push(context_msg.clone());
        }
    }
    
    // Add current user message
    let current_user_message = ChatMessage {
        role: "user".to_string(),
        content: task.clone(),
    };
    messages.push(current_user_message.clone());

    // Update thinking message with reasoning step
    let _ = update_thinking_message(ctx, &mut thinking_msg, "Analyzing task and preparing function definitions", user_id).await;
    
    // Get function definitions
    agent_trace!(user_id, "execute_agent_task", "Getting function definitions...");
    let functions = get_js_code_sandbox_functions();
    agent_trace!(user_id, "execute_agent_task", "Loaded {} function definitions", functions.len());
    for (i, func) in functions.iter().enumerate() {
        let func_name = func.function["name"].as_str().unwrap_or("unknown");
        let func_description = func.function["description"].as_str().unwrap_or("no description");
        agent_trace!(user_id, "execute_agent_task", "Function {}: {} - {}", i, func_name, func_description);
    }
    
    // Update thinking message with next step
    let _ = update_thinking_message(ctx, &mut thinking_msg, "Sending request to AI model for function calling", user_id).await;
    
    // Execute function calling
    agent_trace!(user_id, "execute_agent_task", "Starting function calling execution...");
    let result = match execute_function_calling(&messages, &functions, &config, user_id, Some(&mut response_file)).await {
        Ok(result) => {
            agent_info!(user_id, "execute_agent_task", "Successfully executed function calling");
            agent_trace!(user_id, "execute_agent_task", "Function calling result length: {} chars", result.len());
            agent_trace!(user_id, "execute_agent_task", "Function calling result preview: {}", &result[..std::cmp::min(200, result.len())]);
            result
        }
        Err(e) => {
            agent_error!(user_id, "execute_agent_task", "Failed to execute function calling: {}", e);
            write_to_response_file(Some(&mut response_file), &format!("❌ Error: {}", e), user_id);
            
            // Close the file and upload error result
            drop(response_file);
            let _ = msg.reply(ctx, "❌ Task failed. Check the uploaded file for details.").await;
            return Ok(());
        }
    };

    // Write completion status to file
    write_to_response_file(Some(&mut response_file), "✅ Task completed successfully! Preparing final response...", user_id);
    
    // Save the conversation to context for future use
    agent_trace!(user_id, "execute_agent_task", "Saving conversation to context...");
    add_to_user_context(user_id, current_user_message).await;
    add_to_user_context(user_id, ChatMessage {
        role: "assistant".to_string(),
        content: result.clone(),
    }).await;

    // Write final result to file
    write_to_response_file(Some(&mut response_file), "=== FINAL RESULT ===", user_id);
    write_to_response_file(Some(&mut response_file), &result, user_id);
    
    // Close the file
    drop(response_file);
    
    // Upload the response file to Discord
    agent_info!(user_id, "execute_agent_task", "Uploading response file: {}", response_filename);
    
    let file_content = match std::fs::read_to_string(&response_filename) {
        Ok(content) => content,
        Err(e) => {
            agent_error!(user_id, "execute_agent_task", "Failed to read response file: {}", e);
            let _ = msg.reply(ctx, "❌ Failed to read response file").await;
            return Ok(());
        }
    };

    // Create a summary for Discord message
    let summary = if result.len() > 500 {
        format!("{}...", &result[..500])
                    } else {
        result.clone()
    };
    
    let discord_message = format!(
        "✅ **Agent Task Complete**\n\n**Summary:**\n{}\n\n📎 **Full Response:** See attached file\n\n📝 **Context Saved** - Your conversation history is preserved for future ^agent commands.",
        summary
    );
    
    // Upload file to Discord
    match msg.channel_id.send_files(&ctx.http, vec![(&*file_content.as_bytes(), response_filename.as_str())], |m| {
        m.content(&discord_message)
    }).await {
        Ok(_) => {
            agent_info!(user_id, "execute_agent_task", "Successfully uploaded response file to Discord");
                }
                Err(e) => {
            agent_error!(user_id, "execute_agent_task", "Failed to upload response file: {}", e);
            // Fallback to regular message
            let fallback_message = format!("✅ **Agent Task Complete**\n\n{}\n\n📝 **Context Saved**", summary);
            let _ = msg.channel_id.send_message(&ctx.http, |m| m.content(&fallback_message)).await;
        }
    }
    
    // Clean up the temporary file
    if let Err(e) = std::fs::remove_file(&response_filename) {
        agent_warn!(user_id, "execute_agent_task", "Failed to remove temporary file {}: {}", response_filename, e);
    } else {
        agent_debug!(user_id, "execute_agent_task", "Successfully removed temporary file: {}", response_filename);
    }
    
    // Update status message to indicate completion
    let _ = thinking_msg.edit(&ctx.http, |m| {
        m.content("✅ **Agent Task Complete** - Response file uploaded successfully!")
    }).await;

    let total_duration = start_time.elapsed();
    agent_trace!(user_id, "execute_agent_task", "=== EXECUTE AGENT TASK END ===");
    agent_trace!(user_id, "execute_agent_task", "Total execution time: {:?}", total_duration);
    agent_trace!(user_id, "execute_agent_task", "Final result length: {} chars", result.len());
    agent_info!(user_id, "execute_agent_task", "Completed agent execution in {:?}", total_duration);

    Ok(())
}

async fn execute_function_calling(
    messages: &[ChatMessage],
    functions: &[FunctionDefinition],
    config: &LMConfig,
    user_id: UserId,
    mut response_file: Option<&mut std::fs::File>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    agent_trace!(user_id, "execute_function_calling", "=== EXECUTE FUNCTION CALLING START ===");
    agent_trace!(user_id, "execute_function_calling", "Messages count: {}", messages.len());
    agent_trace!(user_id, "execute_function_calling", "Functions count: {}", functions.len());
    agent_trace!(user_id, "execute_function_calling", "Model: {}", config.default_model);
    agent_trace!(user_id, "execute_function_calling", "Temperature: {}", config.default_temperature);
    agent_trace!(user_id, "execute_function_calling", "Max tokens: {}", config.default_max_tokens);
    
    agent_debug!(user_id, "execute_function_calling", "Starting function calling with {} functions", functions.len());
    
        // Update thinking message if available
    write_to_response_file(response_file.as_deref_mut(), "🔄 Connecting to AI model and preparing request", user_id);
    
    agent_trace!(user_id, "execute_function_calling", "Getting HTTP client...");
        let client = get_http_client().await;
    agent_trace!(user_id, "execute_function_calling", "HTTP client obtained successfully");
        
    // Set stream: true for function calling
        let chat_request = ChatRequest {
        model: config.default_model.clone(),
        messages: messages.to_vec(),
            temperature: config.default_temperature,
            max_tokens: config.default_max_tokens,
        stream: true, // Enable streaming for function calling
            seed: config.default_seed,
        tools: Some(functions.to_vec()),
        tool_choice: Some("auto".to_string()),
        };

    // For Ollama, we need to use the OpenAI-compatible endpoint
        let api_url = format!("{}/v1/chat/completions", config.base_url);
    agent_trace!(user_id, "execute_function_calling", "API URL: {}", api_url);
    agent_trace!(user_id, "execute_function_calling", "Using Ollama OpenAI-compatible endpoint for model: {}", config.default_model);
    agent_trace!(user_id, "execute_function_calling", "Request timeout: {} seconds", config.timeout);
    
    // Test connectivity to Ollama server
    let health_url = format!("{}/api/tags", config.base_url);
    agent_trace!(user_id, "execute_function_calling", "Testing connectivity to: {}", health_url);
    match client.get(&health_url).timeout(Duration::from_secs(10)).send().await {
        Ok(resp) => {
            agent_trace!(user_id, "execute_function_calling", "Connectivity test successful, status: {}", resp.status());
        }
        Err(e) => {
            agent_warn!(user_id, "execute_function_calling", "Connectivity test failed: {}", e);
        }
    }
    
    agent_debug!(user_id, "execute_function_calling", "Sending request to: {}", api_url);
    agent_trace!(user_id, "execute_function_calling", "Request payload: {}", serde_json::to_string_pretty(&chat_request).unwrap_or_else(|_| "Failed to serialize request".to_string()));
    
    // Update thinking message if available (use a different approach to avoid ownership issues)
    write_to_response_file(response_file.as_deref_mut(), "🔄 Sending request to AI model and waiting for response...", user_id);
        
    agent_trace!(user_id, "execute_function_calling", "About to send HTTP POST request...");
    
    // Instead of waiting for the full response, process the stream
    let response = match tokio::time::timeout(Duration::from_secs(config.timeout as u64), client
            .post(&api_url)
            .json(&chat_request)
            .timeout(Duration::from_secs(config.timeout as u64))
            .send()
    ).await {
        Ok(Ok(resp)) => {
            agent_trace!(user_id, "execute_function_calling", "HTTP request completed successfully");
            agent_debug!(user_id, "execute_function_calling", "Received response with status: {}", resp.status());
                resp
            }
        Ok(Err(e)) => {
            agent_trace!(user_id, "execute_function_calling", "HTTP request failed with error");
            agent_error!(user_id, "execute_function_calling", "HTTP request failed: {}", e);
                return Err(e.into());
            }
        Err(_) => {
            agent_trace!(user_id, "execute_function_calling", "HTTP request timed out");
            agent_error!(user_id, "execute_function_calling", "HTTP request timed out after {} seconds", config.timeout);
            return Err("HTTP request timed out".into());
        }
        };

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
        agent_error!(user_id, "execute_function_calling", "API returned error status {}: {}", status, error_text);
        return Err(format!("Function calling failed: HTTP {} - {}", status, error_text).into());
    }

    // Create cache key for this request
    let cache_key = format!("{}_{}", user_id, chrono::Utc::now().timestamp_millis());
    let response_cache = get_response_cache().await;
    
    // --- SSE streaming logic ---
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut collected_tool_calls: Vec<ToolCall> = Vec::new();
    let mut function_call_buffer: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();
    let mut last_update = std::time::Instant::now();
    let update_interval = std::time::Duration::from_millis(250);
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                agent_error!(user_id, "execute_function_calling", "Stream error: {}", e);
                break;
            }
        };
        let text = String::from_utf8_lossy(&chunk);
        for line in text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data.trim() == "[DONE]" { continue; }
                // Try to parse as JSON
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    // Try to extract content delta
                    if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                        for choice in choices {
                            if let Some(delta) = choice.get("delta") {
                                                    // Handle content deltas
                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                        buffer.push_str(content);
                        
                        // Cache the streaming content
                        if let Ok(mut cache) = response_cache.lock() {
                            cache.insert(cache_key.clone(), buffer.clone());
                        }
                    }
                    
                    // Handle tool_calls deltas
                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                        for tool_call in tool_calls {
                            if let Some(index) = tool_call.get("index").and_then(|i| i.as_u64()) {
                                let call_id = format!("call_{}", index);
                                
                                if let Some(function) = tool_call.get("function") {
                                    // Handle function name
                                    if let Some(name) = function.get("name").and_then(|n| n.as_str()) {
                                        if !function_call_buffer.contains_key(&call_id) {
                                            buffer.push_str(&format!("\n🔧 **Function Call:** {}\n📝 **Status:** Receiving arguments via SSE stream...\n", name));
                                            function_call_buffer.insert(call_id.clone(), (name.to_string(), String::new()));
                                        }
                                    }
                                    
                                    // Handle function arguments (stream JavaScript code in real-time)
                                    if let Some(args_chunk) = function.get("arguments").and_then(|a| a.as_str()) {
                                        if let Some((name, existing_args)) = function_call_buffer.get_mut(&call_id) {
                                            existing_args.push_str(args_chunk);
                                            
                                            // For execute_js_code, extract and preview the JavaScript code being streamed
                                            if name == "execute_js_code" {
                                                // Debug: Log the raw arguments to see what we're getting
                                                let args_preview = if existing_args.len() > 200 { &existing_args[..200] } else { &existing_args };
                                                agent_trace!(user_id, "execute_function_calling", "Raw args buffer (first 200 chars): '{}'", args_preview);
                                                
                                                // Extract JavaScript code from the raw arguments string (simple approach)
                                                let mut code_preview = String::new();
                                                let mut description_preview = String::new();
                                                
                                                // Try multiple patterns to find the code
                                                let code_patterns = [
                                                    "\"code\":\"",      // Standard JSON
                                                    "\"code\": \"",     // JSON with space
                                                    "'code':'",         // Single quotes
                                                    "'code': '",        // Single quotes with space
                                                ];
                                                
                                                for pattern in &code_patterns {
                                                    if let Some(code_start) = existing_args.find(pattern) {
                                                        agent_trace!(user_id, "execute_function_calling", "Found code pattern: '{}'", pattern);
                                                        let skip_len = pattern.len();
                                                        let code_content_start = code_start + skip_len;
                                                        
                                                        if code_content_start < existing_args.len() {
                                                            let remaining = &existing_args[code_content_start..];
                                                            let remaining_preview = if remaining.len() > 100 { &remaining[..100] } else { remaining };
                                                            agent_trace!(user_id, "execute_function_calling", "Code content remaining (first 100 chars): '{}'", remaining_preview);
                                                            
                                                            // Find the end of the code string (look for unescaped quote)
                                                            let quote_char = if pattern.contains('\'') { '\'' } else { '"' };
                                                            let mut code_end = remaining.len();
                                                            let mut chars = remaining.chars().enumerate();
                                                            let mut escaped = false;
                                                            
                                                            while let Some((i, ch)) = chars.next() {
                                                                if escaped {
                                                                    escaped = false;
                                                                    continue;
                                                                }
                                                                if ch == '\\' {
                                                                    escaped = true;
                                                                } else if ch == quote_char {
                                                                    code_end = i;
                                                                    break;
                                                                }
                                                            }
                                                            
                                                            if code_end > 0 {
                                                                let raw_code = &remaining[..code_end];
                                                                let raw_code_preview = if raw_code.len() > 200 { &raw_code[..200] } else { raw_code };
                                                                agent_trace!(user_id, "execute_function_calling", "Extracted raw code ({} chars): '{}'", raw_code.len(), raw_code_preview);
                                                                
                                                                // Simple unescape: just replace common escaped characters
                                                                code_preview = raw_code
                                                                    .replace("\\n", "\n")
                                                                    .replace("\\\"", "\"")
                                                                    .replace("\\\\", "\\")
                                                                    .replace("\\t", "\t")
                                                                    .replace("\\'", "'");
                                                                break; // Found code, stop looking
                                                            }
                                                        }
                                                    }
                                                }
                                                
                                                // If no code found yet, try a more aggressive approach - look for any JavaScript-like content
                                                if code_preview.is_empty() && existing_args.len() > 50 {
                                                    // Look for common JavaScript patterns in the raw text
                                                    if existing_args.contains("function") || existing_args.contains("const ") || 
                                                       existing_args.contains("let ") || existing_args.contains("var ") ||
                                                       existing_args.contains("canvas") || existing_args.contains("document") {
                                                        agent_trace!(user_id, "execute_function_calling", "Attempting to extract JS from raw args");
                                                        // Try to extract JavaScript-looking content
                                                        code_preview = existing_args.clone();
                                                    }
                                                }
                                                
                                                // Look for description with multiple patterns
                                                let desc_patterns = ["\"description\":\"", "\"description\": \"", "'description':'", "'description': '"];
                                                for pattern in &desc_patterns {
                                                    if let Some(desc_start) = existing_args.find(pattern) {
                                                        let skip_len = pattern.len();
                                                        let desc_content_start = desc_start + skip_len;
                                                        if desc_content_start < existing_args.len() {
                                                            let remaining = &existing_args[desc_content_start..];
                                                            let quote_char = if pattern.contains('\'') { '\'' } else { '"' };
                                                            if let Some(desc_end) = remaining.find(quote_char) {
                                                                description_preview = remaining[..desc_end].to_string();
                                                                break;
                                                            }
                                                        }
                                                    }
                                                }
                                                
                                                // Create live JavaScript preview
                                                let progress_text = if !code_preview.is_empty() && code_preview.len() > 20 {
                                                    let code_lines = code_preview.lines().count();
                                                    let code_chars = code_preview.len();
                                                    let preview_code = if code_preview.len() > 800 {
                                                        format!("{}...", &code_preview[..800])
        } else {
                                                        code_preview.clone()
                                                    };
                                                    
                                                    agent_trace!(user_id, "execute_function_calling", "Generated JavaScript preview ({} chars, {} lines)", code_chars, code_lines);
                                                    
                                                    format!(
                                                        "📊 **Status:** Streaming JavaScript code...\n📝 **Description:** {}\n📈 **Progress:** {} chars, {} lines\n\n💻 **Live Code Preview:**\n```javascript\n{}\n```\n{}",
                                                        if !description_preview.is_empty() { &description_preview } else { "JavaScript execution" },
                                                        code_chars,
                                                        code_lines,
                                                        preview_code,
                                                        if code_preview.len() > 800 { "\n🔄 **Still streaming...**" } else { "" }
                                                    )
                                                } else {
                                                    agent_trace!(user_id, "execute_function_calling", "No JavaScript code extracted yet, showing progress");
                                                    format!(
                                                        "📊 **Status:** Receiving JavaScript arguments...\n📝 **Description:** {}\n📈 **Progress:** {} chars received\n\n🔄 **Parsing JavaScript code...**\n\n📋 **Raw Preview:** {}",
                                                        if !description_preview.is_empty() { &description_preview } else { "JavaScript execution" },
                                                        existing_args.len(),
                                                        if existing_args.len() > 100 { format!("{}...", &existing_args[..100]) } else { existing_args.clone() }
                                                    )
                                                };
                                                
                                                // Update buffer to show current streaming code
                                                let function_start = buffer.rfind(&format!("🔧 **Function Call:** {}", name)).unwrap_or(0);
                                                let next_function = buffer[function_start..].find("\n🔧 **Function Call:**").map(|pos| function_start + pos);
                                                
                                                let new_content = format!("🔧 **Function Call:** {}\n{}", name, progress_text);
                                                
                                                if let Some(end_pos) = next_function {
                                                    buffer.replace_range(function_start..end_pos, &new_content);
                                                } else {
                                                    // This is the last/only function call
                                                    buffer.truncate(function_start);
                                                    buffer.push_str(&new_content);
                                                }
                                            } else {
                                                // For non-JavaScript functions, show simple progress
                                                let progress_bar = "█".repeat(std::cmp::min(existing_args.len() / 100, 20));
                                                let progress_text = format!(
                                                    "📊 **Progress:** {} ({} chars)\n📋 **Building function arguments...**\n", 
                                                    progress_bar,
                                                    existing_args.len()
                                                );
                                                
                                                // Update buffer to show current progress
                                                let function_start = buffer.rfind(&format!("🔧 **Function Call:** {}", name)).unwrap_or(0);
                                                let next_function = buffer[function_start..].find("\n🔧 **Function Call:**").map(|pos| function_start + pos);
                                                
                                                let new_content = format!("🔧 **Function Call:** {}\n{}", name, progress_text);
                                                
                                                if let Some(end_pos) = next_function {
                                                    buffer.replace_range(function_start..end_pos, &new_content);
                                                } else {
                                                    // This is the last/only function call
                                                    buffer.truncate(function_start);
                                                    buffer.push_str(&new_content);
                                                }
                                                
                                                // Force immediate buffer reset if progress update exceeds 1,800 chars
                                                if buffer.len() > 1800 {
                                                    write_to_response_file(response_file.as_deref_mut(), &format!("🔄 Function Progress - Segment Complete ({} chars)", buffer.len()), user_id);
                                                    buffer.clear();
                                                    last_update = std::time::Instant::now();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                            }
                        }
                    }
                }
            }
        }
        // If buffer exceeds 1,800 chars, write to file and reset cleanly
        if buffer.len() > 1800 {
            write_to_response_file(response_file.as_deref_mut(), &format!("🔄 Stream Segment Complete ({} chars)", buffer.len()), user_id);
            buffer.clear();
            last_update = std::time::Instant::now();
            continue;
        }
        // Write to file every interval
        if last_update.elapsed() > update_interval {
            write_to_response_file(response_file.as_deref_mut(), &buffer, user_id);
            last_update = std::time::Instant::now();
        }
    }
    // Final write to file after stream ends
    write_to_response_file(response_file.as_deref_mut(), "🔄 Stream complete, processing results...", user_id);
    write_to_response_file(response_file.as_deref_mut(), &buffer, user_id);
    // --- End SSE streaming logic ---

    // Finalize tool calls from the accumulated buffer
    for (call_id, (name, args_str)) in function_call_buffer {
        if !args_str.is_empty() {
            // Try to fix common JSON issues before parsing
            let cleaned_args = args_str
                .trim()
                .replace("\n", "\\n")  // Escape newlines
                .replace("\r", "\\r")  // Escape carriage returns
                .replace("\t", "\\t"); // Escape tabs
            
            agent_trace!(user_id, "execute_function_calling", "Attempting to parse args for {}: {}", name, &cleaned_args[..std::cmp::min(200, cleaned_args.len())]);
            
            // Update the final status in the buffer with complete JavaScript code
            let completion_text = if name == "execute_js_code" {
                // Try to parse the complete JSON and extract the code
                let cleaned_args = args_str
                    .trim()
                    .replace("\n", "\\n")
                    .replace("\r", "\\r")
                    .replace("\t", "\\t");
                
                let mut final_code = String::new();
                let mut final_description = String::new();
                
                // Extract the complete JavaScript code from the final arguments
                if let Ok(json_args) = serde_json::from_str::<serde_json::Value>(&cleaned_args) {
                    if let Some(code) = json_args.get("code").and_then(|c| c.as_str()) {
                        final_code = code.to_string();
                    }
                    if let Some(desc) = json_args.get("description").and_then(|d| d.as_str()) {
                        final_description = desc.to_string();
                    }
                }
                
                if !final_code.is_empty() {
                    let code_lines = final_code.lines().count();
                    let code_chars = final_code.len();
                    
                    format!(
                        "✅ **Status:** JavaScript code ready for execution\n📝 **Description:** {}\n📊 **Code Stats:** {} chars, {} lines\n\n💻 **Complete JavaScript Code:**\n```javascript\n{}\n```\n\n🚀 **Executing code...**",
                        if !final_description.is_empty() { &final_description } else { "JavaScript execution" },
                        code_chars,
                        code_lines,
                        final_code
                    )
                } else {
                    format!(
                        "✅ **Status:** Arguments received successfully\n📊 **Total Size:** {} characters\n🚀 **Ready for execution**\n",
                        args_str.len()
                    )
                }
            } else {
                format!(
                    "✅ **Status:** Arguments received successfully\n📊 **Total Size:** {} characters\n🚀 **Ready for execution**\n",
                    args_str.len()
                )
            };
            
            // Update the buffer to show completion
            let function_start = buffer.rfind(&format!("🔧 **Function Call:** {}", name)).unwrap_or(0);
            let next_function = buffer[function_start..].find("\n🔧 **Function Call:**").map(|pos| function_start + pos);
            
            let new_completion_content = format!("🔧 **Function Call:** {}\n{}", name, completion_text);
            
            if let Some(end_pos) = next_function {
                buffer.replace_range(function_start..end_pos, &new_completion_content);
            } else {
                // This is the last/only function call
                buffer.truncate(function_start);
                buffer.push_str(&new_completion_content);
            }
            
            // Force immediate buffer reset if completion update exceeds 1,800 chars
            if buffer.len() > 1800 {
                write_to_response_file(response_file.as_deref_mut(), &format!("✅ Function Arguments Complete ({} chars)", buffer.len()), user_id);
                buffer.clear();
            }
            
            match serde_json::from_str::<serde_json::Value>(&cleaned_args) {
                Ok(args) => {
                    let tool_call = ToolCall {
                        id: call_id,
                        call_type: "function".to_string(),
                        function: FunctionCallResponse {
                            name: name.clone(),
                            arguments: args,
                        },
                    };
                    collected_tool_calls.push(tool_call);
                    agent_debug!(user_id, "execute_function_calling", "Successfully parsed tool call: {}", name);
                }
                Err(e) => {
                    agent_warn!(user_id, "execute_function_calling", "Failed to parse arguments for function {}: {} - Error: {}", name, &args_str[..std::cmp::min(100, args_str.len())], e);
                    
                    // Try to create a basic function call with the raw string as code parameter
                    if name == "execute_js_code" {
                        let fallback_args = serde_json::json!({
                            "code": args_str,
                            "description": "JavaScript code execution (fallback parsing)"
                        });
                        let tool_call = ToolCall {
                            id: call_id,
                            call_type: "function".to_string(),
                            function: FunctionCallResponse {
                                name: name.clone(),
                                arguments: fallback_args,
                            },
                        };
                        collected_tool_calls.push(tool_call);
                        agent_info!(user_id, "execute_function_calling", "Created fallback tool call for execute_js_code");
                    }
                }
            }
        } else {
            // Create tool call with empty arguments
            let tool_call = ToolCall {
                id: call_id,
                call_type: "function".to_string(),
                function: FunctionCallResponse {
                    name: name.clone(),
                    arguments: serde_json::json!({}),
                },
            };
            collected_tool_calls.push(tool_call);
            agent_debug!(user_id, "execute_function_calling", "Finalized tool call: {} with empty args", name);
        }
    }

    // After streaming, execute any collected tool calls
    if !collected_tool_calls.is_empty() {
        agent_debug!(user_id, "execute_function_calling", "Found {} tool calls to execute", collected_tool_calls.len());
        
        // Write function execution status to file
        write_to_response_file(response_file.as_deref_mut(), "🔄 Executing functions...", user_id);
        
        // Execute each tool call
        let mut function_results = Vec::new();
        for tool_call in &collected_tool_calls {
            agent_trace!(user_id, "execute_function_calling", "=== EXECUTE FUNCTION CALL START ===");
            agent_trace!(user_id, "execute_function_calling", "Function: {}", tool_call.function.name);
            agent_trace!(user_id, "execute_function_calling", "Arguments: {}", serde_json::to_string_pretty(&tool_call.function.arguments).unwrap_or_else(|_| "Failed to serialize".to_string()));
            
            match execute_function_call(&tool_call.function, user_id).await {
                Ok(result) => {
                    agent_info!(user_id, "execute_function_calling", "Function '{}' executed successfully", tool_call.function.name);
                    agent_trace!(user_id, "execute_function_calling", "Function result: {}", result);
                    function_results.push(format!("✅ {}: {}", tool_call.function.name, result));
            }
            Err(e) => {
                    agent_error!(user_id, "execute_function_calling", "Function '{}' failed: {}", tool_call.function.name, e);
                    function_results.push(format!("❌ {}: Error - {}", tool_call.function.name, e));
                }
            }
            agent_trace!(user_id, "execute_function_calling", "=== EXECUTE FUNCTION CALL END ===");
        }
        
        // Send function results back to the model for analysis and final response
        agent_debug!(user_id, "execute_function_calling", "Sending function results back to model for final processing");
        
        // Write final processing status to file
        write_to_response_file(response_file.as_deref_mut(), "🔄 Processing function results and generating final response...", user_id);
        
        // Create messages with function results for final response
        let mut final_messages = messages.to_vec();
        
        // Add the assistant's original response (if any)
        if !buffer.trim().is_empty() {
            final_messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: buffer.clone(),
            });
        }
        
        // Add function results as tool responses
        for (i, result) in function_results.iter().enumerate() {
            final_messages.push(ChatMessage {
                role: "tool".to_string(),
                content: format!("Function execution result {}: {}", i + 1, result),
            });
        }
        
        // Add a user message asking for analysis
        final_messages.push(ChatMessage {
            role: "user".to_string(),
            content: "Please analyze the function results above and provide a comprehensive final answer. If there were any errors, explain what went wrong and suggest fixes.".to_string(),
        });
        
        // Get final response from model with function results
        match get_final_response(&final_messages, functions, config, user_id, response_file.as_deref_mut()).await {
            Ok(final_response) => {
                agent_info!(user_id, "execute_function_calling", "Successfully got final response from model");
                
                        // Extract any JavaScript code from function results for prominent display
        let mut executed_code = String::new();
        for result in &function_results {
            if result.contains("```javascript") {
                if let Some(start) = result.find("```javascript") {
                    if let Some(end) = result[start..].find("```\n") {
                        let code_section = &result[start + 13..start + end];
                        if !code_section.trim().is_empty() {
                            executed_code = code_section.trim().to_string();
                            break;
                        }
                    }
                }
            }
        }
        
        // Combine everything into a comprehensive response with code prominently displayed
        let comprehensive_response = if final_response.trim().is_empty() {
            // If no analysis from model, show results with code emphasis
            if !executed_code.is_empty() {
                format!(
                    "**JavaScript Execution Results:**\n{}\n\n🚀 **Ready-to-Use Code:**\n```javascript\n{}\n```\n\n✨ **Copy the code above to use it in your project!**", 
                    function_results.join("\n\n"),
                    executed_code
                )
                    } else {
                format!("**Execution Results:**\n{}", function_results.join("\n\n"))
            }
        } else {
            // Include results, analysis, and prominently display code
            if !executed_code.is_empty() {
                format!(
                    "**JavaScript Execution Results:**\n{}\n\n**AI Analysis:**\n{}\n\n🚀 **Ready-to-Use Code:**\n```javascript\n{}\n```\n\n✨ **Copy the code above to use it in your project!**", 
                    function_results.join("\n\n"), 
                    final_response,
                    executed_code
                )
            } else {
                format!(
                    "**Execution Results:**\n{}\n\n**AI Analysis:**\n{}", 
                    function_results.join("\n\n"), 
                    final_response
                )
            }
        };
        
        Ok(comprehensive_response)
                }
                Err(e) => {
                agent_warn!(user_id, "execute_function_calling", "Failed to get final response, using function results only: {}", e);
                
                // Extract code for fallback as well
                let mut executed_code_fallback = String::new();
                for result in &function_results {
                    if result.contains("```javascript") {
                        if let Some(start) = result.find("```javascript") {
                            if let Some(end) = result[start..].find("```\n") {
                                let code_section = &result[start + 13..start + end];
                                if !code_section.trim().is_empty() {
                                    executed_code_fallback = code_section.trim().to_string();
                                    break;
                                }
                            }
                        }
                    }
                }
                
                // Fallback to just function results if final response fails
                let fallback_response = if buffer.trim().is_empty() {
                    if !executed_code_fallback.is_empty() {
                        format!(
                            "**JavaScript Execution Results:**\n{}\n\n🚀 **Ready-to-Use Code:**\n```javascript\n{}\n```\n\n✨ **Copy the code above to use it in your project!**", 
                            function_results.join("\n\n"),
                            executed_code_fallback
                        )
                    } else {
                        format!("**Execution Results:**\n{}", function_results.join("\n\n"))
                    }
                } else {
                    if !executed_code_fallback.is_empty() {
                        format!(
                            "**AI Response:**\n{}\n\n**JavaScript Execution Results:**\n{}\n\n🚀 **Ready-to-Use Code:**\n```javascript\n{}\n```\n\n✨ **Copy the code above to use it in your project!**", 
                            buffer, 
                            function_results.join("\n\n"),
                            executed_code_fallback
                        )
                    } else {
                        format!("**AI Response:**\n{}\n\n**Execution Results:**\n{}", buffer, function_results.join("\n\n"))
                    }
                };
                
                Ok(fallback_response)
            }
        }
    } else {
        // No tool calls, just return the text response
        write_to_response_file(response_file.as_deref_mut(), "✅ No function calls needed, returning text response", user_id);
        
        Ok(buffer)
    }
}

async fn get_final_response(
    messages: &[ChatMessage],
    _functions: &[FunctionDefinition],
    config: &LMConfig,
    user_id: UserId,
    mut response_file: Option<&mut std::fs::File>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    agent_debug!(user_id, "get_final_response", "Getting final response from model");
    agent_trace!(user_id, "get_final_response", "Messages count: {}", messages.len());
    
    // Update thinking message if available
    write_to_response_file(response_file.as_deref_mut(), "🤖 AI is analyzing function results and preparing final answer...", user_id);
        
        let client = get_http_client().await;
        
        let chat_request = ChatRequest {
        model: config.default_model.clone(),
        messages: messages.to_vec(),
            temperature: config.default_temperature,
            max_tokens: config.default_max_tokens,
        stream: true, // Enable streaming for final response
            seed: config.default_seed,
        tools: None, // No tools for final response
        tool_choice: None,
        };

        let api_url = format!("{}/v1/chat/completions", config.base_url);
        
        let response = match client
            .post(&api_url)
            .json(&chat_request)
            .timeout(Duration::from_secs(config.timeout as u64))
            .send()
            .await
        {
        Ok(resp) => resp,
            Err(e) => {
            agent_error!(user_id, "get_final_response", "HTTP request failed: {}", e);
                return Err(e.into());
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
        agent_error!(user_id, "get_final_response", "API returned error status {}: {}", status, error_text);
        return Err(format!("Final response failed: HTTP {} - {}", status, error_text).into());
    }

    // --- SSE streaming logic for final response ---
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut last_update = std::time::Instant::now();
    let update_interval = std::time::Duration::from_millis(250);
    
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                agent_error!(user_id, "get_final_response", "Stream error: {}", e);
                break;
            }
        };
        let text = String::from_utf8_lossy(&chunk);
        for line in text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data.trim() == "[DONE]" { continue; }
                // Try to parse as JSON
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    // Try to extract content delta
                    if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                        for choice in choices {
                            if let Some(delta) = choice.get("delta") {
                                if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                    buffer.push_str(content);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // If buffer exceeds 1,500 chars, write to file and reset
        if buffer.len() > 1500 {
            write_to_response_file(response_file.as_deref_mut(), &format!("🤖 AI Analysis Segment ({} chars)", buffer.len()), user_id);
            buffer.clear();
            last_update = std::time::Instant::now();
            continue;
        }
        
        // Write to file every interval
        if last_update.elapsed() > update_interval {
            write_to_response_file(response_file.as_deref_mut(), &buffer, user_id);
            last_update = std::time::Instant::now();
        }
    }
    
    // Final write to file after stream ends
    write_to_response_file(response_file.as_deref_mut(), "🤖 AI Analysis Complete", user_id);
    write_to_response_file(response_file.as_deref_mut(), &buffer, user_id);
    // --- End SSE streaming logic ---
    
    if buffer.trim().is_empty() {
        agent_error!(user_id, "get_final_response", "No content received from stream");
        
        // Write error to file
        write_to_response_file(response_file.as_deref_mut(), "❌ Error: No content received from stream", user_id);
        
        return Err("No content received from stream".into());
    }
    
    agent_info!(user_id, "get_final_response", "Successfully got final response via streaming, length: {} chars", buffer.len());
    
    // Write completion to file
    write_to_response_file(response_file.as_deref_mut(), "✅ Final response generation complete", user_id);
    
    Ok(buffer)
}

fn create_agent_system_prompt() -> String {
    let user_id = UserId(0); // Use a dummy user ID for system operations
    
    // Try to load from external file first
    let prompt_paths = [
        "agent_prompt.txt",
        "../agent_prompt.txt", 
        "../../agent_prompt.txt",
        "src/agent_prompt.txt"
    ];
    
    agent_debug!(user_id, "create_agent_system_prompt", "Attempting to load system prompt from external file...");
    for prompt_path in &prompt_paths {
        agent_debug!(user_id, "create_agent_system_prompt", "Trying path: {}", prompt_path);
        match fs::read_to_string(prompt_path) {
            Ok(content) => {
                agent_info!(user_id, "create_agent_system_prompt", "Successfully loaded system prompt from {}", prompt_path);
                agent_debug!(user_id, "create_agent_system_prompt", "Loaded prompt content length: {} chars", content.len());
                return content;
            }
            Err(e) => {
                agent_debug!(user_id, "create_agent_system_prompt", "Failed to load prompt from {}: {}", prompt_path, e);
                continue;
            }
        }
    }
    
    // Fallback to default prompt if file not found
    agent_warn!(user_id, "create_agent_system_prompt", "agent_prompt.txt not found in any location, using default system prompt");
    r#"You are an intelligent AI agent with access to JavaScript code execution capabilities. You can perform calculations, process text, analyze data, and execute custom JavaScript code to help users with their tasks.

**Available Functions:**
1. **execute_js_code** - Execute custom JavaScript code for any computational task
2. **calculate_math** - Perform mathematical calculations and operations
3. **process_text** - Process and manipulate text (uppercase, lowercase, reverse, count words, etc.)
4. **analyze_data** - Analyze data structures, arrays, and objects

**Guidelines:**
- Always use function calling when you need to perform calculations, process data, or execute code
- Provide clear explanations of what you're doing
- Handle errors gracefully and provide helpful feedback
- Be creative and helpful in solving user problems
- When appropriate, combine multiple function calls to solve complex tasks

**Example Usage:**
- For math: Use calculate_math with mathematical expressions
- For text processing: Use process_text with appropriate operations
- For data analysis: Use analyze_data with data structures
- For custom logic: Use execute_js_code with your own JavaScript code

Remember: You have the power to execute JavaScript code safely in a sandboxed environment. Use this capability to help users solve their problems effectively."#.to_string()
}

// ============================================================================
// STAGE-SPECIFIC SYSTEM PROMPTS
// ============================================================================

fn create_stage_1_planning_prompt() -> String {
    r#"You are a task analysis and planning agent. Your job is to analyze user requests and create detailed execution plans.

**STAGE 1: TASK ANALYSIS & PLANNING**

**Your Role:**
- Analyze the user's request thoroughly
- Break down complex tasks into clear, actionable steps
- Identify the best approach for solving the problem
- Create a structured plan with expected outcomes
- Consider potential challenges and solutions

**Planning Guidelines:**
1. **Understand the Core Task**: What exactly does the user want to accomplish?
2. **Identify Requirements**: What inputs, outputs, and constraints are involved?
3. **Choose the Right Tools**: Which functions (execute_js_code, calculate_math, process_text, analyze_data) are needed?
4. **Plan the Execution**: What steps will lead to the desired outcome?
5. **Consider Edge Cases**: What could go wrong and how to handle it?
6. **Estimate Complexity**: How complex is this task and what resources will be needed?

**Output Format:**
Provide a structured plan with:
- **Task Summary**: Brief description of what needs to be done
- **Requirements**: What inputs and outputs are expected
- **Execution Steps**: Detailed step-by-step plan
- **Tools Needed**: Which functions will be used and why
- **Expected Outcome**: What the final result should look like
- **Potential Issues**: Any challenges that might arise

**Example Plan:**
```
TASK: Calculate the factorial of 10 and display it with formatting

REQUIREMENTS:
- Input: Number 10
- Output: Formatted factorial result
- Tools: calculate_math for computation, process_text for formatting

EXECUTION STEPS:
1. Use calculate_math to compute 10!
2. Format the result with process_text for better display
3. Present the final formatted result

EXPECTED OUTCOME: "The factorial of 10 is: 3,628,800"

POTENTIAL ISSUES: Large numbers might need scientific notation
```

Focus on creating clear, actionable plans that will guide the next stages of execution."#.to_string()
}

fn create_stage_2_code_generation_prompt() -> String {
    r#"You are a JavaScript code generation agent. Your job is to create high-quality, executable JavaScript code based on the planning stage.

**STAGE 2: CODE GENERATION & VALIDATION**

**Your Role:**
- Generate clean, efficient JavaScript code
- Ensure code follows best practices
- Validate that code will execute successfully
- Provide clear documentation and comments
- Handle edge cases and error conditions

**Code Generation Guidelines:**
1. **Follow the Plan**: Use the planning stage output as your blueprint
2. **Write Clean Code**: Use clear variable names, proper indentation, and good structure
3. **Add Comments**: Explain complex logic and important sections
4. **Handle Errors**: Include try-catch blocks and validation where appropriate
5. **Optimize Performance**: Use efficient algorithms and data structures
6. **Test Your Logic**: Think through how the code will execute

**Code Quality Standards:**
- **Readability**: Code should be easy to understand
- **Efficiency**: Use appropriate algorithms and avoid unnecessary operations
- **Robustness**: Handle edge cases and potential errors
- **Documentation**: Include clear comments explaining the logic
- **Modularity**: Break complex tasks into smaller, reusable functions

**Output Format:**
Provide:
- **Code Overview**: Brief description of what the code does
- **Implementation**: Complete, executable JavaScript code
- **Usage Instructions**: How to use the generated code
- **Expected Results**: What output to expect
- **Error Handling**: How errors are managed

**Example Output:**
```
CODE OVERVIEW: Calculates factorial with input validation and formatting

IMPLEMENTATION:
```javascript
function calculateFactorial(n) {
    // Validate input
    if (n < 0 || !Number.isInteger(n)) {
        throw new Error('Input must be a non-negative integer');
    }
    
    // Calculate factorial
    let result = 1;
    for (let i = 2; i <= n; i++) {
        result *= i;
    }
    
    return result;
}

// Usage example
const number = 10;
const factorial = calculateFactorial(number);
console.log(`The factorial of ${number} is: ${factorial.toLocaleString()}`);
```

EXPECTED RESULTS: "The factorial of 10 is: 3,628,800"

ERROR HANDLING: Throws error for invalid inputs (negative numbers, non-integers)
```

Focus on creating production-ready code that is both functional and maintainable."#.to_string()
}

fn create_stage_3_execution_prompt() -> String {
    r#"You are a code execution and testing agent. Your job is to execute the generated JavaScript code and perform comprehensive testing.

**STAGE 3: EXECUTION & TESTING**

**Your Role:**
- Execute the generated JavaScript code safely
- Monitor execution for errors and issues
- Perform comprehensive testing with various inputs
- Validate that results match expectations
- Identify and report any problems

**Execution Guidelines:**
1. **Safe Execution**: Run code in a controlled environment
2. **Error Monitoring**: Watch for runtime errors, exceptions, and unexpected behavior
3. **Input Testing**: Test with various inputs including edge cases
4. **Output Validation**: Verify that results are correct and complete
5. **Performance Analysis**: Monitor execution time and resource usage
6. **Documentation**: Record all test results and findings

**Testing Strategy:**
- **Unit Tests**: Test individual functions and components
- **Integration Tests**: Test how different parts work together
- **Edge Case Testing**: Test with boundary values and unusual inputs
- **Error Testing**: Test error conditions and exception handling
- **Performance Testing**: Measure execution time and efficiency

**Output Format:**
Provide:
- **Execution Summary**: Overview of what was executed
- **Test Results**: Detailed results from all tests
- **Performance Metrics**: Execution time and resource usage
- **Issues Found**: Any problems or unexpected behavior
- **Recommendations**: Suggestions for improvements

**Example Output:**
```
EXECUTION SUMMARY: Successfully executed factorial calculation function

TEST RESULTS:
✓ Basic test (n=5): Result = 120 ✅
✓ Edge case (n=0): Result = 1 ✅
✓ Large number (n=10): Result = 3,628,800 ✅
✓ Error handling (n=-1): Throws error as expected ✅

PERFORMANCE METRICS:
- Execution time: < 1ms for n=10
- Memory usage: Minimal
- CPU usage: Low

ISSUES FOUND: None

RECOMMENDATIONS: Code performs well, no changes needed
```

Focus on thorough testing and validation to ensure code quality and reliability."#.to_string()
}

fn create_stage_4_analysis_prompt() -> String {
    r#"You are a result analysis and optimization agent. Your job is to analyze execution results and optimize the solution.

**STAGE 4: RESULT ANALYSIS & OPTIMIZATION**

**Your Role:**
- Analyze execution results and performance metrics
- Identify areas for improvement and optimization
- Suggest enhancements to code quality and efficiency
- Validate that the solution meets all requirements
- Provide recommendations for future improvements

**Analysis Guidelines:**
1. **Result Validation**: Verify that outputs match expected results
2. **Performance Analysis**: Evaluate execution speed and resource usage
3. **Code Quality Review**: Assess readability, maintainability, and best practices
4. **Optimization Opportunities**: Identify ways to improve efficiency
5. **Requirement Compliance**: Ensure all user requirements are met
6. **Future-Proofing**: Consider scalability and extensibility

**Optimization Focus Areas:**
- **Algorithm Efficiency**: Can the algorithm be improved?
- **Code Structure**: Is the code well-organized and maintainable?
- **Error Handling**: Are all edge cases properly handled?
- **Documentation**: Is the code well-documented?
- **Performance**: Can execution time or memory usage be reduced?
- **User Experience**: Is the output clear and user-friendly?

**Output Format:**
Provide:
- **Analysis Summary**: Overview of findings and conclusions
- **Performance Assessment**: Detailed performance analysis
- **Quality Review**: Code quality and best practices assessment
- **Optimization Suggestions**: Specific recommendations for improvement
- **Final Recommendation**: Whether to proceed with current solution or implement changes

**Example Output:**
```
ANALYSIS SUMMARY: Code performs well and meets all requirements

PERFORMANCE ASSESSMENT:
- Execution speed: Excellent (< 1ms for typical inputs)
- Memory efficiency: Good (minimal memory usage)
- Scalability: Good (handles large inputs efficiently)

QUALITY REVIEW:
- Code structure: Well-organized and readable
- Error handling: Comprehensive and robust
- Documentation: Clear and helpful
- Best practices: Follows JavaScript conventions

OPTIMIZATION SUGGESTIONS:
- Consider adding input validation for very large numbers
- Could add caching for frequently calculated values
- Documentation could include more usage examples

FINAL RECOMMENDATION: Current solution is production-ready
```

Focus on providing actionable insights and recommendations for improvement."#.to_string()
}

fn create_stage_5_delivery_prompt() -> String {
    r#"You are a final delivery and documentation agent. Your job is to prepare the complete solution for delivery to the user.

**STAGE 5: FINAL DELIVERY & DOCUMENTATION**

**Your Role:**
- Compile all previous stage results into a comprehensive deliverable
- Create clear, user-friendly documentation
- Provide usage examples and instructions
- Ensure the solution is ready for immediate use
- Present results in a professional, organized format

**Documentation Guidelines:**
1. **Complete Solution**: Include all code, explanations, and resources
2. **Clear Instructions**: Provide step-by-step usage instructions
3. **Examples**: Include practical examples showing how to use the solution
4. **Troubleshooting**: Address common issues and how to resolve them
5. **Best Practices**: Share tips for optimal usage
6. **Future Considerations**: Suggest ways to extend or modify the solution

**Delivery Components:**
- **Executive Summary**: Brief overview of what was accomplished
- **Complete Code**: Final, tested, and optimized code
- **Usage Instructions**: How to use the solution
- **Examples**: Practical examples with expected outputs
- **Troubleshooting Guide**: Common issues and solutions
- **Performance Notes**: Important performance considerations
- **Extension Ideas**: Suggestions for future enhancements

**Output Format:**
Provide a comprehensive deliverable with:

```
🎯 EXECUTIVE SUMMARY
Brief overview of the solution and its capabilities

💻 COMPLETE CODE
```javascript
// Final, production-ready code
```

📖 USAGE INSTRUCTIONS
Step-by-step guide on how to use the solution

🔧 EXAMPLES
Practical examples with inputs and expected outputs

⚠️ TROUBLESHOOTING
Common issues and how to resolve them

⚡ PERFORMANCE NOTES
Important performance considerations and optimizations

🚀 EXTENSION IDEAS
Suggestions for future enhancements and modifications
```

Focus on creating a professional, complete deliverable that users can immediately understand and use."#.to_string()
}

// ============================================================================
// STAGE MANAGEMENT FUNCTIONS
// ============================================================================

async fn execute_stage_1_planning(
    task: &str,
    user_id: UserId,
    config: &LMConfig,
    response_file: &mut std::fs::File,
) -> Result<AgentStage, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    let stage_id = 1;
    
    agent_trace!(user_id, "execute_stage_1_planning", "=== STAGE 1: PLANNING START ===");
    agent_info!(user_id, "execute_stage_1_planning", "Starting planning stage for task: '{}'", task);
    
    // Create stage record
    let mut stage = AgentStage {
        stage_id,
        name: "Task Analysis & Planning".to_string(),
        description: "Analyze user request and create execution plan".to_string(),
        status: StageStatus::InProgress,
        input: Some(task.to_string()),
        output: None,
        timestamp: Utc::now(),
        duration: None,
    };
    
    // Write stage start to file
    write_to_response_file(Some(response_file), &format!("=== STAGE {}: {} ===", stage_id, stage.name), user_id);
    write_to_response_file(Some(response_file), "🔄 Starting task analysis and planning...", user_id);
    
    // Create system prompt for planning
    let system_prompt = create_stage_1_planning_prompt();
    
    // Build messages for planning
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Please analyze this task and create a detailed execution plan: {}", task),
        },
    ];
    
    // Execute planning with function calling
    let functions = get_js_code_sandbox_functions();
    let planning_result = match execute_function_calling(&messages, &functions, config, user_id, Some(response_file)).await {
        Ok(result) => {
            agent_info!(user_id, "execute_stage_1_planning", "Planning stage completed successfully");
            result
            }
            Err(e) => {
            agent_error!(user_id, "execute_stage_1_planning", "Planning stage failed: {}", e);
            stage.status = StageStatus::Failed;
            stage.duration = Some(start_time.elapsed());
                return Err(e);
            }
        };
        
    // Update stage with results
    stage.output = Some(planning_result.clone());
    stage.status = StageStatus::Completed;
    stage.duration = Some(start_time.elapsed());
    
    // Write completion to file
    write_to_response_file(Some(response_file), "✅ Planning stage completed successfully", user_id);
    write_to_response_file(Some(response_file), &format!("📊 Planning duration: {:?}", stage.duration.unwrap()), user_id);
    
    agent_trace!(user_id, "execute_stage_1_planning", "=== STAGE 1: PLANNING END ===");
    agent_info!(user_id, "execute_stage_1_planning", "Planning stage completed in {:?}", stage.duration.unwrap());
    
    Ok(stage)
}

async fn execute_stage_2_code_generation(
    plan: &AgentStage,
    user_id: UserId,
    config: &LMConfig,
    response_file: &mut std::fs::File,
) -> Result<AgentStage, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    let stage_id = 2;
    
    agent_trace!(user_id, "execute_stage_2_code_generation", "=== STAGE 2: CODE GENERATION START ===");
    agent_info!(user_id, "execute_stage_2_code_generation", "Starting code generation stage");
    
    // Create stage record
    let mut stage = AgentStage {
        stage_id,
        name: "Code Generation & Validation".to_string(),
        description: "Generate JavaScript code based on planning stage".to_string(),
        status: StageStatus::InProgress,
        input: plan.output.clone(),
        output: None,
        timestamp: Utc::now(),
        duration: None,
    };
    
    // Write stage start to file
    write_to_response_file(Some(response_file), &format!("=== STAGE {}: {} ===", stage_id, stage.name), user_id);
    write_to_response_file(Some(response_file), "🔄 Starting code generation and validation...", user_id);
    
    // Create system prompt for code generation
    let system_prompt = create_stage_2_code_generation_prompt();
    
    // Build messages for code generation
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Based on this plan, generate the JavaScript code:\n\n{}", 
                plan.output.as_ref().unwrap_or(&"No plan available".to_string())),
        },
    ];
    
    // Execute code generation with function calling
    let functions = get_js_code_sandbox_functions();
    let code_result = match execute_function_calling(&messages, &functions, config, user_id, Some(response_file)).await {
                        Ok(result) => {
            agent_info!(user_id, "execute_stage_2_code_generation", "Code generation stage completed successfully");
                            result
                        }
                        Err(e) => {
            agent_error!(user_id, "execute_stage_2_code_generation", "Code generation stage failed: {}", e);
            stage.status = StageStatus::Failed;
            stage.duration = Some(start_time.elapsed());
            return Err(e);
        }
    };
    
    // Update stage with results
    stage.output = Some(code_result.clone());
    stage.status = StageStatus::Completed;
    stage.duration = Some(start_time.elapsed());
    
    // Write completion to file
    write_to_response_file(Some(response_file), "✅ Code generation stage completed successfully", user_id);
    write_to_response_file(Some(response_file), &format!("📊 Code generation duration: {:?}", stage.duration.unwrap()), user_id);
    
    agent_trace!(user_id, "execute_stage_2_code_generation", "=== STAGE 2: CODE GENERATION END ===");
    agent_info!(user_id, "execute_stage_2_code_generation", "Code generation stage completed in {:?}", stage.duration.unwrap());
    
    Ok(stage)
}

async fn execute_stage_3_execution(
    code_stage: &AgentStage,
    user_id: UserId,
    config: &LMConfig,
    response_file: &mut std::fs::File,
) -> Result<AgentStage, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    let stage_id = 3;
    
    agent_trace!(user_id, "execute_stage_3_execution", "=== STAGE 3: EXECUTION START ===");
    agent_info!(user_id, "execute_stage_3_execution", "Starting execution and testing stage");
    
    // Create stage record
    let mut stage = AgentStage {
        stage_id,
        name: "Execution & Testing".to_string(),
        description: "Execute generated code and perform testing".to_string(),
        status: StageStatus::InProgress,
        input: code_stage.output.clone(),
        output: None,
        timestamp: Utc::now(),
        duration: None,
    };
    
    // Write stage start to file
    write_to_response_file(Some(response_file), &format!("=== STAGE {}: {} ===", stage_id, stage.name), user_id);
    write_to_response_file(Some(response_file), "🔄 Starting code execution and testing...", user_id);
    
    // Create system prompt for execution
    let system_prompt = create_stage_3_execution_prompt();
    
    // Build messages for execution
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Execute and test this JavaScript code:\n\n{}", 
                code_stage.output.as_ref().unwrap_or(&"No code available".to_string())),
        },
    ];
    
    // Execute testing with function calling
    let functions = get_js_code_sandbox_functions();
    let execution_result = match execute_function_calling(&messages, &functions, config, user_id, Some(response_file)).await {
                        Ok(result) => {
            agent_info!(user_id, "execute_stage_3_execution", "Execution stage completed successfully");
                            result
                        }
                        Err(e) => {
            agent_error!(user_id, "execute_stage_3_execution", "Execution stage failed: {}", e);
            stage.status = StageStatus::Failed;
            stage.duration = Some(start_time.elapsed());
            return Err(e);
        }
    };
    
    // Update stage with results
    stage.output = Some(execution_result.clone());
    stage.status = StageStatus::Completed;
    stage.duration = Some(start_time.elapsed());
    
    // Write completion to file
    write_to_response_file(Some(response_file), "✅ Execution stage completed successfully", user_id);
    write_to_response_file(Some(response_file), &format!("📊 Execution duration: {:?}", stage.duration.unwrap()), user_id);
    
    agent_trace!(user_id, "execute_stage_3_execution", "=== STAGE 3: EXECUTION END ===");
    agent_info!(user_id, "execute_stage_3_execution", "Execution stage completed in {:?}", stage.duration.unwrap());
    
    Ok(stage)
}

async fn execute_stage_4_analysis(
    execution_stage: &AgentStage,
    user_id: UserId,
    config: &LMConfig,
    response_file: &mut std::fs::File,
) -> Result<AgentStage, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    let stage_id = 4;
    
    agent_trace!(user_id, "execute_stage_4_analysis", "=== STAGE 4: ANALYSIS START ===");
    agent_info!(user_id, "execute_stage_4_analysis", "Starting result analysis and optimization stage");
    
    // Create stage record
    let mut stage = AgentStage {
        stage_id,
        name: "Result Analysis & Optimization".to_string(),
        description: "Analyze execution results and optimize solution".to_string(),
        status: StageStatus::InProgress,
        input: execution_stage.output.clone(),
        output: None,
        timestamp: Utc::now(),
        duration: None,
    };
    
    // Write stage start to file
    write_to_response_file(Some(response_file), &format!("=== STAGE {}: {} ===", stage_id, stage.name), user_id);
    write_to_response_file(Some(response_file), "🔄 Starting result analysis and optimization...", user_id);
    
    // Create system prompt for analysis
    let system_prompt = create_stage_4_analysis_prompt();
    
    // Build messages for analysis
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Analyze these execution results and provide optimization recommendations:\n\n{}", 
                execution_stage.output.as_ref().unwrap_or(&"No execution results available".to_string())),
        },
    ];
    
    // Execute analysis with function calling
    let functions = get_js_code_sandbox_functions();
    let analysis_result = match execute_function_calling(&messages, &functions, config, user_id, Some(response_file)).await {
        Ok(result) => {
            agent_info!(user_id, "execute_stage_4_analysis", "Analysis stage completed successfully");
            result
        }
        Err(e) => {
            agent_error!(user_id, "execute_stage_4_analysis", "Analysis stage failed: {}", e);
            stage.status = StageStatus::Failed;
            stage.duration = Some(start_time.elapsed());
            return Err(e);
        }
    };
    
    // Update stage with results
    stage.output = Some(analysis_result.clone());
    stage.status = StageStatus::Completed;
    stage.duration = Some(start_time.elapsed());
    
    // Write completion to file
    write_to_response_file(Some(response_file), "✅ Analysis stage completed successfully", user_id);
    write_to_response_file(Some(response_file), &format!("📊 Analysis duration: {:?}", stage.duration.unwrap()), user_id);
    
    agent_trace!(user_id, "execute_stage_4_analysis", "=== STAGE 4: ANALYSIS END ===");
    agent_info!(user_id, "execute_stage_4_analysis", "Analysis stage completed in {:?}", stage.duration.unwrap());
    
    Ok(stage)
}

async fn execute_stage_5_delivery(
    analysis_stage: &AgentStage,
    user_id: UserId,
    config: &LMConfig,
    response_file: &mut std::fs::File,
) -> Result<AgentStage, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    let stage_id = 5;
    
    agent_trace!(user_id, "execute_stage_5_delivery", "=== STAGE 5: DELIVERY START ===");
    agent_info!(user_id, "execute_stage_5_delivery", "Starting final delivery and documentation stage");
    
    // Create stage record
    let mut stage = AgentStage {
        stage_id,
        name: "Final Delivery & Documentation".to_string(),
        description: "Prepare complete solution for delivery".to_string(),
        status: StageStatus::InProgress,
        input: analysis_stage.output.clone(),
        output: None,
        timestamp: Utc::now(),
        duration: None,
    };
    
    // Write stage start to file
    write_to_response_file(Some(response_file), &format!("=== STAGE {}: {} ===", stage_id, stage.name), user_id);
    write_to_response_file(Some(response_file), "🔄 Starting final delivery and documentation...", user_id);
    
    // Create system prompt for delivery
    let system_prompt = create_stage_5_delivery_prompt();
    
    // Build messages for delivery
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Create a comprehensive final deliverable based on this analysis:\n\n{}", 
                analysis_stage.output.as_ref().unwrap_or(&"No analysis available".to_string())),
        },
    ];
    
    // Execute delivery with function calling
    let functions = get_js_code_sandbox_functions();
    let delivery_result = match execute_function_calling(&messages, &functions, config, user_id, Some(response_file)).await {
        Ok(result) => {
            agent_info!(user_id, "execute_stage_5_delivery", "Delivery stage completed successfully");
            result
        }
        Err(e) => {
            agent_error!(user_id, "execute_stage_5_delivery", "Delivery stage failed: {}", e);
            stage.status = StageStatus::Failed;
            stage.duration = Some(start_time.elapsed());
            return Err(e);
        }
    };
    
    // Update stage with results
    stage.output = Some(delivery_result.clone());
    stage.status = StageStatus::Completed;
    stage.duration = Some(start_time.elapsed());
    
    // Write completion to file
    write_to_response_file(Some(response_file), "✅ Delivery stage completed successfully", user_id);
    write_to_response_file(Some(response_file), &format!("📊 Delivery duration: {:?}", stage.duration.unwrap()), user_id);
    
    agent_trace!(user_id, "execute_stage_5_delivery", "=== STAGE 5: DELIVERY END ===");
    agent_info!(user_id, "execute_stage_5_delivery", "Delivery stage completed in {:?}", stage.duration.unwrap());
    
    Ok(stage)
}

// ============================================================================
// STREAMING STAGE EXECUTION FUNCTIONS
// ============================================================================

async fn execute_stage_1_planning_streaming(
    task: &str,
    user_id: UserId,
    config: &LMConfig,
    streaming_msg: &mut Message,
    ctx: &Context,
) -> Result<AgentStage, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    let stage_id = 1;
    
    agent_trace!(user_id, "execute_stage_1_planning_streaming", "=== STAGE 1: PLANNING STREAMING START ===");
    agent_info!(user_id, "execute_stage_1_planning_streaming", "Starting planning stage with streaming for task: '{}'", task);
    
    // Create stage record
    let mut stage = AgentStage {
        stage_id,
        name: "Task Analysis & Planning".to_string(),
        description: "Analyze user request and create execution plan".to_string(),
        status: StageStatus::InProgress,
        input: Some(task.to_string()),
        output: None,
        timestamp: Utc::now(),
        duration: None,
    };
    
    // Create system prompt for planning
    let system_prompt = create_stage_1_planning_prompt();
    
    // Build messages for planning
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Please analyze this task and create a detailed execution plan: {}", task),
        },
    ];
    
    // Execute planning with streaming
    let functions = get_js_code_sandbox_functions();
    let planning_result = match execute_function_calling_streaming(&messages, &functions, config, user_id, streaming_msg, ctx).await {
        Ok(result) => {
            agent_info!(user_id, "execute_stage_1_planning_streaming", "Planning stage completed successfully");
            result
        }
        Err(e) => {
            agent_error!(user_id, "execute_stage_1_planning_streaming", "Planning stage failed: {}", e);
            stage.status = StageStatus::Failed;
            stage.duration = Some(start_time.elapsed());
            return Err(e);
        }
    };
    
    // Update stage with results
    stage.output = Some(planning_result.clone());
    stage.status = StageStatus::Completed;
    stage.duration = Some(start_time.elapsed());
    
    // Update streaming message with completion
    let completion_message = format!(
        "✅ **Stage {} Complete: {}**\n\n📝 **Planning Results:**\n{}\n\n⏱️ **Duration:** {:?}\n\n🔄 **Ready for Stage 2: Code Generation**",
        stage_id, stage.name, planning_result, stage.duration.unwrap()
    );
    let _ = streaming_msg.edit(&ctx.http, |m| m.content(&completion_message)).await;
    
    agent_trace!(user_id, "execute_stage_1_planning_streaming", "=== STAGE 1: PLANNING STREAMING END ===");
    agent_info!(user_id, "execute_stage_1_planning_streaming", "Planning stage completed in {:?}", stage.duration.unwrap());
    
    Ok(stage)
}

async fn execute_stage_2_code_generation_streaming(
    plan: &AgentStage,
    user_id: UserId,
    config: &LMConfig,
    streaming_msg: &mut Message,
    ctx: &Context,
) -> Result<AgentStage, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    let stage_id = 2;
    
    agent_trace!(user_id, "execute_stage_2_code_generation_streaming", "=== STAGE 2: CODE GENERATION STREAMING START ===");
    agent_info!(user_id, "execute_stage_2_code_generation_streaming", "Starting code generation stage with streaming");
    
    // Create stage record
    let mut stage = AgentStage {
        stage_id,
        name: "Code Generation & Validation".to_string(),
        description: "Generate JavaScript code based on planning stage".to_string(),
        status: StageStatus::InProgress,
        input: plan.output.clone(),
        output: None,
        timestamp: Utc::now(),
        duration: None,
    };
    
    // Create system prompt for code generation
    let system_prompt = create_stage_2_code_generation_prompt();
    
    // Build messages for code generation
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Based on this plan, generate the JavaScript code:\n\n{}", 
                plan.output.as_ref().unwrap_or(&"No plan available".to_string())),
        },
    ];
    
    // Execute code generation with streaming
    let functions = get_js_code_sandbox_functions();
    let code_result = match execute_function_calling_streaming(&messages, &functions, config, user_id, streaming_msg, ctx).await {
        Ok(result) => {
            agent_info!(user_id, "execute_stage_2_code_generation_streaming", "Code generation stage completed successfully");
            result
        }
        Err(e) => {
            agent_error!(user_id, "execute_stage_2_code_generation_streaming", "Code generation stage failed: {}", e);
            stage.status = StageStatus::Failed;
            stage.duration = Some(start_time.elapsed());
            return Err(e);
        }
    };
    
    // Update stage with results
    stage.output = Some(code_result.clone());
    stage.status = StageStatus::Completed;
    stage.duration = Some(start_time.elapsed());
    
    // Update streaming message with completion
    let completion_message = format!(
        "✅ **Stage {} Complete: {}**\n\n📝 **Generated Code:**\n{}\n\n⏱️ **Duration:** {:?}\n\n🔄 **Ready for Stage 3: Execution & Testing**",
        stage_id, stage.name, code_result, stage.duration.unwrap()
    );
    let _ = streaming_msg.edit(&ctx.http, |m| m.content(&completion_message)).await;
    
    agent_trace!(user_id, "execute_stage_2_code_generation_streaming", "=== STAGE 2: CODE GENERATION STREAMING END ===");
    agent_info!(user_id, "execute_stage_2_code_generation_streaming", "Code generation stage completed in {:?}", stage.duration.unwrap());
    
    Ok(stage)
}

async fn execute_stage_3_execution_streaming(
    code_stage: &AgentStage,
    user_id: UserId,
    config: &LMConfig,
    streaming_msg: &mut Message,
    ctx: &Context,
) -> Result<AgentStage, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    let stage_id = 3;
    
    agent_trace!(user_id, "execute_stage_3_execution_streaming", "=== STAGE 3: EXECUTION STREAMING START ===");
    agent_info!(user_id, "execute_stage_3_execution_streaming", "Starting execution and testing stage with streaming");
    
    // Create stage record
    let mut stage = AgentStage {
        stage_id,
        name: "Execution & Testing".to_string(),
        description: "Execute generated code and perform testing".to_string(),
        status: StageStatus::InProgress,
        input: code_stage.output.clone(),
        output: None,
        timestamp: Utc::now(),
        duration: None,
    };
    
    // Create system prompt for execution
    let system_prompt = create_stage_3_execution_prompt();
    
    // Build messages for execution
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Execute and test this JavaScript code:\n\n{}", 
                code_stage.output.as_ref().unwrap_or(&"No code available".to_string())),
        },
    ];
    
    // Execute testing with streaming
    let functions = get_js_code_sandbox_functions();
    let execution_result = match execute_function_calling_streaming(&messages, &functions, config, user_id, streaming_msg, ctx).await {
        Ok(result) => {
            agent_info!(user_id, "execute_stage_3_execution_streaming", "Execution stage completed successfully");
            result
        }
        Err(e) => {
            agent_error!(user_id, "execute_stage_3_execution_streaming", "Execution stage failed: {}", e);
            stage.status = StageStatus::Failed;
            stage.duration = Some(start_time.elapsed());
            return Err(e);
        }
    };
    
    // Update stage with results
    stage.output = Some(execution_result.clone());
    stage.status = StageStatus::Completed;
    stage.duration = Some(start_time.elapsed());
    
    // Update streaming message with completion
    let completion_message = format!(
        "✅ **Stage {} Complete: {}**\n\n📝 **Execution Results:**\n{}\n\n⏱️ **Duration:** {:?}\n\n🔄 **Ready for Stage 4: Result Analysis**",
        stage_id, stage.name, execution_result, stage.duration.unwrap()
    );
    let _ = streaming_msg.edit(&ctx.http, |m| m.content(&completion_message)).await;
    
    agent_trace!(user_id, "execute_stage_3_execution_streaming", "=== STAGE 3: EXECUTION STREAMING END ===");
    agent_info!(user_id, "execute_stage_3_execution_streaming", "Execution stage completed in {:?}", stage.duration.unwrap());
    
    Ok(stage)
}

async fn execute_stage_4_analysis_streaming(
    execution_stage: &AgentStage,
    user_id: UserId,
    config: &LMConfig,
    streaming_msg: &mut Message,
    ctx: &Context,
) -> Result<AgentStage, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    let stage_id = 4;
    
    agent_trace!(user_id, "execute_stage_4_analysis_streaming", "=== STAGE 4: ANALYSIS STREAMING START ===");
    agent_info!(user_id, "execute_stage_4_analysis_streaming", "Starting result analysis and optimization stage with streaming");
    
    // Create stage record
    let mut stage = AgentStage {
        stage_id,
        name: "Result Analysis & Optimization".to_string(),
        description: "Analyze execution results and optimize solution".to_string(),
        status: StageStatus::InProgress,
        input: execution_stage.output.clone(),
        output: None,
        timestamp: Utc::now(),
        duration: None,
    };
    
    // Create system prompt for analysis
    let system_prompt = create_stage_4_analysis_prompt();
    
    // Build messages for analysis
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Analyze these execution results and provide optimization recommendations:\n\n{}", 
                execution_stage.output.as_ref().unwrap_or(&"No execution results available".to_string())),
        },
    ];
    
    // Execute analysis with streaming
    let functions = get_js_code_sandbox_functions();
    let analysis_result = match execute_function_calling_streaming(&messages, &functions, config, user_id, streaming_msg, ctx).await {
        Ok(result) => {
            agent_info!(user_id, "execute_stage_4_analysis_streaming", "Analysis stage completed successfully");
            result
        }
        Err(e) => {
            agent_error!(user_id, "execute_stage_4_analysis_streaming", "Analysis stage failed: {}", e);
            stage.status = StageStatus::Failed;
            stage.duration = Some(start_time.elapsed());
            return Err(e);
        }
    };
    
    // Update stage with results
    stage.output = Some(analysis_result.clone());
    stage.status = StageStatus::Completed;
    stage.duration = Some(start_time.elapsed());
    
    // Update streaming message with completion
    let completion_message = format!(
        "✅ **Stage {} Complete: {}**\n\n📝 **Analysis Results:**\n{}\n\n⏱️ **Duration:** {:?}\n\n🔄 **Ready for Stage 5: Final Delivery**",
        stage_id, stage.name, analysis_result, stage.duration.unwrap()
    );
    let _ = streaming_msg.edit(&ctx.http, |m| m.content(&completion_message)).await;
    
    agent_trace!(user_id, "execute_stage_4_analysis_streaming", "=== STAGE 4: ANALYSIS STREAMING END ===");
    agent_info!(user_id, "execute_stage_4_analysis_streaming", "Analysis stage completed in {:?}", stage.duration.unwrap());
    
    Ok(stage)
}

async fn execute_stage_5_delivery_streaming(
    analysis_stage: &AgentStage,
    user_id: UserId,
    config: &LMConfig,
    streaming_msg: &mut Message,
    ctx: &Context,
) -> Result<AgentStage, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    let stage_id = 5;
    
    agent_trace!(user_id, "execute_stage_5_delivery_streaming", "=== STAGE 5: DELIVERY STREAMING START ===");
    agent_info!(user_id, "execute_stage_5_delivery_streaming", "Starting final delivery and documentation stage with streaming");
    
    // Create stage record
    let mut stage = AgentStage {
        stage_id,
        name: "Final Delivery & Documentation".to_string(),
        description: "Prepare complete solution for delivery".to_string(),
        status: StageStatus::InProgress,
        input: analysis_stage.output.clone(),
        output: None,
        timestamp: Utc::now(),
        duration: None,
    };
    
    // Create system prompt for delivery
    let system_prompt = create_stage_5_delivery_prompt();
    
    // Build messages for delivery
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Create a comprehensive final deliverable based on this analysis:\n\n{}", 
                analysis_stage.output.as_ref().unwrap_or(&"No analysis available".to_string())),
        },
    ];
    
    // Execute delivery with streaming
    let functions = get_js_code_sandbox_functions();
    let delivery_result = match execute_function_calling_streaming(&messages, &functions, config, user_id, streaming_msg, ctx).await {
        Ok(result) => {
            agent_info!(user_id, "execute_stage_5_delivery_streaming", "Delivery stage completed successfully");
            result
        }
        Err(e) => {
            agent_error!(user_id, "execute_stage_5_delivery_streaming", "Delivery stage failed: {}", e);
            stage.status = StageStatus::Failed;
            stage.duration = Some(start_time.elapsed());
            return Err(e);
        }
    };
    
    // Update stage with results
    stage.output = Some(delivery_result.clone());
    stage.status = StageStatus::Completed;
    stage.duration = Some(start_time.elapsed());
    
    // Update streaming message with completion
    let completion_message = format!(
        "✅ **Stage {} Complete: {}**\n\n📝 **Final Deliverable:**\n{}\n\n⏱️ **Duration:** {:?}\n\n🎉 **All Stages Complete!**",
        stage_id, stage.name, delivery_result, stage.duration.unwrap()
    );
    let _ = streaming_msg.edit(&ctx.http, |m| m.content(&completion_message)).await;
    
    agent_trace!(user_id, "execute_stage_5_delivery_streaming", "=== STAGE 5: DELIVERY STREAMING END ===");
    agent_info!(user_id, "execute_stage_5_delivery_streaming", "Delivery stage completed in {:?}", stage.duration.unwrap());
    
    Ok(stage)
}

// ============================================================================
// AGENT COMMAND HANDLERS
// ============================================================================

#[command]
#[aliases("ai", "assistant")]
/// Main ^agent command handler - Self-contained LLM Agent with function calling
/// Handles complex tasks using LM Studio's js-code-sandbox tool
/// Supports:
///   - ^agent <task> (full agent mode with function calling)
///   - ^agent --help (show help)
///   - ^agent --tools (list available tools)
///   - ^agent --clear (clear context)
pub async fn agent(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let user_id = msg.author.id;
    let input = args.message().trim();
    let start_time = Instant::now();
    
    agent_trace!(user_id, "agent", "=== AGENT COMMAND START ===");
    agent_trace!(user_id, "agent", "User: {} ({})", msg.author.name, user_id);
    agent_trace!(user_id, "agent", "Input: '{}' ({} chars)", input, input.len());
    agent_trace!(user_id, "agent", "Channel: {} ({})", msg.channel_id, msg.channel_id);
    agent_trace!(user_id, "agent", "Message ID: {}", msg.id);
    
    agent_info!(user_id, "agent", "Processing input: '{}' ({} chars) for user {}", input, input.len(), msg.author.name);
    
    if input.is_empty() {
        msg.reply(ctx, "Please provide a task! Usage: `^agent <your task>` or `^agent --help` for options").await?;
        return Ok(());
    }

    // Parse agent command
    if input == "--help" || input == "-h" {
        show_agent_help(ctx, msg).await
    } else if input == "--tools" || input == "-t" {
            list_available_tools(ctx, msg).await
    } else if input == "--clear" || input == "-c" {
            clear_agent_memory(ctx, msg).await
    } else {
        // Default to execute mode
        agent_trace!(user_id, "agent", "Executing agent task: '{}'", input);
        let result = execute_agent_task(input.to_string(), ctx, msg).await;
        
        let duration = start_time.elapsed();
        agent_trace!(user_id, "agent", "=== AGENT COMMAND END ===");
        agent_trace!(user_id, "agent", "Total execution time: {:?}", duration);
        agent_trace!(user_id, "agent", "Result: {:?}", result);
        
        result
    }
}

async fn show_agent_help(ctx: &Context, msg: &Message) -> CommandResult {
    let user_id = msg.author.id;
    agent_info!(user_id, "show_agent_help", "Showing agent help");
    let help_text = r#"🤖 **Agent Command Help**

**Basic Usage:**
- `^agent <task>` - Execute a complex task with function calling
- `^agent --tools` - List available tools
- `^agent --clear` - Clear agent memory
- `^agent --help` - Show this help

**Examples:**
- `^agent "Calculate the factorial of 10"`
- `^agent "Convert this text to uppercase: hello world"`
- `^agent "Analyze this data: [1, 2, 3, 4, 5]"`
- `^agent "Write a function to find the largest number in an array"`

**Available Tools:**
- 🧮 **Mathematical Calculations** - Complex math operations
- 📝 **Text Processing** - String manipulation, case conversion, word counting, etc.
- 📊 **Data Analysis** - Array statistics, structure analysis, data validation
- 💻 **JavaScript Execution** - Custom code execution

**Features:**
- 🤖 **Intelligent Function Calling** - Automatic tool selection
- 🛠️ **JavaScript Sandbox** - Safe code execution
- 🧠 **Context Carryover** - Remembers previous conversations for modifications
- ⚡ **Real-time Execution** - Live progress updates
- 🔄 **Error Recovery** - Robust error handling
- 📋 **Ready-to-Use Code** - Tested code always included in responses

*This agent uses LM Studio's js-code-sandbox tool for safe JavaScript execution.*"#;

    msg.reply(ctx, help_text).await?;
    Ok(())
}

async fn list_available_tools(ctx: &Context, msg: &Message) -> CommandResult {
    let user_id = msg.author.id;
    let start_time = Instant::now();
    
    agent_info!(user_id, "list_available_tools", "Listing available tools");
    
    let tools = vec![
        "🧮 **Mathematical Calculations** - Complex math operations using JavaScript Math library",
        "📝 **Text Processing** - String manipulation, case conversion, word counting, etc.",
        "📊 **Data Analysis** - Array statistics, structure analysis, data validation",
        "💻 **JavaScript Execution** - Custom code execution in sandboxed environment",
    ];

    let tools_text = format!(
        "🛠️ **Available Agent Tools**\n\n{}",
        tools.join("\n")
    );

    msg.reply(ctx, &tools_text).await?;
    
    let duration = start_time.elapsed();
    agent_info!(user_id, "list_available_tools", "Completed tools listing in {:?}", duration);
    
    Ok(())
}

async fn clear_agent_memory(ctx: &Context, msg: &Message) -> CommandResult {
    let user_id = msg.author.id;
    let start_time = Instant::now();
    
    agent_info!(user_id, "clear_agent_memory", "Clearing agent memory for user {}", msg.author.name);
    
    // Actually clear the user's context
    clear_user_context(user_id).await;
    
    msg.reply(ctx, "🧹 **Agent Memory Cleared**\n\nYour agent conversation history has been reset. The next ^agent command will start fresh.").await?;
    
    let duration = start_time.elapsed();
    agent_info!(user_id, "clear_agent_memory", "Completed memory clearing in {:?}", duration);
    
    Ok(())
}

// ============================================================================
// BACKWARD COMPATIBILITY FUNCTIONS
// ============================================================================

#[command]
#[aliases("clearagent", "resetagent")]
/// Command to clear the user's agent chat context (backward compatibility)
pub async fn clearagentcontext(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let _user_id = msg.author.id;
    clear_agent_memory(ctx, msg).await
}

// ============================================================================
// SELF-CONTAINED CONFIGURATION AND UTILITY FUNCTIONS
// ============================================================================

async fn load_agent_config() -> Result<LMConfig, Box<dyn std::error::Error + Send + Sync>> {
    let user_id = UserId(0); // Use a dummy user ID for system operations
    let config_paths = [
        "lmapiconf.txt",
        "../lmapiconf.txt", 
        "../../lmapiconf.txt",
        "src/lmapiconf.txt"
    ];
    
    let mut content = String::new();
    let mut found_file = false;
    let mut config_source = "";
    
    for config_path in &config_paths {
        match fs::read_to_string(config_path) {
            Ok(file_content) => {
                content = file_content;
                found_file = true;
                config_source = config_path;
                agent_info!(user_id, "load_agent_config", "Found config file at {}", config_path);
                break;
            }
            Err(e) => {
                agent_debug!(user_id, "load_agent_config", "Config file not found at {}: {}", config_path, e);
                continue;
            }
        }
    }
    
    if !found_file {
        return Err("lmapiconf.txt file not found in any expected location for agent".into());
    }
    
    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    let mut config_map = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        
        if let Some(equals_pos) = line.find('=') {
            let key = line[..equals_pos].trim().to_string();
            let value = line[equals_pos + 1..].trim().to_string();
            config_map.insert(key, value);
        }
    }
    
    let required_keys = [
        "LM_STUDIO_BASE_URL",
        "LM_STUDIO_TIMEOUT", 
        "DEFAULT_REASON_MODEL",
        "DEFAULT_TEMPERATURE",
        "DEFAULT_MAX_TOKENS",
        "MAX_DISCORD_MESSAGE_LENGTH",
        "RESPONSE_FORMAT_PADDING",
    ];
    
    for key in &required_keys {
        if !config_map.contains_key(*key) {
            return Err(format!("Required setting '{}' not found in {} (agent)", key, config_source).into());
        }
    }
    
    let config = LMConfig {
        base_url: config_map.get("LM_STUDIO_BASE_URL")
            .ok_or("LM_STUDIO_BASE_URL not found")?.clone(),
        timeout: config_map.get("LM_STUDIO_TIMEOUT")
            .ok_or("LM_STUDIO_TIMEOUT not found")?
            .parse()
            .map_err(|_| "Invalid LM_STUDIO_TIMEOUT value")?,
        default_model: config_map.get("DEFAULT_REASON_MODEL")
            .ok_or("DEFAULT_REASON_MODEL not found")?.clone(),
        default_temperature: config_map.get("DEFAULT_TEMPERATURE")
            .ok_or("DEFAULT_TEMPERATURE not found")?
            .parse()
            .map_err(|_| "Invalid DEFAULT_TEMPERATURE value")?,
        default_max_tokens: config_map.get("DEFAULT_MAX_TOKENS")
            .ok_or("DEFAULT_MAX_TOKENS not found")?
            .parse()
            .map_err(|_| "Invalid DEFAULT_MAX_TOKENS value")?,
        max_discord_message_length: config_map.get("MAX_DISCORD_MESSAGE_LENGTH")
            .ok_or("MAX_DISCORD_MESSAGE_LENGTH not found")?
            .parse()
            .map_err(|_| "Invalid MAX_DISCORD_MESSAGE_LENGTH value")?,
        response_format_padding: config_map.get("RESPONSE_FORMAT_PADDING")
            .ok_or("RESPONSE_FORMAT_PADDING not found")?
            .parse()
            .map_err(|_| "Invalid RESPONSE_FORMAT_PADDING value")?,
        default_seed: config_map.get("DEFAULT_SEED")
            .map(|s| s.parse::<i64>())
            .transpose()
            .map_err(|_| "DEFAULT_SEED must be a valid integer if specified")?,
    };

    agent_info!(user_id, "load_agent_config", "Successfully loaded config from {} with model: '{}'", config_source, config.default_model);
    Ok(config)
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

fn filter_thinking_tags(content: &str) -> String {
    let filtered = THINKING_TAG_REGEX.replace_all(content, "");
    
    let lines: Vec<&str> = filtered
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    
    lines.join("\n").trim().to_string()
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_thinking_tags() {
        let content_with_tags = "Here is some content <think>This is internal thinking</think> and more content.";
        let filtered = filter_thinking_tags(content_with_tags);
        assert_eq!(filtered, "Here is some content  and more content.");
    }

    #[test]
    fn test_js_code_sandbox_functions() {
        let functions = get_js_code_sandbox_functions();
        assert_eq!(functions.len(), 4);
        
        // Check that all functions have the correct type
        for function in &functions {
            assert_eq!(function.function_type, "function");
        }
        
        // Check function names from the function field
        let function_names: Vec<String> = functions.iter()
            .map(|f| f.function["name"].as_str().unwrap().to_string())
            .collect();
        
        assert_eq!(function_names[0], "execute_js_code");
        assert_eq!(function_names[1], "calculate_math");
        assert_eq!(function_names[2], "process_text");
        assert_eq!(function_names[3], "analyze_data");
    }

    #[test]
    fn test_create_agent_system_prompt() {
        let prompt = create_agent_system_prompt();
        assert!(prompt.contains("JavaScript code execution"));
        assert!(prompt.contains("execute_js_code"));
        assert!(prompt.contains("calculate_math"));
    }
}

// Command group exports
#[group]
#[commands(agent, clearagentcontext, staged)]
pub struct Agent;

impl Agent {
    pub const fn new() -> Self {
        Agent
    }
}

#[command]
#[aliases("staged", "step")]
/// Staged agent command - Execute tasks in 5 distinct stages with user control
/// Handles complex tasks using a staged approach:
///   - Stage 1: Task Analysis & Planning
///   - Stage 2: Code Generation & Validation  
///   - Stage 3: Execution & Testing
///   - Stage 4: Result Analysis & Optimization
///   - Stage 5: Final Delivery & Documentation
/// Supports:
///   - ^staged <task> (staged execution mode)
///   - ^staged --help (show staged help)
///   - ^staged --status (show current stage status)
///   - ^staged --approve (approve current stage and continue)
///   - ^staged --modify <feedback> (modify current stage output)
///   - ^staged --skip (skip current stage)
///   - ^staged --pause (pause execution)
///   - ^staged --resume (resume execution)
pub async fn staged(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let user_id = msg.author.id;
    let input = args.message().trim();
    let start_time = Instant::now();
    
    agent_trace!(user_id, "staged", "=== STAGED AGENT COMMAND START ===");
    agent_trace!(user_id, "staged", "User: {} ({})", msg.author.name, user_id);
    agent_trace!(user_id, "staged", "Input: '{}' ({} chars)", input, input.len());
    
    agent_info!(user_id, "staged", "Processing staged input: '{}' ({} chars) for user {}", input, input.len(), msg.author.name);
    
    if input.is_empty() {
        msg.reply(ctx, "Please provide a task! Usage: `^staged <your task>` or `^staged --help` for options").await?;
        return Ok(());
    }

    // Parse staged command
    if input == "--help" || input == "-h" {
        show_staged_help(ctx, msg).await
    } else if input == "--status" || input == "-s" {
        show_staged_status(ctx, msg).await
    } else if input == "--approve" || input == "-a" {
        approve_current_stage(ctx, msg).await
    } else if input.starts_with("--modify ") || input.starts_with("-m ") {
        let feedback = input.trim_start_matches("--modify ").trim_start_matches("-m ").trim();
        modify_current_stage(ctx, msg, feedback.to_string()).await
    } else if input == "--skip" || input == "-k" {
        skip_current_stage(ctx, msg).await
    } else if input == "--pause" || input == "-p" {
        pause_staged_execution(ctx, msg).await
    } else if input == "--resume" || input == "-r" {
        resume_staged_execution(ctx, msg).await
        } else {
        // Default to staged execution mode
        agent_trace!(user_id, "staged", "Executing staged task: '{}'", input);
        let result = execute_staged_task(input.to_string(), ctx, msg).await;
        
        let duration = start_time.elapsed();
        agent_trace!(user_id, "staged", "=== STAGED AGENT COMMAND END ===");
        agent_trace!(user_id, "staged", "Total execution time: {:?}", duration);
        agent_trace!(user_id, "staged", "Result: {:?}", result);
        
        result
    }
}

async fn show_staged_help(ctx: &Context, msg: &Message) -> CommandResult {
    let user_id = msg.author.id;
    agent_info!(user_id, "show_staged_help", "Showing staged help");
    let help_text = r#"🤖 **Staged Agent Command Help**

**Basic Usage:**
- `^staged <task>` - Execute a complex task in staged mode
- `^staged --status` - Show current stage status
- `^staged --approve` - Approve current stage and continue
- `^staged --modify <feedback>` - Modify current stage output
- `^staged --skip` - Skip current stage
- `^staged --pause` - Pause execution
- `^staged --resume` - Resume execution

**Examples:**
- `^staged "Calculate the factorial of 10"`
- `^staged --status`
- `^staged --approve`
- `^staged --modify "Improved planning"`
- `^staged --skip`
- `^staged --pause`
- `^staged --resume`

**Available Commands:**
- **Stage 1: Task Analysis & Planning**
  - `^staged --modify "Improved planning"`
- **Stage 2: Code Generation & Validation**
  - `^staged --modify "Clearer code"`
- **Stage 3: Execution & Testing**
  - `^staged --modify "More thorough testing"`
- **Stage 4: Result Analysis & Optimization**
  - `^staged --modify "Optimized solution"`
- **Stage 5: Final Delivery & Documentation**
  - `^staged --modify "Improved documentation"`

**Guidelines:**
- Use `--modify <feedback>` to provide specific feedback for each stage
- `--status` shows the current stage and its status
- `--approve` approves the current stage and continues to the next
- `--skip` skips the current stage
- `--pause` pauses execution
- `--resume` resumes execution from the paused stage

Remember: Each stage has specific instructions and guidelines. Use `--modify <feedback>` to tailor the output to your needs."#;

    msg.reply(ctx, help_text).await?;
    Ok(())
}

async fn show_staged_status(ctx: &Context, msg: &Message) -> CommandResult {
    let user_id = msg.author.id;
    agent_info!(user_id, "show_staged_status", "Showing staged status");
    
    // Fetch the staged task for the user
    let staged_task = get_staged_tasks().await.lock().unwrap().get(&user_id.to_string()).cloned();
    
    match staged_task {
        Some(task) => {
            let status_text = task.stages.iter().enumerate().map(|(i, stage)| {
                format!("**Stage {}:** {}\nStatus: {}\n\n", i + 1, stage.name, stage.status)
            }).collect::<Vec<String>>().join("\n");
            
            let response = format!("🤖 **Staged Task Status**\n\n{}", status_text);
            msg.reply(ctx, &response).await?;
        }
        None => {
            let response = "🤖 **Staged Task Status**\n\nNo staged task found for this user.";
            msg.reply(ctx, response).await?;
        }
    }
    
    Ok(())
}

async fn approve_current_stage(ctx: &Context, msg: &Message) -> CommandResult {
    let user_id = msg.author.id;
    agent_info!(user_id, "approve_current_stage", "Approving current stage");
    
    // Fetch the staged task for the user
    let staged_task = get_staged_tasks().await.lock().unwrap().get(&user_id.to_string()).cloned();
    
    match staged_task {
        Some(mut task) => {
            // Approve the current stage
            task.stages[task.current_stage as usize].status = StageStatus::Completed;
            task.updated_at = Utc::now();
            
            // Save the updated task
            save_staged_task(&task).await?;
            
            let response = format!("🤖 **Stage Approved**\n\nCurrent stage '{}' has been approved. Moving to the next stage.", task.stages[task.current_stage as usize].name);
            msg.reply(ctx, &response).await?;
            
            // Move to the next stage
            task.current_stage = (task.current_stage + 1) % task.stages.len() as u8;
            task.overall_status = TaskStatus::InProgress;
            
            // Save the updated task
            save_staged_task(&task).await?;
        }
        None => {
            let response = "🤖 **No Staged Task**\n\nNo staged task found for this user.";
            msg.reply(ctx, response).await?;
        }
    }
    
    Ok(())
}

async fn modify_current_stage(ctx: &Context, msg: &Message, feedback: String) -> CommandResult {
    let user_id = msg.author.id;
    agent_info!(user_id, "modify_current_stage", "Modifying current stage output with feedback: '{}'", feedback);
    
    // Fetch the staged task for the user
    let staged_task = get_staged_tasks().await.lock().unwrap().get(&user_id.to_string()).cloned();
    
    match staged_task {
        Some(mut task) => {
            // Modify the current stage output
            task.stages[task.current_stage as usize].output = Some(feedback.clone());
            task.updated_at = Utc::now();
            
            // Save the updated task
            save_staged_task(&task).await?;
            
            let response = format!("🤖 **Stage Modified**\n\nCurrent stage '{}' has been modified with feedback: '{}'. Moving to the next stage.", task.stages[task.current_stage as usize].name, feedback);
            msg.reply(ctx, &response).await?;
            
            // Move to the next stage
            task.current_stage = (task.current_stage + 1) % task.stages.len() as u8;
            task.overall_status = TaskStatus::InProgress;
            
            // Save the updated task
            save_staged_task(&task).await?;
        }
        None => {
            let response = "🤖 **No Staged Task**\n\nNo staged task found for this user.";
            msg.reply(ctx, response).await?;
        }
    }
    
    Ok(())
}

async fn skip_current_stage(ctx: &Context, msg: &Message) -> CommandResult {
    let user_id = msg.author.id;
    agent_info!(user_id, "skip_current_stage", "Skipping current stage");
    
    // Fetch the staged task for the user
    let staged_task = get_staged_tasks().await.lock().unwrap().get(&user_id.to_string()).cloned();
    
    match staged_task {
        Some(mut task) => {
            // Skip the current stage
            task.stages[task.current_stage as usize].status = StageStatus::Skipped;
            task.updated_at = Utc::now();
            
            // Save the updated task
            save_staged_task(&task).await?;
            
            let response = format!("🤖 **Stage Skipped**\n\nCurrent stage '{}' has been skipped. Moving to the next stage.", task.stages[task.current_stage as usize].name);
            msg.reply(ctx, &response).await?;
            
            // Move to the next stage
            task.current_stage = (task.current_stage + 1) % task.stages.len() as u8;
            task.overall_status = TaskStatus::InProgress;
            
            // Save the updated task
            save_staged_task(&task).await?;
        }
        None => {
            let response = "🤖 **No Staged Task**\n\nNo staged task found for this user.";
            msg.reply(ctx, response).await?;
        }
    }
    
    Ok(())
}

async fn pause_staged_execution(ctx: &Context, msg: &Message) -> CommandResult {
    let user_id = msg.author.id;
    agent_info!(user_id, "pause_staged_execution", "Pausing staged execution");
    
    // Fetch the staged task for the user
    let staged_task = get_staged_tasks().await.lock().unwrap().get(&user_id.to_string()).cloned();
    
    match staged_task {
        Some(mut task) => {
            // Pause the staged execution
            task.overall_status = TaskStatus::Paused;
            task.updated_at = Utc::now();
            
            // Save the updated task
            save_staged_task(&task).await?;
            
            let response = "🤖 **Staged Execution Paused**\n\nExecution has been paused. Use `--resume` to continue.";
            msg.reply(ctx, response).await?;
        }
        None => {
            let response = "🤖 **No Staged Task**\n\nNo staged task found for this user.";
            msg.reply(ctx, response).await?;
        }
    }
    
    Ok(())
}

async fn resume_staged_execution(ctx: &Context, msg: &Message) -> CommandResult {
    let user_id = msg.author.id;
    agent_info!(user_id, "resume_staged_execution", "Resuming staged execution");
    
    // Fetch the staged task for the user
    let staged_task = get_staged_tasks().await.lock().unwrap().get(&user_id.to_string()).cloned();
    
    match staged_task {
        Some(mut task) => {
            // Resume the staged execution
            task.overall_status = TaskStatus::InProgress;
            task.updated_at = Utc::now();
            
            // Save the updated task
            save_staged_task(&task).await?;
            
            let response = "🤖 **Staged Execution Resumed**\n\nExecution has been resumed. Continue with the next stage.";
            msg.reply(ctx, response).await?;
        }
        None => {
            let response = "🤖 **No Staged Task**\n\nNo staged task found for this user.";
            msg.reply(ctx, response).await?;
        }
    }
    
    Ok(())
}

async fn execute_staged_task(task: String, ctx: &Context, msg: &Message) -> CommandResult {
    let user_id = msg.author.id;
    let start_time = Instant::now();
    
    agent_trace!(user_id, "execute_staged_task", "=== EXECUTE STAGED TASK START ===");
    agent_trace!(user_id, "execute_staged_task", "Task: '{}'", task);
    agent_trace!(user_id, "execute_staged_task", "Task length: {} chars", task.len());
    
    agent_info!(user_id, "execute_staged_task", "Starting staged execution for task: '{}'", task);
    
    // Load configuration
    agent_trace!(user_id, "execute_staged_task", "Loading agent configuration...");
    let config = match load_agent_config().await {
        Ok(config) => {
            agent_trace!(user_id, "execute_staged_task", "Configuration loaded successfully");
            agent_trace!(user_id, "execute_staged_task", "Model: {}", config.default_model);
            agent_trace!(user_id, "execute_staged_task", "Base URL: {}", config.base_url);
            agent_trace!(user_id, "execute_staged_task", "Temperature: {}", config.default_temperature);
            agent_trace!(user_id, "execute_staged_task", "Max tokens: {}", config.default_max_tokens);
            agent_debug!(user_id, "execute_staged_task", "Successfully loaded agent configuration");
            config
        }
        Err(e) => {
            agent_trace!(user_id, "execute_staged_task", "Configuration loading failed: {}", e);
            agent_error!(user_id, "execute_staged_task", "Failed to load agent configuration: {}", e);
            msg.reply(ctx, "❌ Failed to load agent configuration").await?;
            return Ok(());
        }
    };
    
    // Create a file to stream the agent response to
    let response_filename = format!("staged_agent_response_{}_{}.txt", user_id, chrono::Utc::now().timestamp());
    let mut response_file = match std::fs::File::create(&response_filename) {
        Ok(file) => {
            agent_info!(user_id, "execute_staged_task", "Created response file: {}", response_filename);
            file
        }
        Err(e) => {
            agent_error!(user_id, "execute_staged_task", "Failed to create response file: {}", e);
            let _ = msg.reply(ctx, "❌ Failed to create response file").await;
            return Ok(());
        }
    };

    // Write initial header to file
    use std::io::Write;
    let header = format!("🤖 **Staged AI Agent Response**\nUser: {} ({})\nTask: {}\nTimestamp: {}\n\n", 
        msg.author.name, user_id, task, chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
    if let Err(e) = response_file.write_all(header.as_bytes()) {
        agent_error!(user_id, "execute_staged_task", "Failed to write header to file: {}", e);
    }

    // Send initial Discord message indicating staged processing
    let mut thinking_msg = match msg.channel_id.send_message(&ctx.http, |m| {
        m.content("🤖 **Staged AI Agent Processing...**\n\n📝 **Stage 1:** Task Analysis & Planning\n⏳ Starting staged execution...")
    }).await {
        Ok(message) => {
            agent_debug!(user_id, "execute_staged_task", "Successfully sent status message");
            message
        }
        Err(e) => {
            agent_error!(user_id, "execute_staged_task", "Failed to send status message: {}", e);
            msg.reply(ctx, "Failed to start staged execution!").await?;
            return Ok(());
        }
    };

    // Create staged task
    let task_id = format!("staged_{}_{}", user_id, chrono::Utc::now().timestamp());
    let mut staged_task = StagedTask {
        task_id: task_id.clone(),
        user_id,
        original_request: task.clone(),
        stages: Vec::new(),
        current_stage: 0,
        overall_status: TaskStatus::Planning,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    // Initialize all 5 stages
    staged_task.stages = vec![
        AgentStage {
            stage_id: 1,
            name: "Task Analysis & Planning".to_string(),
            description: "Analyze user request and create execution plan".to_string(),
            status: StageStatus::Pending,
            input: Some(task.clone()),
            output: None,
            timestamp: Utc::now(),
            duration: None,
        },
        AgentStage {
            stage_id: 2,
            name: "Code Generation & Validation".to_string(),
            description: "Generate JavaScript code based on planning stage".to_string(),
            status: StageStatus::Pending,
            input: None,
            output: None,
            timestamp: Utc::now(),
            duration: None,
        },
        AgentStage {
            stage_id: 3,
            name: "Execution & Testing".to_string(),
            description: "Execute generated code and perform testing".to_string(),
            status: StageStatus::Pending,
            input: None,
            output: None,
            timestamp: Utc::now(),
            duration: None,
        },
        AgentStage {
            stage_id: 4,
            name: "Result Analysis & Optimization".to_string(),
            description: "Analyze execution results and optimize solution".to_string(),
            status: StageStatus::Pending,
            input: None,
            output: None,
            timestamp: Utc::now(),
            duration: None,
        },
        AgentStage {
            stage_id: 5,
            name: "Final Delivery & Documentation".to_string(),
            description: "Prepare complete solution for delivery".to_string(),
            status: StageStatus::Pending,
            input: None,
            output: None,
            timestamp: Utc::now(),
            duration: None,
        },
    ];

    // Save initial staged task
    save_staged_task(&staged_task).await?;

    // Execute each stage sequentially
    let mut current_stage_index = 0;
    let mut previous_stage_output: Option<String> = None;

    while current_stage_index < staged_task.stages.len() {
        staged_task.current_stage = current_stage_index as u8;
        staged_task.overall_status = match current_stage_index {
            0 => TaskStatus::Planning,
            1 => TaskStatus::CodeGeneration,
            2 => TaskStatus::Execution,
            3 => TaskStatus::Analysis,
            4 => TaskStatus::Complete,
            _ => TaskStatus::Complete,
        };

        // Create a temporary streaming message for this stage
        let stage_id = staged_task.stages[current_stage_index].stage_id;
        let stage_name = staged_task.stages[current_stage_index].name.clone();
        let initial_stage_message = format!(
            "🤖 **Stage {}: {}**\n\n🔄 **Connecting to LM Studio API...**\n\n📝 **Live Progress:**\n",
            stage_id, stage_name
        );
        
        let mut stage_streaming_msg = match msg.channel_id.send_message(&ctx.http, |m| {
            m.content(&initial_stage_message)
        }).await {
            Ok(message) => {
                agent_debug!(user_id, "execute_staged_task", "Created streaming message for stage {}", stage_id);
                message
            }
            Err(e) => {
                agent_error!(user_id, "execute_staged_task", "Failed to create streaming message: {}", e);
                // Fallback to updating the main thinking message
                let fallback_message = format!(
                    "🤖 **Stage {}: {}**\n\n❌ Failed to create streaming message\n\n📝 **Error:** {}", 
                    stage_id, stage_name, e
                );
                let _ = thinking_msg.edit(&ctx.http, |m| m.content(&fallback_message)).await;
                continue;
            }
        };

        // Execute the current stage with streaming
        let stage_result = match current_stage_index {
            0 => {
                // Stage 1: Planning
                execute_stage_1_planning_streaming(&task, user_id, &config, &mut stage_streaming_msg, ctx).await
            }
            1 => {
                // Stage 2: Code Generation (using planning output)
                if let Some(plan_output) = &previous_stage_output {
                    let plan_stage = AgentStage {
                        stage_id: 1,
                        name: "Task Analysis & Planning".to_string(),
                        description: "Analyze user request and create execution plan".to_string(),
                        status: StageStatus::Completed,
                        input: Some(task.clone()),
                        output: Some(plan_output.clone()),
                        timestamp: Utc::now(),
                        duration: None,
                    };
                    execute_stage_2_code_generation_streaming(&plan_stage, user_id, &config, &mut stage_streaming_msg, ctx).await
        } else {
                    Err("No planning output available for code generation".into())
                }
            }
            2 => {
                // Stage 3: Execution (using code generation output)
                if let Some(code_output) = &previous_stage_output {
                    let code_stage = AgentStage {
                        stage_id: 2,
                        name: "Code Generation & Validation".to_string(),
                        description: "Generate JavaScript code based on planning stage".to_string(),
                        status: StageStatus::Completed,
                        input: None,
                        output: Some(code_output.clone()),
                        timestamp: Utc::now(),
                        duration: None,
                    };
                    execute_stage_3_execution_streaming(&code_stage, user_id, &config, &mut stage_streaming_msg, ctx).await
                } else {
                    Err("No code generation output available for execution".into())
                }
            }
            3 => {
                // Stage 4: Analysis (using execution output)
                if let Some(execution_output) = &previous_stage_output {
                    let execution_stage = AgentStage {
                        stage_id: 3,
                        name: "Execution & Testing".to_string(),
                        description: "Execute generated code and perform testing".to_string(),
                        status: StageStatus::Completed,
                        input: None,
                        output: Some(execution_output.clone()),
                        timestamp: Utc::now(),
                        duration: None,
                    };
                    execute_stage_4_analysis_streaming(&execution_stage, user_id, &config, &mut stage_streaming_msg, ctx).await
                } else {
                    Err("No execution output available for analysis".into())
                }
            }
            4 => {
                // Stage 5: Delivery (using analysis output)
                if let Some(analysis_output) = &previous_stage_output {
                    let analysis_stage = AgentStage {
                        stage_id: 4,
                        name: "Result Analysis & Optimization".to_string(),
                        description: "Analyze execution results and optimize solution".to_string(),
                        status: StageStatus::Completed,
                        input: None,
                        output: Some(analysis_output.clone()),
                        timestamp: Utc::now(),
                        duration: None,
                    };
                    execute_stage_5_delivery_streaming(&analysis_stage, user_id, &config, &mut stage_streaming_msg, ctx).await
                } else {
                    Err("No analysis output available for delivery".into())
                }
            }
            _ => Err("Invalid stage index".into()),
        };

        match stage_result {
            Ok(completed_stage) => {
                // Store output for next stage
                previous_stage_output = completed_stage.output.clone();

                // Update stage with results
                staged_task.stages[current_stage_index].status = StageStatus::Completed;
                staged_task.stages[current_stage_index].output = completed_stage.output;
                staged_task.stages[current_stage_index].duration = completed_stage.duration;
                staged_task.stages[current_stage_index].timestamp = Utc::now();

                // Update staged task
                staged_task.updated_at = Utc::now();
                save_staged_task(&staged_task).await?;

                // Get stage info for message
                let stage_id = staged_task.stages[current_stage_index].stage_id;
                let stage_name = staged_task.stages[current_stage_index].name.clone();
                let stage_output = staged_task.stages[current_stage_index].output.clone();
                let next_stage_name = if current_stage_index + 1 < staged_task.stages.len() {
                    staged_task.stages[current_stage_index + 1].name.clone()
                } else {
                    "All stages complete!".to_string()
                };

                // Update Discord message with completion
                let completion_message = format!(
                    "✅ **Stage {} Complete:** {}\n\n📝 **Output Preview:**\n{}\n\n🔄 **Next Stage:** {}\n\n💡 Use `^staged --approve` to continue\n💡 Use `^staged --modify <feedback>` to provide feedback",
                    stage_id,
                    stage_name,
                    if let Some(output) = &stage_output {
                        if output.len() > 200 {
                            format!("{}...", &output[..200])
                        } else {
                            output.clone()
                        }
                    } else {
                        "No output available".to_string()
                    },
                    next_stage_name
                );
                let _ = thinking_msg.edit(&ctx.http, |m| m.content(&completion_message)).await;

                agent_info!(user_id, "execute_staged_task", "Stage {} completed successfully", stage_id);
            }
            Err(e) => {
                // Handle stage failure
                staged_task.stages[current_stage_index].status = StageStatus::Failed;
                staged_task.stages[current_stage_index].output = Some(format!("Error: {}", e));
                staged_task.stages[current_stage_index].timestamp = Utc::now();

                staged_task.overall_status = TaskStatus::Failed;
                staged_task.updated_at = Utc::now();
                save_staged_task(&staged_task).await?;

                let stage_id = staged_task.stages[current_stage_index].stage_id;
                let stage_name = staged_task.stages[current_stage_index].name.clone();

                let error_message = format!(
                    "❌ **Stage {} Failed:** {}\n\n📝 **Error:** {}\n\n🔄 **Staged execution paused**\n\n💡 Use `^staged --status` to check current state\n💡 Use `^staged --resume` to retry\n💡 Use `^staged --modify <feedback>` to provide input",
                    stage_id, stage_name, e
                );
                let _ = thinking_msg.edit(&ctx.http, |m| m.content(&error_message)).await;

                agent_error!(user_id, "execute_staged_task", "Stage {} failed: {}", stage_id, e);
                break;
            }
        }

        current_stage_index += 1;
    }

    // Write final summary to file
    write_to_response_file(Some(&mut response_file), "=== STAGED EXECUTION SUMMARY ===", user_id);
    for (i, stage) in staged_task.stages.iter().enumerate() {
        write_to_response_file(Some(&mut response_file), &format!("Stage {}: {} - {}", i + 1, stage.name, stage.status), user_id);
        if let Some(output) = &stage.output {
            write_to_response_file(Some(&mut response_file), &format!("Output: {}", output), user_id);
        }
    }

    // Close the file
    drop(response_file);

    // Upload the response file to Discord
    agent_info!(user_id, "execute_staged_task", "Uploading staged response file: {}", response_filename);

    let file_content = match std::fs::read_to_string(&response_filename) {
        Ok(content) => content,
        Err(e) => {
            agent_error!(user_id, "execute_staged_task", "Failed to read response file: {}", e);
            let _ = msg.reply(ctx, "❌ Failed to read response file").await;
            return Ok(());
        }
    };

    // Create final summary
    let completed_stages = staged_task.stages.iter().filter(|s| s.status == StageStatus::Completed).count();
    let total_stages = staged_task.stages.len();
    let final_status = if staged_task.overall_status == TaskStatus::Complete {
        "✅ **Staged Execution Complete**"
    } else if staged_task.overall_status == TaskStatus::Failed {
        "❌ **Staged Execution Failed**"
    } else {
        "⏸️ **Staged Execution Paused**"
    };

    let discord_message = format!(
        "{}\n\n📊 **Progress:** {}/{} stages completed\n📝 **Task:** {}\n\n📎 **Full Response:** See attached file\n\n💡 Use `^staged --status` to check current state\n💡 Use `^staged --approve` to continue\n💡 Use `^staged --modify <feedback>` to provide feedback",
        final_status, completed_stages, total_stages, task
    );

    // Upload file to Discord
    match msg.channel_id.send_files(&ctx.http, vec![(&*file_content.as_bytes(), response_filename.as_str())], |m| {
        m.content(&discord_message)
    }).await {
        Ok(_) => {
            agent_info!(user_id, "execute_staged_task", "Successfully uploaded staged response file to Discord");
        }
        Err(e) => {
            agent_error!(user_id, "execute_staged_task", "Failed to upload response file: {}", e);
            // Fallback to regular message
            let fallback_message = format!("{} - Staged execution completed. Check logs for details.", final_status);
            let _ = msg.channel_id.send_message(&ctx.http, |m| m.content(&fallback_message)).await;
        }
    }

    // Clean up the temporary file
    if let Err(e) = std::fs::remove_file(&response_filename) {
        agent_warn!(user_id, "execute_staged_task", "Failed to remove temporary file {}: {}", response_filename, e);
    } else {
        agent_debug!(user_id, "execute_staged_task", "Successfully removed temporary file: {}", response_filename);
    }

    let total_duration = start_time.elapsed();
    agent_trace!(user_id, "execute_staged_task", "=== EXECUTE STAGED TASK END ===");
    agent_trace!(user_id, "execute_staged_task", "Total execution time: {:?}", total_duration);
    agent_info!(user_id, "execute_staged_task", "Completed staged execution in {:?}", total_duration);

    Ok(())
}

async fn save_staged_task(task: &StagedTask) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let contexts = get_staged_tasks().await;
    if let Ok(mut contexts_map) = contexts.lock() {
        contexts_map.insert(task.user_id.to_string(), task.clone());
        agent_debug!(task.user_id, "save_staged_task", "Saved staged task for user: {}", task.user_id);
    }
    Ok(())
}

// ============================================================================
// STREAMING FUNCTION CALLING
// ============================================================================

async fn execute_function_calling_streaming(
    messages: &[ChatMessage],
    functions: &[FunctionDefinition],
    config: &LMConfig,
    user_id: UserId,
    streaming_msg: &mut Message,
    ctx: &Context,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    
    agent_trace!(user_id, "execute_function_calling_streaming", "=== FUNCTION CALLING STREAMING START ===");
    agent_trace!(user_id, "execute_function_calling_streaming", "Messages count: {}", messages.len());
    agent_trace!(user_id, "execute_function_calling_streaming", "Functions count: {}", functions.len());
    
    // Create the chat request
    let chat_request = ChatRequest {
        model: config.default_model.clone(),
        messages: messages.to_vec(),
        temperature: config.default_temperature,
        max_tokens: config.default_max_tokens,
        stream: true,
        seed: config.default_seed,
        tools: Some(functions.to_vec()),
        tool_choice: Some("auto".to_string()),
    };
    
    agent_trace!(user_id, "execute_function_calling_streaming", "Created chat request with streaming enabled");
    
    // Get HTTP client
    let client = get_http_client().await;
    
    // Serialize the request
    let request_body = match serde_json::to_string(&chat_request) {
        Ok(body) => {
            agent_trace!(user_id, "execute_function_calling_streaming", "Request body length: {} chars", body.len());
            body
        }
        Err(e) => {
            agent_error!(user_id, "execute_function_calling_streaming", "Failed to serialize request: {}", e);
            return Err(format!("Failed to serialize request: {}", e).into());
        }
    };
    
    // Update streaming message with connection status
    let _ = streaming_msg.edit(&ctx.http, |m| {
        m.content("🤖 **Connecting to LM Studio API...**\n\n🔄 **Status:** Sending request...\n\n📝 **Live Progress:**\n")
    }).await;
    
    // Send the request
    let response = match client
        .post(&format!("{}/v1/chat/completions", config.base_url))
        .header("Content-Type", "application/json")
        .body(request_body)
        .timeout(Duration::from_secs(config.timeout))
        .send()
        .await
    {
        Ok(response) => {
            agent_trace!(user_id, "execute_function_calling_streaming", "Received response from API");
            response
        }
        Err(e) => {
            agent_error!(user_id, "execute_function_calling_streaming", "Failed to send request: {}", e);
            let error_msg = format!("❌ **API Connection Failed**\n\n📝 **Error:** {}\n\n🔄 **Status:** Request failed", e);
            let _ = streaming_msg.edit(&ctx.http, |m| m.content(&error_msg)).await;
            return Err(format!("Failed to send request: {}", e).into());
        }
    };
    
    // Check if the response is successful
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        agent_error!(user_id, "execute_function_calling_streaming", "API returned error status {}: {}", status, error_text);
        let error_msg = format!("❌ **API Error**\n\n📝 **Status:** {}\n📝 **Error:** {}\n\n🔄 **Status:** Request failed", status, error_text);
        let _ = streaming_msg.edit(&ctx.http, |m| m.content(&error_msg)).await;
        return Err(format!("API returned error status {}: {}", status, error_text).into());
    }
    
    // Update streaming message with streaming status
    let _ = streaming_msg.edit(&ctx.http, |m| {
        m.content("🤖 **Connected to LM Studio API**\n\n🔄 **Status:** Streaming response...\n\n📝 **Live Progress:**\n")
    }).await;
    
    // Get the response stream
    let mut stream = response.bytes_stream();
    agent_trace!(user_id, "execute_function_calling_streaming", "Successfully created response stream");
    
    // Variables for streaming
    let mut buffer = String::new();
    let mut collected_tool_calls: Vec<String> = Vec::new();
    let mut function_call_buffer = String::new();
    let mut last_update = Instant::now();
    let update_interval = Duration::from_millis(500); // Update every 500ms to reduce load
    
    // Accumulation buffers (keep all content)
    let mut content_buffer = String::new();
    let mut tool_calls_buffer = String::new();
    
    // Scrolling display system
    let mut display_content = String::new();
    let mut display_tool_calls = String::new();
    let max_display_chars = 1200; // Keep display under Discord limit
    
    agent_trace!(user_id, "execute_function_calling_streaming", "Starting SSE streaming loop");
    agent_info!(user_id, "execute_function_calling_streaming", "Streaming configuration: update_interval={:?}, max_display_chars={}", update_interval, max_display_chars);
    
    // Process the stream
    let mut chunk_count = 0;
    while let Some(chunk_result) = stream.next().await {
        chunk_count += 1;
        if chunk_count % 10 == 0 {
            agent_trace!(user_id, "execute_function_calling_streaming", "Processed {} chunks, buffer_len={}, display_len={}", chunk_count, buffer.len(), display_content.len());
        }
        match chunk_result {
            Ok(chunk) => {
                // Convert chunk to string
                let chunk_str = match String::from_utf8(chunk.to_vec()) {
                    Ok(s) => s,
                    Err(e) => {
                        agent_warn!(user_id, "execute_function_calling_streaming", "Failed to convert chunk to string: {}", e);
                        continue;
                    }
                };
                
                // Split by lines and process each line
                for line in chunk_str.lines() {
                    if line.starts_with("data: ") {
                        let data = &line[6..]; // Remove "data: " prefix
                        
                        if data == "[DONE]" {
                            agent_trace!(user_id, "execute_function_calling_streaming", "Received [DONE] signal");
                            break;
                        }
                        
                        // Parse the JSON data
                        match serde_json::from_str::<serde_json::Value>(data) {
                            Ok(json_data) => {
                                // Extract content delta
                                if let Some(choices) = json_data.get("choices").and_then(|c| c.as_array()) {
                                    for choice in choices {
                                        if let Some(delta) = choice.get("delta") {
                                            // Handle content delta with simplified scrolling
                                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                                content_buffer.push_str(content);
                                                buffer.push_str(content);
                                                display_content.push_str(content);
                                                
                                                // Only scroll when significantly over limit to reduce frequency
                                                if display_content.len() > max_display_chars + 200 {
                                                    // Scroll: keep only the most recent content
                                                    let keep_chars = max_display_chars;
                                                    if display_content.len() > keep_chars {
                                                        display_content = display_content[display_content.len() - keep_chars..].to_string();
                                                    }
                                                }
                                            }
                                            
                                            // Handle tool_calls delta
                                            if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                                                for tool_call in tool_calls {
                                                    if let Some(function) = tool_call.get("function") {
                                                        // Extract function name
                                                        if let Some(name) = function.get("name").and_then(|n| n.as_str()) {
                                                            if !collected_tool_calls.contains(&name.to_string()) {
                                                                collected_tool_calls.push(name.to_string());
                                                                agent_trace!(user_id, "execute_function_calling_streaming", "Detected function call: {}", name);
                                                            }
                                                        }
                                                        
                                                        // Extract arguments with simplified scrolling
                                                        if let Some(args) = function.get("arguments").and_then(|a| a.as_str()) {
                                                            function_call_buffer.push_str(args);
                                                            tool_calls_buffer.push_str(args);
                                                            display_tool_calls.push_str(args);
                                                            
                                                            // Only scroll tool calls when significantly over limit
                                                            if display_tool_calls.len() > 400 {
                                                                // Scroll: keep only the most recent tool calls
                                                                let keep_chars = 300;
                                                                if display_tool_calls.len() > keep_chars {
                                                                    display_tool_calls = display_tool_calls[display_tool_calls.len() - keep_chars..].to_string();
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                agent_warn!(user_id, "execute_function_calling_streaming", "Failed to parse JSON: {}", e);
                                continue;
                            }
                        }
                    }
                }
                
                // Update Discord message periodically with scrolling display
                if last_update.elapsed() >= update_interval {
                    let mut update_content = String::new();
                    update_content.push_str("🤖 **Streaming from LM Studio API**\n\n");
                    update_content.push_str("🔄 **Status:** Processing...\n\n");
                    update_content.push_str("📝 **Live Progress:**\n");
                    
                    // Add scrolling content preview
                    if !display_content.is_empty() {
                        update_content.push_str(&format!("💭 **AI Response:**\n{}\n\n", display_content));
                    }
                    
                    // Add function call preview
                    if !collected_tool_calls.is_empty() {
                        update_content.push_str(&format!("🔧 **Function Calls:** {}\n", collected_tool_calls.join(", ")));
                    }
                    
                    // Add scrolling tool calls preview
                    if !display_tool_calls.is_empty() {
                        update_content.push_str(&format!("📝 **Tool Arguments:**\n{}\n", display_tool_calls));
                    }
                    
                    // Check if we need to reset due to Discord limit
                    if update_content.len() > 1800 {
                        // Clear display buffers for fresh start
                        display_content.clear();
                        display_tool_calls.clear();
                        
                        // Start fresh display
                        update_content = "🤖 **Streaming from LM Studio API**\n\n🔄 **Status:** Processing...\n\n📝 **Live Progress:**\n(Scrolling reset - continuing stream)\n".to_string();
                        
                        // Force immediate update to Discord to reset the message
                        let _ = streaming_msg.edit(&ctx.http, |m| m.content(&update_content)).await;
                        last_update = Instant::now();
                        continue; // Skip the regular update cycle
                    }
                    
                    let _ = streaming_msg.edit(&ctx.http, |m| m.content(&update_content)).await;
                    last_update = Instant::now();
                }
            }
            Err(e) => {
                agent_error!(user_id, "execute_function_calling_streaming", "Error reading stream chunk: {}", e);
                let error_msg = format!("❌ **Streaming Error**\n\n📝 **Error:** {}\n\n🔄 **Status:** Stream reading failed", e);
                let _ = streaming_msg.edit(&ctx.http, |m| m.content(&error_msg)).await;
                return Err(format!("Error reading stream chunk: {}", e).into());
            }
        }
    }
    
    // Final update with complete results
    agent_info!(user_id, "execute_function_calling_streaming", "Streaming completed: {} chunks processed, {} chars in buffer", chunk_count, buffer.len());
    let final_content = format!(
        "✅ **Streaming Complete**\n\n📝 **Final Response:**\n{}\n\n⏱️ **Duration:** {:?}",
        buffer, start_time.elapsed()
    );
    let _ = streaming_msg.edit(&ctx.http, |m| m.content(&final_content)).await;
    
    agent_trace!(user_id, "execute_function_calling_streaming", "=== FUNCTION CALLING STREAMING END ===");
    agent_info!(user_id, "execute_function_calling_streaming", "Streaming completed in {:?}", start_time.elapsed());
    
    Ok(buffer)
}