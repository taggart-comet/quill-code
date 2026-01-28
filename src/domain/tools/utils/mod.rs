use std::path::Path;

pub fn short_filename(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "files".to_string();
    }
    let name = Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed);
    if name.is_empty() {
        "files".to_string()
    } else {
        name.to_string()
    }
}

pub fn short_label_from_path(label: &str) -> String {
    let trimmed = label.trim();
    if trimmed.contains('/') || trimmed.contains('\\') {
        short_filename(trimmed)
    } else {
        trimmed.to_string()
    }
}

// parsing what a tool is doing for displaying info to UI
pub fn short_words(input: &str, max_words: usize) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut words = trimmed.split_whitespace();
    let mut parts = Vec::new();
    for _ in 0..max_words {
        if let Some(word) = words.next() {
            parts.push(word);
        } else {
            break;
        }
    }
    let has_more = words.next().is_some();
    let base = parts.join(" ");
    if base.is_empty() {
        String::new()
    } else if has_more {
        format!("{}...", base)
    } else {
        base
    }
}

pub fn truncate_with_notice(text: &str, limit: usize) -> String {
    let current_len = text.chars().count();
    if current_len <= limit {
        return text.to_string();
    }

    let mut truncated: String = text.chars().take(limit).collect();
    truncated.push_str(&format!(
        "\n[output truncated to {} chars; refine your query to limit output]",
        limit
    ));
    truncated
}
