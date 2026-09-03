//! UiEvent emit helpers for the agent loop.
use crate::conversation::Message;
use crate::{EventSink, PermissionChoice, RunMetadata, UiEvent};
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
    sink.emit(UiEvent::Diff {
        file: path.unwrap_or("").into(),
        summary: preview.chars().take(200).collect(),
        lines: vec![],
    });
}
