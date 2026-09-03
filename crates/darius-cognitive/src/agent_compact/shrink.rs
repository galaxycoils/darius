//! UTF-8-safe content shrinking with an explicit compaction marker.
const MARKER: &str = "\n[compacted]";

pub(super) fn shrink(content: &mut String, target: usize) {
    if content.len() <= target {
        return;
    }
    if target < MARKER.len() {
        content.clear();
        return;
    }
    let mut end = target - MARKER.len();
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    content.truncate(end);
    content.push_str(MARKER);
}
