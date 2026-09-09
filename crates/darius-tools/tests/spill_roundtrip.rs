use darius_tools::{ToolCall, ToolOutcome, ToolRegistry, register_coding_builtins};
#[test]
fn spill_read_recalls_marker_beyond_real_preview_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let spill = dir.path().join("tool_results");
    let mut registry = ToolRegistry::new_with_roots(dir.path(), &spill).unwrap();
    register_coding_builtins(&mut registry);
    let offset = 40_000;
    let content = format!("{}SPILL_TAIL_Z9", "x".repeat(offset));
    std::fs::write(dir.path().join("large.txt"), &content).unwrap();
    let read = ToolCall {
        id: "large-read".into(),
        name: "read_file".into(),
        arguments: serde_json::json!({"path":"large.txt","limit":50000}),
    };
    let ToolOutcome::Ok {
        preview,
        spilled_path: Some(path),
    } = registry.execute(&read)
    else {
        panic!("large result must spill with default 32 KiB ceiling");
    };
    assert!(preview.len() <= 32 * 1024);
    assert!(!preview.contains("SPILL_TAIL_Z9"));
    assert!(path.starts_with(spill.canonicalize().unwrap()));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    let recall = ToolCall {
        id: "recall-tail".into(),
        name: "spill_read".into(),
        arguments: serde_json::json!({"path":path,"offset":offset,"limit":100}),
    };
    let ToolOutcome::Ok {
        preview,
        spilled_path,
    } = registry.execute(&recall)
    else {
        panic!("actual spill path must be readable");
    };
    assert_eq!(preview, "SPILL_TAIL_Z9");
    assert!(spilled_path.is_none());
}
