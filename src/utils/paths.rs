use std::path::Path;

/// Check if the given path is within the specified root directory.
/// Both paths are canonicalized before comparison to handle symlinks and relative paths.
pub fn is_within_root(path: &Path, root: &Path) -> bool {
    match (path.canonicalize(), root.canonicalize()) {
        (Ok(abs_path), Ok(abs_root)) => abs_path.starts_with(&abs_root),
        _ => false,
    }
}

pub fn is_file_exists(path: &Path) -> bool {
    path.exists() && path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_same_path() {
        let path = PathBuf::from("/tmp");
        assert!(is_within_root(&path, &path));
    }

    #[test]
    fn test_current_dir_within_itself() {
        if let Ok(cwd) = std::env::current_dir() {
            assert!(is_within_root(&cwd, &cwd));
        }
    }
}
