use std::path::{Component, Path};

/// Check if the given path is within the specified root directory.
/// Both paths are canonicalized before comparison to handle symlinks and relative paths.
pub fn is_within_root(path: &Path, root: &Path) -> bool {
    let abs_root = match root.canonicalize() {
        Ok(root) => root,
        Err(_) => return false,
    };

    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        abs_root.join(path)
    };

    if let Ok(abs_path) = resolved.canonicalize() {
        return abs_path.starts_with(&abs_root);
    }

    // Fallback for non-existent targets: canonicalize the nearest existing ancestor.
    let mut ancestor = resolved.as_path();
    while let Some(parent) = ancestor.parent() {
        if parent.exists() {
            if let Ok(abs_parent) = parent.canonicalize() {
                if let Ok(tail) = resolved.strip_prefix(parent) {
                    if tail
                        .components()
                        .any(|component| matches!(component, Component::ParentDir))
                    {
                        return false;
                    }
                    let candidate = abs_parent.join(tail);
                    return candidate.starts_with(&abs_root);
                }
            }
            break;
        }
        ancestor = parent;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

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

    #[test]
    fn test_missing_file_within_root() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let missing = root.join("missing.txt");
        assert!(is_within_root(&missing, root));
    }

    #[test]
    fn test_missing_nested_file_within_root() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let nested = root.join("src");
        std::fs::create_dir_all(&nested).unwrap();
        let missing = nested.join("missing.txt");
        assert!(is_within_root(&missing, root));
    }

    #[test]
    fn test_path_traversal_outside_root_fails() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let outside = root.join("../outside.txt");
        assert!(!is_within_root(&outside, root));
    }
}
