use reqwest;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

/// Error types for search operations
#[derive(Debug)]
pub enum SearchError {
    HttpError(reqwest::Error),
    ParseError(String),
    NoResults(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SearchError::HttpError(e) => write!(f, "HTTP request failed: {}", e),
            SearchError::ParseError(msg) => write!(f, "HTML parsing failed: {}", msg),
            SearchError::NoResults(msg) => write!(f, "No search results found: {}", msg),
        }
    }
}

impl Error for SearchError {}

impl From<reqwest::Error> for SearchError {
    fn from(error: reqwest::Error) -> Self {
        SearchError::HttpError(error)
    }
}

/// Represents a single search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub link: String,
    pub snippet: String,
}

impl SearchResult {
    pub fn new(title: String, link: String, snippet: String) -> Self {
        Self {
            title: title.trim().to_string(),
            link: link.trim().to_string(),
            snippet: snippet.trim().to_string(),
        }
    }
}

/// Perform a DuckDuckGo search and return top results
pub async fn ddg_search(query: &str) -> Result<Vec<SearchResult>, SearchError> {
    if query.trim().is_empty() {
        return Err(SearchError::NoResults("Empty search query provided".to_string()));
    }

    println!("🔍 Performing DuckDuckGo search for: '{}'", query);

    // Build DuckDuckGo search URL
    let encoded_query = urlencoding::encode(query);
    let search_url = format!("https://html.duckduckgo.com/html/?q={}", encoded_query);
    
    // Create HTTP client with timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .build()?;

    // Send GET request
    let response = client.get(&search_url).send().await?;
    
    if !response.status().is_success() {
        return Err(SearchError::ParseError(
            format!("HTTP request failed with status: {}", response.status())
        ));
    }

    let html_content = response.text().await?;
    
    // Parse HTML content
    let document = Html::parse_document(&html_content);
    
    // DuckDuckGo result selectors
    let result_selector = Selector::parse("div.result").map_err(|e| {
        SearchError::ParseError(format!("Failed to parse result selector: {:?}", e))
    })?;
    
    let title_selector = Selector::parse("a.result__a").map_err(|e| {
        SearchError::ParseError(format!("Failed to parse title selector: {:?}", e))
    })?;
    
    let snippet_selector = Selector::parse("a.result__snippet").map_err(|e| {
        SearchError::ParseError(format!("Failed to parse snippet selector: {:?}", e))
    })?;

    let mut search_results = Vec::new();
    let max_results = 5; // Limit to top 5 results

    // Extract search results
    for result_element in document.select(&result_selector).take(max_results) {
        let title_element = result_element.select(&title_selector).next();
        let snippet_element = result_element.select(&snippet_selector).next();

        if let (Some(title_elem), Some(snippet_elem)) = (title_element, snippet_element) {
            let title = title_elem.inner_html();
            let link = title_elem.value().attr("href").unwrap_or("").to_string();
            let snippet = snippet_elem.inner_html();

            // Clean up the extracted text (remove HTML tags, decode entities)
            let clean_title = clean_html_text(&title);
            let clean_snippet = clean_html_text(&snippet);
            let clean_link = if link.starts_with("//") {
                format!("https:{}", link)
            } else if link.starts_with("/") {
                format!("https://duckduckgo.com{}", link)
            } else {
                link
            };

            if !clean_title.is_empty() && !clean_link.is_empty() {
                search_results.push(SearchResult::new(
                    clean_title,
                    clean_link,
                    clean_snippet,
                ));
            }
        }
    }

    println!("🔍 Found {} search results", search_results.len());

    if search_results.is_empty() {
        return Err(SearchError::NoResults(format!(
            "No results found for query: '{}'", query
        )));
    }

    Ok(search_results)
}

/// Helper function to clean HTML text and decode HTML entities
fn clean_html_text(html: &str) -> String {
    // Remove HTML tags
    let document = Html::parse_fragment(html);
    let text = document.root_element().text().collect::<Vec<_>>().join(" ");
    
    // Basic HTML entity decoding
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

/// Format search results into a user-friendly string
pub fn format_search_results(results: &[SearchResult], query: &str) -> String {
    let mut formatted = format!("🔍 **Search Results for:** `{}`\n\n", query);
    
    for (index, result) in results.iter().enumerate() {
        formatted.push_str(&format!(
            "**{}. {}**\n{}\n🔗 {}\n\n",
            index + 1,
            result.title,
            if result.snippet.is_empty() { 
                "*No description available*" 
            } else { 
                &result.snippet 
            },
            result.link
        ));
    }
    
    if results.len() >= 5 {
        formatted.push_str("*Showing top 5 results*");
    }
    
    formatted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_html_text() {
        let html = "&lt;b&gt;Hello &amp; World&lt;/b&gt;";
        let cleaned = clean_html_text(html);
        assert_eq!(cleaned, "<b>Hello & World</b>");
    }

    #[test] 
    fn test_search_result_creation() {
        let result = SearchResult::new(
            "  Test Title  ".to_string(),
            "  https://example.com  ".to_string(),
            "  Test snippet  ".to_string(),
        );
        
        assert_eq!(result.title, "Test Title");
        assert_eq!(result.link, "https://example.com");
        assert_eq!(result.snippet, "Test snippet");
    }
} 