use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub enum FileInsertError {
    FileNotFound(String),
    InvalidLineNumber(String),
    IoError(String),
}

#[derive(Debug, Clone)]
pub enum FileRemoveError {
    FileNotFound(String),
    InvalidLineNumber(String),
    InvalidCount(String),
    IoError(String),
}

#[derive(Debug, Clone)]
pub enum FileReplaceError {
    FileNotFound(String),
    InvalidLineNumber(String),
    InvalidRange(String),
    IoError(String),
}

impl std::fmt::Display for FileInsertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileInsertError::FileNotFound(path) => write!(f, "File not found: {}", path),
            FileInsertError::InvalidLineNumber(msg) => write!(f, "Invalid line number: {}", msg),
            FileInsertError::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::fmt::Display for FileRemoveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileRemoveError::FileNotFound(path) => write!(f, "File not found: {}", path),
            FileRemoveError::InvalidLineNumber(msg) => write!(f, "Invalid line number: {}", msg),
            FileRemoveError::InvalidCount(msg) => write!(f, "Invalid count: {}", msg),
            FileRemoveError::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::fmt::Display for FileReplaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileReplaceError::FileNotFound(path) => write!(f, "File not found: {}", path),
            FileReplaceError::InvalidLineNumber(msg) => write!(f, "Invalid line number: {}", msg),
            FileReplaceError::InvalidRange(msg) => write!(f, "Invalid range: {}", msg),
            FileReplaceError::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

/// Insert content at a specific line in a file.
/// Line numbers are 1-based.
pub fn insert_content(file_path: &str, target_line: usize, content: &str) -> Result<(), FileInsertError> {
    if !Path::new(file_path).exists() {
        return Err(FileInsertError::FileNotFound(file_path.to_string()));
    }

    if target_line == 0 {
        return Err(FileInsertError::InvalidLineNumber("Line numbers must be 1-based".to_string()));
    }

    let file_content = fs::read_to_string(file_path)
        .map_err(|e| FileInsertError::IoError(format!("Failed to read file: {}", e)))?;

    let mut lines: Vec<String> = file_content.lines().map(|s| s.to_string()).collect();
    
    // Convert 1-based line number to 0-based index
    let insert_index = if target_line > lines.len() {
        lines.len()
    } else {
        target_line - 1
    };

    // Split the content to insert into lines
    let content_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    
    // Insert the content
    for (i, line) in content_lines.iter().enumerate() {
        lines.insert(insert_index + i, line.clone());
    }

    let new_content = lines.join("\n");
    fs::write(file_path, new_content)
        .map_err(|e| FileInsertError::IoError(format!("Failed to write file: {}", e)))?;

    Ok(())
}

/// Remove a specified number of lines starting from a target line.
/// Line numbers are 1-based.
pub fn remove_lines(file_path: &str, target_line: usize, count: usize) -> Result<(), FileRemoveError> {
    if !Path::new(file_path).exists() {
        return Err(FileRemoveError::FileNotFound(file_path.to_string()));
    }

    if target_line == 0 {
        return Err(FileRemoveError::InvalidLineNumber("Line numbers must be 1-based".to_string()));
    }

    if count == 0 {
        return Err(FileRemoveError::InvalidCount("Count must be greater than 0".to_string()));
    }

    let file_content = fs::read_to_string(file_path)
        .map_err(|e| FileRemoveError::IoError(format!("Failed to read file: {}", e)))?;

    let mut lines: Vec<String> = file_content.lines().map(|s| s.to_string()).collect();
    
    // Convert 1-based line number to 0-based index
    let start_index = if target_line > lines.len() {
        return Err(FileRemoveError::InvalidLineNumber(
            format!("Line {} is beyond file length ({})", target_line, lines.len())
        ));
    } else {
        target_line - 1
    };

    let end_index = (start_index + count).min(lines.len());

    // Remove the lines
    lines.drain(start_index..end_index);

    let new_content = lines.join("\n");
    fs::write(file_path, new_content)
        .map_err(|e| FileRemoveError::IoError(format!("Failed to write file: {}", e)))?;

    Ok(())
}

/// Replace content between start_line and end_line (inclusive) with new content.
/// Line numbers are 1-based.
pub fn replace_lines(file_path: &str, start_line: usize, end_line: usize, content: &str) -> Result<(), FileReplaceError> {
    if !Path::new(file_path).exists() {
        return Err(FileReplaceError::FileNotFound(file_path.to_string()));
    }

    if start_line == 0 || end_line == 0 {
        return Err(FileReplaceError::InvalidLineNumber("Line numbers must be 1-based".to_string()));
    }

    if start_line > end_line {
        return Err(FileReplaceError::InvalidRange(
            format!("Start line ({}) must be <= end line ({})", start_line, end_line)
        ));
    }

    let file_content = fs::read_to_string(file_path)
        .map_err(|e| FileReplaceError::IoError(format!("Failed to read file: {}", e)))?;

    let mut lines: Vec<String> = file_content.lines().map(|s| s.to_string()).collect();
    
    // Convert 1-based line numbers to 0-based indices
    let start_index = if start_line > lines.len() {
        return Err(FileReplaceError::InvalidLineNumber(
            format!("Start line {} is beyond file length ({})", start_line, lines.len())
        ));
    } else {
        start_line - 1
    };

    let end_index = if end_line > lines.len() {
        lines.len()
    } else {
        end_line
    };

    // Split the replacement content into lines
    let replacement_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    // Remove the old lines and insert the new ones
    lines.drain(start_index..end_index);
    for (i, line) in replacement_lines.iter().enumerate() {
        lines.insert(start_index + i, line.clone());
    }

    let new_content = lines.join("\n");
    fs::write(file_path, new_content)
        .map_err(|e| FileReplaceError::IoError(format!("Failed to write file: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn create_test_file(dir: &TempDir, name: &str, content: &str) -> String {
        let file_path = dir.path().join(name);
        fs::write(&file_path, content).unwrap();
        file_path.to_str().unwrap().to_string()
    }

    #[test]
    fn test_insert_content() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = create_test_file(&dir, "test.txt", "line 1\nline 2\nline 3");

        insert_content(&file_path, 2, "inserted line\nanother inserted line").unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line 1\ninserted line\nanother inserted line\nline 2\nline 3");
    }

    #[test]
    fn test_remove_lines() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = create_test_file(&dir, "test.txt", "line 1\nline 2\nline 3\nline 4\nline 5");

        remove_lines(&file_path, 2, 2).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line 1\nline 4\nline 5");
    }

    #[test]
    fn test_replace_lines() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = create_test_file(&dir, "test.txt", "line 1\nline 2\nline 3\nline 4");

        replace_lines(&file_path, 2, 3, "new line 2\nnew line 3").unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line 1\nnew line 2\nnew line 3\nline 4");
    }

    #[test]
    fn test_insert_at_end() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = create_test_file(&dir, "test.txt", "line 1\nline 2");

        insert_content(&file_path, 10, "line 3").unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line 1\nline 2\nline 3");
    }
}