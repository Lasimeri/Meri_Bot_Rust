# 📊 Modular Rank System Documentation

## Overview

The `^rank` command now supports a modular system prompt solution that allows users to specify different analysis types using command flags. This provides specialized analysis for different aspects of content evaluation.

## Supported Analysis Modes

### Default Mode
- **`^rank <url>`** - Comprehensive analysis (default behavior)

### Specialized Analysis Modes
- **`^rank -usability <url>`** - Usability-focused analysis
- **`^rank -quality <url>`** - Content quality analysis  
- **`^rank -accessibility <url>`** - Accessibility analysis
- **`^rank -seo <url>`** - SEO analysis
- **`^rank -performance <url>`** - Performance analysis
- **`^rank -security <url>`** - Security analysis
- **`^rank -educational <url>`** - Educational value analysis
- **`^rank -entertainment <url>`** - Entertainment value analysis
- **`^rank -technical <url>`** - Technical analysis
- **`^rank -comprehensive <url>`** - Explicit comprehensive analysis

### Help
- **`^rank -h`** - Display help information

## Analysis Mode Details

### 1. Usability Analysis (`-usability`)
**Focus:** User experience and ease of use
- User Interface Design (1-10)
- Navigation Ease (1-10)
- Information Architecture (1-10)
- User Flow (1-10)
- Mobile Responsiveness (1-10)
- Loading Speed (1-10)
- Overall User Experience (1-10)

### 2. Quality Analysis (`-quality`)
**Focus:** Content quality, accuracy, and value
- Content Accuracy (1-10)
- Writing Quality (1-10)
- Information Depth (1-10)
- Originality (1-10)
- Relevance (1-10)
- Completeness (1-10)
- Overall Value (1-10)

### 3. Accessibility Analysis (`-accessibility`)
**Focus:** Accessibility compliance and inclusive design
- WCAG Compliance (1-10)
- Screen Reader Compatibility (1-10)
- Keyboard Navigation (1-10)
- Color Contrast (1-10)
- Text Readability (1-10)
- Alternative Text (1-10)
- Overall Accessibility (1-10)

### 4. SEO Analysis (`-seo`)
**Focus:** Search engine optimization and discoverability
- Keyword Optimization (1-10)
- Meta Tags (1-10)
- Content Structure (1-10)
- Internal Linking (1-10)
- Page Speed (1-10)
- Mobile Optimization (1-10)
- Overall SEO Score (1-10)

### 5. Performance Analysis (`-performance`)
**Focus:** Speed, efficiency, and technical performance
- Loading Speed (1-10)
- Resource Optimization (1-10)
- Code Efficiency (1-10)
- Caching Strategy (1-10)
- Image Optimization (1-10)
- Server Response Time (1-10)
- Overall Performance (1-10)

### 6. Security Analysis (`-security`)
**Focus:** Security vulnerabilities and best practices
- Data Protection (1-10)
- Privacy Compliance (1-10)
- Secure Communication (1-10)
- Input Validation (1-10)
- Authentication Security (1-10)
- Vulnerability Assessment (1-10)
- Overall Security Score (1-10)

### 7. Educational Analysis (`-educational`)
**Focus:** Educational value and learning potential
- Learning Objectives (1-10)
- Content Clarity (1-10)
- Engagement Level (1-10)
- Knowledge Retention (1-10)
- Practical Application (1-10)
- Difficulty Level (1-10)
- Overall Educational Value (1-10)

### 8. Entertainment Analysis (`-entertainment`)
**Focus:** Entertainment value and engagement
- Engagement Level (1-10)
- Content Appeal (1-10)
- Entertainment Quality (1-10)
- Audience Retention (1-10)
- Creative Elements (1-10)
- Production Value (1-10)
- Overall Entertainment Score (1-10)

### 9. Technical Analysis (`-technical`)
**Focus:** Technical implementation and architecture
- Code Quality (1-10)
- Architecture Design (1-10)
- Scalability (1-10)
- Maintainability (1-10)
- Technology Stack (1-10)
- Best Practices (1-10)
- Overall Technical Excellence (1-10)

### 10. Comprehensive Analysis (`-comprehensive` or default)
**Focus:** Complete analysis covering all aspects
- Content Quality (1-10)
- Relevance (1-10)
- Engagement Potential (1-10)
- Educational Value (1-10)
- Technical Excellence (1-10)
- Usability (1-10)
- Accessibility (1-10)
- SEO Optimization (1-10)

## Usage Examples

```bash
# Comprehensive analysis (default)
^rank https://example.com

# Usability-focused analysis
^rank -usability https://example.com

# SEO analysis for a YouTube video
^rank -seo https://youtube.com/watch?v=example

# Accessibility analysis
^rank -accessibility https://example.com

# Performance analysis
^rank -performance https://example.com

# Educational value analysis
^rank -educational https://example.com

# Get help
^rank -h
```

## Custom Prompt Files

The system supports custom prompt files for each analysis mode. Create these files in your project directory:

