//! Bounded recursive directory walk (never follows symlinks).
use std::path::PathBuf;

/// Walk `root` depth-first, collecting files where `accept` holds, up to
/// `cap` hits, `max_visited` entries, and `max_depth` levels.
pub fn walk(
    root: PathBuf,
    cap: usize,
    max_visited: usize,
    max_depth: usize,
    accept: impl Fn(&PathBuf) -> bool,
) -> (Vec<String>, usize) {
    let (mut out, mut stack) = (Vec::new(), vec![(root, 0usize)]);
    let mut visited = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if visited >= max_visited {
                    return (out, visited);
                }
                visited += 1;
                let path: PathBuf = entry.path();
                if path.is_symlink() {
                    continue;
                }
                if path.is_dir() {
                    if depth < max_depth {
                        stack.push((path, depth + 1));
                    }
                } else if accept(&path) {
                    out.push(path.display().to_string());
                    if out.len() >= cap {
                        return (out, visited);
                    }
                }
            }
        }
    }
    (out, visited)
}
