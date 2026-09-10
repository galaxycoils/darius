//! UiEvent emit helpers for the agent loop.
use crate::conversation::Message;
use crate::{DiffKind, DiffLine, EventSink, PermissionChoice, RunMetadata, UiEvent};
use darius_tools::ToolCall;
pub fn emit_header(sink: &dyn EventSink, meta: &RunMetadata, goal: &str) {
    sink.emit(UiEvent::Header {
        profile: meta.profile.clone(),
        model: meta.model.clone(),
        goal: goal.into(),
    });
}
pub fn emit_start(sink: &dyn EventSink, call: &ToolCall) {
    sink.emit(UiEvent::ToolStart {
        id: call.id.clone(),
        name: call.name.clone(),
        args_preview: darius_safety::redact_secrets(&format!("{:?}", call.arguments)),
    });
}
pub fn emit_end(
    sink: &dyn EventSink,
    id: &str,
    ok: bool,
    preview: &str,
    spilled: Option<std::path::PathBuf>,
) {
    sink.emit(UiEvent::ToolEnd {
        id: id.into(),
        ok,
        preview: darius_safety::redact_secrets(preview),
        spilled: spilled.map(|p| p.to_string_lossy().to_string()),
    });
}
pub fn deny_call(sink: &dyn EventSink, msgs: &mut Vec<Message>, call: &ToolCall) {
    const REASON: &str = "permission denied by user";
    sink.emit(UiEvent::PermissionResolved {
        id: call.id.clone(),
        choice: PermissionChoice::Deny,
    });
    msgs.push(Message::Tool {
        tool_call_id: call.id.clone(),
        name: call.name.clone(),
        content: REASON.into(),
    });
    emit_end(sink, &call.id, false, REASON, None);
}
pub fn emit_write_diff(sink: &dyn EventSink, call: &ToolCall, preview: &str) {
    if call.name != "write_file" {
        return;
    }
    let path = call.arguments.get("path").and_then(|v| v.as_str());

    const MAX_DIFF_LINES: usize = 200;
    // Parse diff from preview (format: "wrote N bytes to path\n+added\n-removed\n context")
    let lines = if let Some(diff_start) = preview.find('\n') {
        let mut parsed = Vec::new();
        let mut truncated = false;
        for line in preview[diff_start + 1..].lines() {
            if line.is_empty() {
                continue;
            }
            if parsed.len() >= MAX_DIFF_LINES {
                truncated = true;
                break;
            }
            let kind = match line.chars().next() {
                Some('+') => DiffKind::Add,
                Some('-') => DiffKind::Delete,
                Some(' ') => DiffKind::Context,
                _ => DiffKind::Context,
            };
            parsed.push(DiffLine {
                kind,
                old: None,
                new: None,
                text: line[1..].to_string(),
            });
        }
        if truncated {
            parsed.push(DiffLine {
                kind: DiffKind::Context,
                old: None,
                new: None,
                text: format!("... [diff truncated at {} lines]", MAX_DIFF_LINES),
            });
        }
        parsed
    } else {
        vec![]
    };

    sink.emit(UiEvent::Diff {
        file: path.unwrap_or("").into(),
        summary: preview.chars().take(200).collect(),
        lines,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    struct TestSink(std::sync::mpsc::Sender<UiEvent>);
    impl EventSink for TestSink {
        fn emit(&self, event: UiEvent) {
            let _ = self.0.send(event);
        }
    }

    #[test]
    fn emit_write_diff_caps_lines_at_200_with_marker() {
        let (tx, rx) = channel();
        let sink = TestSink(tx);
        let call = ToolCall {
            id: "call_1".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({"path": "test.txt"}),
        };
        let mut preview = "wrote 5000 bytes to test.txt".to_string();
        for i in 0..250 {
            preview.push_str(&format!("\n+line {i}"));
        }
        emit_write_diff(&sink, &call, &preview);
        let event = rx.try_recv().expect("expected Diff event");
        let UiEvent::Diff { file, lines, .. } = event else {
            panic!("expected Diff event");
        };
        assert_eq!(file, "test.txt");
        // 200 capped lines + 1 truncation marker line
        assert_eq!(lines.len(), 201);
        assert_eq!(lines[0].kind, DiffKind::Add);
        assert_eq!(lines[199].kind, DiffKind::Add);
        assert_eq!(lines[200].kind, DiffKind::Context);
        assert!(lines[200].text.contains("truncated at 200 lines"));
    }
}