### File Naming Convention
- **Generic prompts:** `rank_[mode]_prompt.txt`
- **YouTube-specific prompts:** `rank_[mode]_prompt_youtube.txt`
- **Example prompts:** `example_rank_[mode]_prompt.txt`

### Example File Names
```
rank_usability_prompt.txt
rank_accessibility_prompt.txt
rank_seo_prompt.txt
rank_performance_prompt.txt
rank_security_prompt.txt
rank_educational_prompt.txt
rank_entertainment_prompt.txt
rank_technical_prompt.txt
rank_comprehensive_prompt.txt
```

### File Search Order
The system searches for prompt files in this order:
1. `rank_[mode]_prompt_youtube.txt` (for YouTube content)
2. `rank_[mode]_prompt.txt` (generic)
3. `example_rank_[mode]_prompt.txt` (examples)
4. Built-in fallback prompts

### Example Custom Prompt File
Create `rank_usability_prompt.txt`:
```
You are a usability expert and UX analyst specializing in user experience evaluation. Your task is to analyze the provided content focusing specifically on usability aspects and provide detailed insights.

**Analysis Focus Areas:**
1. **User Interface Design (1-10)** - Visual clarity, layout organization, and design consistency
2. **Navigation Ease (1-10)** - How easily users can find what they're looking for
3. **Information Architecture (1-10)** - Logical content organization and structure
4. **User Flow (1-10)** - Smoothness of user journey and task completion
5. **Mobile Responsiveness (1-10)** - How well the content adapts to mobile devices
6. **Loading Speed (1-10)** - Performance and responsiveness
7. **Overall User Experience (1-10)** - Comprehensive usability score

**Your Analysis Should Include:**
- Specific examples from the content that demonstrate usability strengths/weaknesses
- User journey insights and potential pain points
- Actionable recommendations for improvement
- Accessibility considerations
- Mobile-first design principles
- Performance optimization suggestions

Provide a comprehensive usability analysis that would help content creators and developers improve the user experience.
```

## Technical Implementation

### Core Components

1. **RankingMode Enum** - Defines all available analysis modes
2. **RankCommandArgs Struct** - Holds parsed command arguments
3. **parse_rank_command()** - Parses user input and extracts mode/URL
4. **load_ranking_mode_prompt()** - Loads appropriate prompt for mode
5. **generate_fallback_prompt()** - Provides built-in prompts

### Key Features

- **Backward Compatibility** - Default behavior unchanged
- **Modular Design** - Easy to add new analysis modes
- **Multi-path Fallback** - Robust prompt file loading
- **YouTube Support** - Specialized prompts for video content
- **Help System** - Built-in help command
- **Error Handling** - Graceful error messages

### Adding New Analysis Modes

To add a new analysis mode:

1. **Add to RankingMode enum:**
```rust
pub enum RankingMode {
    // ... existing modes ...
    NewMode,  // Add your new mode
}
```

2. **Update RankingMode implementation:**
```rust
impl RankingMode {
    pub fn flag(&self) -> &'static str {
        match self {
            // ... existing cases ...
            RankingMode::NewMode => "-newmode",
        }
    }
    
    pub fn display_name(&self) -> &'static str {
        match self {
            // ... existing cases ...
            RankingMode::NewMode => "New Mode Analysis",
        }
    }
    
    pub fn description(&self) -> &'static str {
        match self {
            // ... existing cases ...
            RankingMode::NewMode => "Description of new mode",
        }
    }
}
```

3. **Update parse_rank_command():**
```rust
let mode = match words[0] {
    // ... existing cases ...
    "-newmode" => RankingMode::NewMode,
    _ => return Err(format!("Unknown flag: {}. Use -h for help.", words[0])),
};
```

4. **Update load_ranking_mode_prompt():**
```rust
let base_filename = match mode {
    // ... existing cases ...
    RankingMode::NewMode => "rank_newmode_prompt",
};
```

5. **Update generate_fallback_prompt():**
```rust
match mode {
    // ... existing cases ...
    RankingMode::NewMode => {
        format!("You are a new mode specialist...", content_type)
    },
}
```

## Benefits

1. **Specialized Analysis** - Each mode provides focused, relevant insights
2. **Flexible Usage** - Users can choose the analysis type they need
3. **Customizable** - Easy to create custom prompts for specific needs
4. **Extensible** - Simple to add new analysis modes
5. **User-Friendly** - Clear help system and error messages
6. **Backward Compatible** - Existing usage continues to work

## Error Handling

The system provides clear error messages for:
- Unknown flags
- Missing URLs after flags
- Invalid command syntax
- Help requests

Example error messages:
```
Unknown flag: -invalid. Use -h for help.
Missing URL after flag -usability. Usage: ^rank -usability <url>
```

## Future Enhancements

Potential future improvements:
- **Combined Modes** - Allow multiple flags (e.g., `-usability -accessibility`)
- **Custom Scoring** - Allow users to define custom evaluation criteria
- **Template System** - More sophisticated prompt templating
- **Analysis History** - Track and compare different analysis modes
- **Export Options** - Export analysis results in different formats 