use std::env;
use std::fs::File;
use std::io::{self, Write};
use std::process;
use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::Utc;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio;

#[derive(Debug, Serialize, Deserialize)]
struct SubtitleTrack {
    baseUrl: Option<String>,
    name: Option<String>,
    languageCode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlayerResponse {
    captions: Option<Captions>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Captions {
    playerCaptionsRenderer: Option<PlayerCaptionsRenderer>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlayerCaptionsRenderer {
    captionTracks: Option<Vec<SubtitleTrack>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() != 2 {
        eprintln!("Usage: {} <youtube_url>", args[0]);
        eprintln!("Example: {} https://www.youtube.com/watch?v=dQw4w9WgXcQ", args[0]);
        process::exit(1);
    }
    
    let url = &args[1];
    
    if !url.contains("youtube.com") && !url.contains("youtu.be") {
        eprintln!("Error: Please provide a valid YouTube URL");
        process::exit(1);
    }
    
    println!("Scraping subtitles from: {}", url);
    
    // Extract video ID from URL
    let video_id = extract_video_id(url)
        .context("Could not extract video ID from URL")?;
    
    println!("Video ID: {}", video_id);
    
    // Try to get actual subtitles
    match get_youtube_subtitles(&video_id).await {
        Ok(subtitles) => {
            let filename = format!("subtitles_{}.srt", video_id);
            save_subtitles_to_file(&filename, &subtitles)?;
            println!("Subtitles saved to: {}", filename);
        }
        Err(e) => {
            eprintln!("Warning: Could not fetch actual subtitles: {}", e);
            eprintln!("Generating mock subtitles instead...");
            
            let mock_subtitles = generate_mock_subtitles(&video_id);
            let filename = format!("subtitles_{}_mock.txt", video_id);
            save_subtitles_to_file(&filename, &mock_subtitles)?;
            println!("Mock subtitles saved to: {}", filename);
        }
    }
    
    Ok(())
}

fn extract_video_id(url: &str) -> Result<String> {
    // Handle youtube.com URLs
    if url.contains("youtube.com/watch") {
        if let Some(v_index) = url.find("v=") {
            let start = v_index + 2;
            let end = url[start..].find('&').unwrap_or(url.len() - start) + start;
            return Ok(url[start..end].to_string());
        }
    }
    
    // Handle youtu.be URLs
    if url.contains("youtu.be/") {
        if let Some(slash_index) = url.rfind('/') {
            let start = slash_index + 1;
            let end = url[start..].find('?').unwrap_or(url.len() - start) + start;
            return Ok(url[start..end].to_string());
        }
    }
    
    Err(anyhow::anyhow!("Invalid YouTube URL format"))
}

async fn get_youtube_subtitles(video_id: &str) -> Result<String> {
    let client = Client::new();
    
    // First, get the video page to extract player response
    let video_url = format!("https://www.youtube.com/watch?v={}", video_id);
    let response = client.get(&video_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .send()
        .await
        .context("Failed to fetch video page")?;
    
    let html = response.text().await
        .context("Failed to get response text")?;
    
    // Extract player response from the HTML
    let player_response = extract_player_response(&html)
        .context("Failed to extract player response")?;
    
    // Parse the player response to get subtitle tracks
    let player_data: PlayerResponse = serde_json::from_str(&player_response)
        .context("Failed to parse player response")?;
    
    let subtitle_tracks = player_data
        .captions
        .and_then(|c| c.playerCaptionsRenderer)
        .and_then(|p| p.captionTracks)
        .ok_or_else(|| anyhow::anyhow!("No subtitle tracks found"))?;
    
    // Find English subtitles (or first available)
    let subtitle_track = subtitle_tracks
        .iter()
        .find(|track| {
            track.languageCode.as_deref() == Some("en") || 
            track.name.as_deref().map(|n| n.to_lowercase().contains("english")).unwrap_or(false)
        })
        .or_else(|| subtitle_tracks.first())
        .ok_or_else(|| anyhow::anyhow!("No subtitle tracks available"))?;
    
    let subtitle_url = subtitle_track.baseUrl
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No subtitle URL found"))?;
    
    // Fetch the actual subtitle data
    let subtitle_response = client.get(subtitle_url)
        .send()
        .await
        .context("Failed to fetch subtitle data")?;
    
    let subtitle_xml = subtitle_response.text().await
        .context("Failed to get subtitle XML")?;
    
    // Parse the XML subtitle data
    let subtitles = parse_subtitle_xml(&subtitle_xml)
        .context("Failed to parse subtitle XML")?;
    
    Ok(subtitles)
}

fn extract_player_response(html: &str) -> Result<String> {
    // Look for the player response in the HTML
    let pattern = r#"var ytInitialPlayerResponse = ({.+?});"#;
    let re = Regex::new(pattern)
        .context("Failed to compile regex pattern")?;
    
    if let Some(captures) = re.captures(html) {
        if let Some(player_response) = captures.get(1) {
            return Ok(player_response.as_str().to_string());
        }
    }
    
    Err(anyhow::anyhow!("Could not find player response in HTML"))
}

fn parse_subtitle_xml(xml: &str) -> Result<String> {
    // Simple XML parsing for subtitle data
    // This is a basic implementation - you might want to use a proper XML parser
    let mut subtitles = String::new();
    let mut counter = 1;
    
    // Split by <text> tags
    let parts: Vec<&str> = xml.split("<text").collect();
    
    for part in parts.iter().skip(1) {
        if let Some(text_start) = part.find('>') {
            if let Some(text_end) = part[text_start..].find("</text>") {
                let text_content = &part[text_start + 1..text_start + text_end];
                
                // Extract start time
                let start_match = Regex::new(r#"start="([^"]*)"#).unwrap();
                let start_time = if let Some(cap) = start_match.captures(part) {
                    cap.get(1).unwrap().as_str()
                } else {
                    "00:00:00,000"
                };
                
                // Extract duration
                let dur_match = Regex::new(r#"dur="([^"]*)"#).unwrap();
                let duration = if let Some(cap) = dur_match.captures(part) {
                    cap.get(1).unwrap().as_str()
                } else {
                    "3.0"
                };
                
                // Calculate end time
                let start_seconds: f64 = start_time.parse().unwrap_or(0.0);
                let dur_seconds: f64 = duration.parse().unwrap_or(3.0);
                let end_seconds = start_seconds + dur_seconds;
                
                let start_formatted = format_time(start_seconds);
                let end_formatted = format_time(end_seconds);
                
                // Decode HTML entities
                let decoded_text = decode_html_entities(text_content);
                
                subtitles.push_str(&format!("{}\n", counter));
                subtitles.push_str(&format!("{} --> {}\n", start_formatted, end_formatted));
                subtitles.push_str(&format!("{}\n\n", decoded_text));
                
                counter += 1;
            }
        }
    }
    
    if subtitles.is_empty() {
        return Err(anyhow::anyhow!("No subtitle content found in XML"));
    }
    
    Ok(subtitles)
}

fn format_time(seconds: f64) -> String {
    let hours = (seconds / 3600.0) as u32;
    let minutes = ((seconds % 3600.0) / 60.0) as u32;
    let secs = (seconds % 60.0) as u32;
    let millis = ((seconds % 1.0) * 1000.0) as u32;
    
    format!("{:02}:{:02}:{:02},{:03}", hours, minutes, secs, millis)
}

fn decode_html_entities(text: &str) -> String {
    let mut result = text.to_string();
    
    // Basic HTML entity decoding
    let entities = [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&nbsp;", " "),
    ];
    
    for (entity, replacement) in entities.iter() {
        result = result.replace(entity, replacement);
    }
    
    result
}

fn generate_mock_subtitles(video_id: &str) -> String {
    format!(
        "Subtitles for video: {}\n\
        \n\
        1\n\
        00:00:01,000 --> 00:00:04,000\n\
        Welcome to this YouTube video!\n\
        \n\
        2\n\
        00:00:04,000 --> 00:00:08,000\n\
        This is a demonstration of subtitle scraping.\n\
        \n\
        3\n\
        00:00:08,000 --> 00:00:12,000\n\
        In a real implementation, this would contain\n\
        the actual subtitles from the video.\n\
        \n\
        Note: This is a mock output. For actual subtitle scraping,\n\
        you would need to implement proper YouTube API integration\n\
        or web scraping techniques that comply with YouTube's terms of service.\n\
        \n\
        Video ID: {}\n\
        Generated at: {}\n",
        video_id,
        video_id,
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    )
}

fn save_subtitles_to_file(filename: &str, content: &str) -> Result<()> {
    let mut file = File::create(filename)
        .context("Failed to create output file")?;
    file.write_all(content.as_bytes())
        .context("Failed to write subtitles to file")?;
    Ok(())
}
