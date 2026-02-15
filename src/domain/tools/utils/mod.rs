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

pub fn is_read_only_command(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }

    if let Some(left) = strip_harmless_or_true(trimmed) {
        return is_read_only_command(left);
    }

    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut prev_escape = false;

    for (idx, ch) in trimmed.char_indices() {
        if prev_escape {
            prev_escape = false;
            continue;
        }

        match ch {
            '\\' => {
                prev_escape = true;
            }
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            '|' if !in_single && !in_double => {
                parts.push(trimmed[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    if !parts.is_empty() {
        parts.push(trimmed[start..].trim());
        if parts.len() != 2 {
            return false;
        }

        return parts.iter().all(|part| is_read_only_command(part));
    }

    if trimmed.contains(';')
        || trimmed.contains("&&")
        || trimmed.contains("||")
        || trimmed.contains('>')
        || trimmed.contains('<')
    {
        return false;
    }

    let mut parts = trimmed.split_whitespace();
    let first = parts.next().unwrap_or_default();
    let args: Vec<&str> = parts.collect();

    match first {
        "rg" | "grep" | "glob" | "cat" | "head" | "tail" | "less" | "more" | "wc" | "cut"
        | "sort" | "uniq" | "find" | "ls" | "tree" | "stat" | "file" | "awk" | "pwd" | "which"
        | "type" | "nl" => true,
        "sed" => args
            .iter()
            .any(|arg| *arg == "-n" || *arg == "--quiet" || *arg == "--silent"),
        _ => false,
    }
}

fn strip_harmless_or_true(command: &str) -> Option<&str> {
    let bytes = command.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    let mut prev_escape = false;
    let mut or_pos: Option<usize> = None;

    let mut idx = 0;
    while idx < bytes.len() {
        if prev_escape {
            prev_escape = false;
            idx += 1;
            continue;
        }

        match bytes[idx] {
            b'\\' => {
                prev_escape = true;
            }
            b'\'' if !in_double => {
                in_single = !in_single;
            }
            b'"' if !in_single => {
                in_double = !in_double;
            }
            b'|' if !in_single && !in_double => {
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'|' {
                    if or_pos.is_some() {
                        return None;
                    }
                    or_pos = Some(idx);
                    idx += 2;
                    continue;
                }
            }
            _ => {}
        }

        idx += 1;
    }

    let pos = or_pos?;
    if pos + 2 > command.len() {
        return None;
    }

    let left = command[..pos].trim();
    let right = command[pos + 2..].trim();

    if left.is_empty() {
        return None;
    }

    if right == "true" {
        return Some(left);
    }

    None
}