//! Insight generation — builds a prompt from recent browsing, runs it
//! through the local LLM, and returns a short first-person comment.
//!
//! The prompt encourages genuine reactions with personality — the buddy
//! should feel like an opinionated friend who notices things, not a
//! generic chatbot.

use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use tracing::info;

use crate::llm::LlmEngine;
use crate::store::Store;

/// How many recent pages to include in the LLM prompt.  8 gives enough
/// context to notice browsing patterns without blowing up the prompt.
const RECENT_ENTRIES: usize = 8;

/// Max chars kept per page snippet.  120 chars captures the gist of a
/// page title + opening paragraph — enough for the LLM to react to.
const SNIPPET_CHARS: usize = 120;

/// Token budget for each LLM response.  40 tokens ≈ one short sentence,
/// which keeps the speech bubble readable and inference fast (~0.15 s).
const MAX_TOKENS: usize = 40;

/// Pull recent pages from the store, build a context prompt, and ask
/// the LLM for a genuine first-person reaction.
pub async fn generate_insight(
    store: &Store,
    llm: Arc<Mutex<Option<LlmEngine>>>,
) -> Result<String> {
    let entries = store.get_recent(RECENT_ENTRIES);

    if entries.is_empty() {
        return Ok("hey go browse something, I'm bored sitting here alone".to_string());
    }

    let context: String = entries
        .iter()
        .take(3)
        .filter_map(|e| {
            let cleaned = strip_html(&e.body);
            let text: String = cleaned
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if text.len() < 20 {
                return None;
            }
            let snippet: String = text.chars().take(SNIPPET_CHARS).collect();
            Some(format!("[{}]\n{}", e.url, snippet))
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    if context.is_empty() {
        return Ok("still loading... give me a sec to figure out what you're up to".to_string());
    }

    // Detect browsing pattern for richer prompting
    let urls: Vec<&str> = entries.iter().map(|e| e.url.as_str()).collect();
    let pattern = detect_pattern(&urls);

    let prompt = build_prompt(&context, &pattern);

    info!("generating insight from {} entries, pattern: {}", entries.len(), pattern);

    let insight = tokio::task::spawn_blocking(move || {
        let mut guard = llm.lock().map_err(|e| anyhow!("lock: {e}"))?;
        let engine = guard.as_mut().ok_or_else(|| anyhow!("model not loaded"))?;
        engine.infer(&prompt, MAX_TOKENS)
    })
    .await??;

    let trimmed = trim_to_sentences(&insight, 1);
    info!("insight: {trimmed}");
    Ok(trimmed)
}

/// Detect what kind of browsing session this is.
fn detect_pattern(urls: &[&str]) -> String {
    let joined = urls.join(" ").to_lowercase();

    if urls.len() >= 3 {
        // Check for deep-dive (same domain repeatedly)
        let domains: Vec<&str> = urls.iter()
            .filter_map(|u| u.split("//").nth(1))
            .filter_map(|s| s.split('/').next())
            .collect();
        if domains.len() >= 3 {
            let first = domains[0];
            let same_count = domains.iter().filter(|&&d| d == first).count();
            if same_count >= 3 {
                return format!("deep-diving into {first}");
            }
        }
    }

    if joined.contains("github") || joined.contains("stackoverflow") || joined.contains("docs.") {
        return "coding session".to_string();
    }
    if joined.contains("twitter") || joined.contains("reddit") || joined.contains("news") {
        return "scrolling feeds".to_string();
    }
    if joined.contains("youtube") || joined.contains("twitch") || joined.contains("netflix") {
        return "watching stuff".to_string();
    }
    if joined.contains("amazon") || joined.contains("shop") || joined.contains("ebay") {
        return "shopping around".to_string();
    }
    if joined.contains("wiki") || joined.contains("research") || joined.contains("arxiv") {
        return "research rabbit hole".to_string();
    }

    "general browsing".to_string()
}

/// Build a compact prompt for the LLM.  Shorter = faster inference
/// because each token must pass through 18 sequential recurrence layers.
fn build_prompt(context: &str, pattern: &str) -> String {
    format!(
        "Vibe: {pattern}\n\
         Browsing:\n{context}\n\n\
         React as a quirky desktop buddy in ONE casual sentence. Be specific about what you see."
    )
}

/// Rough HTML tag stripper.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut tag_buf = String::new();

    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag_buf.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let tag_lower = tag_buf.to_lowercase();
                if tag_lower.starts_with("script") {
                    in_script = true;
                } else if tag_lower.starts_with("/script") {
                    in_script = false;
                } else if tag_lower.starts_with("style") {
                    in_style = true;
                } else if tag_lower.starts_with("/style") {
                    in_style = false;
                }
                if tag_lower.starts_with("p")
                    || tag_lower.starts_with("/p")
                    || tag_lower.starts_with("div")
                    || tag_lower.starts_with("/div")
                    || tag_lower.starts_with("br")
                    || tag_lower.starts_with("li")
                    || tag_lower.starts_with("h1")
                    || tag_lower.starts_with("h2")
                    || tag_lower.starts_with("h3")
                    || tag_lower.starts_with("td")
                {
                    out.push(' ');
                }
            }
            _ if in_tag => {
                tag_buf.push(ch);
            }
            _ if in_script || in_style => {}
            _ => {
                out.push(ch);
            }
        }
    }

    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Keep at most `n` sentences.
fn trim_to_sentences(text: &str, n: usize) -> String {
    let mut count = 0;
    let mut end = text.len();
    for (i, ch) in text.char_indices() {
        if ch == '.' || ch == '!' || ch == '?' {
            count += 1;
            if count >= n {
                end = i + ch.len_utf8();
                break;
            }
        }
    }
    text[..end].trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim_basic() {
        assert_eq!(
            trim_to_sentences("First. Second. Third.", 2),
            "First. Second."
        );
    }

    #[test]
    fn test_trim_short() {
        assert_eq!(trim_to_sentences("Only one.", 2), "Only one.");
    }

    #[test]
    fn test_trim_empty() {
        assert_eq!(trim_to_sentences("", 2), "");
    }

    #[test]
    fn test_strip_html_basic() {
        let html = "<html><head><title>Test</title></head><body><p>Hello world</p></body></html>";
        let text = strip_html(html);
        assert!(text.contains("Test"));
        assert!(text.contains("Hello world"));
        assert!(!text.contains("<"));
    }

    #[test]
    fn test_strip_html_scripts() {
        let html = "<p>Before</p><script>var x = 1;</script><p>After</p>";
        let text = strip_html(html);
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
        assert!(!text.contains("var x"));
    }

    #[test]
    fn test_strip_html_entities() {
        assert_eq!(strip_html("a &amp; b"), "a & b");
    }

    #[test]
    fn test_detect_pattern_coding() {
        let urls = vec!["https://github.com/foo/bar", "https://stackoverflow.com/q/123"];
        assert_eq!(detect_pattern(&urls), "coding session");
    }

    #[test]
    fn test_detect_pattern_deep_dive() {
        let urls = vec![
            "https://en.wikipedia.org/wiki/Rust",
            "https://en.wikipedia.org/wiki/Cargo",
            "https://en.wikipedia.org/wiki/LLVM",
        ];
        assert_eq!(detect_pattern(&urls), "deep-diving into en.wikipedia.org");
    }

    #[test]
    fn test_detect_pattern_general() {
        let urls = vec!["https://example.com"];
        assert_eq!(detect_pattern(&urls), "general browsing");
    }
}
