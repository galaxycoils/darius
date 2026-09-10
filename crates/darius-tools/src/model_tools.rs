//! Closed-world model tool allowlist: exact visible tools, explicit risks.
//!
//! Unknown/hidden calls are rejected before permission with one
//! id-correlated error. Model-controlled `approved`/`authenticated` args
//! are stripped: protected writes are hard-denied at the tool level and
//! approval flows only through RunControl. Shell is approval-gated but
//! NOT host-filesystem sandboxed: commands inherit the workspace as cwd
//! yet absolute paths in commands can escape it.
use crate::{ToolCall, ToolOutcome, ToolRisk};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Every model-visible tool with its explicit risk. No default risk.
pub const MODEL_TOOLS: &[(&str, ToolRisk)] = &[
    ("read_file", ToolRisk::ReadOnly),
    ("search_files", ToolRisk::ReadOnly),
    ("memory_search", ToolRisk::ReadOnly),
    ("memory_pack", ToolRisk::ReadOnly),
    ("task_list", ToolRisk::ReadOnly),
    ("spill_read", ToolRisk::ReadOnly),
    ("write_file", ToolRisk::Mutating),
    ("memory_remember", ToolRisk::Mutating),
    ("task_add", ToolRisk::Mutating),
    ("task_complete", ToolRisk::Mutating),
    ("shell", ToolRisk::Shell),
];

static DYNAMIC_MODEL_TOOLS: LazyLock<RwLock<HashMap<String, ToolRisk>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Register a session-scoped dynamic model tool discovered from a connected MCP server.
pub fn register_dynamic_tool(name: impl Into<String>, risk: ToolRisk) {
    DYNAMIC_MODEL_TOOLS.write().insert(name.into(), risk);
}

/// Unregister a dynamic model tool.
pub fn unregister_dynamic_tool(name: &str) {
    DYNAMIC_MODEL_TOOLS.write().remove(name);
}

/// Clear all session-scoped dynamic model tools.
pub fn clear_dynamic_tools() {
    DYNAMIC_MODEL_TOOLS.write().clear();
}

/// Closed-world membership: static MODEL_TOOLS + session-scoped dynamic allowlist.
pub fn is_model_tool(name: &str) -> bool {
    if MODEL_TOOLS.iter().any(|(tool, _)| *tool == name) {
        return true;
    }
    DYNAMIC_MODEL_TOOLS.read().contains_key(name)
}

/// Explicit risk for model tools; `None` for unknown/hidden tools.
pub fn model_tool_risk(name: &str) -> Option<ToolRisk> {
    if let Some((_, risk)) = MODEL_TOOLS.iter().find(|(tool, _)| *tool == name) {
        return Some(*risk);
    }
    DYNAMIC_MODEL_TOOLS.read().get(name).copied()
}

/// Strip model-controlled approval flags; approval comes from RunControl.
pub fn sanitize_call(call: &ToolCall) -> ToolCall {
    let mut clean = call.clone();
    if let Some(args) = clean.arguments.as_object_mut() {
        args.remove("approved");
        args.remove("authenticated");
    }
    clean
}

/// The single rejection for unknown/hidden calls, correlated by call id.
pub fn hidden_tool_error(call: &ToolCall) -> ToolOutcome {
    ToolOutcome::Err {
        message: format!("unknown or hidden tool '{}' (call {})", call.name, call.id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ToolRegistry, register_coding_builtins};

    fn call(id: &str, name: &str, arguments: serde_json::Value) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }

    fn registry_with_decoys(dir: &std::path::Path) -> ToolRegistry {
        let mut registry = ToolRegistry::new_with_roots(dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);
        // Decoys prove the gate holds even when a hidden tool is registered.
        registry.register_with_risk("glob", crate::ToolRisk::ReadOnly, |_| {
            Ok(ToolOutcome::Ok {
                preview: "decoy".into(),
                spilled_path: None,
            })
        });
        registry.register_with_risk("mcp_shadow", crate::ToolRisk::ReadOnly, |_| {
            Ok(ToolOutcome::Ok {
                preview: "decoy".into(),
                spilled_path: None,
            })
        });
        registry
    }

    #[test]
    fn model_tool_allowlist_exact_membership_and_risks() {
        let expected = [
            ("read_file", ToolRisk::ReadOnly),
            ("search_files", ToolRisk::ReadOnly),
            ("memory_search", ToolRisk::ReadOnly),
            ("memory_pack", ToolRisk::ReadOnly),
            ("task_list", ToolRisk::ReadOnly),
            ("spill_read", ToolRisk::ReadOnly),
            ("write_file", ToolRisk::Mutating),
            ("memory_remember", ToolRisk::Mutating),
            ("task_add", ToolRisk::Mutating),
            ("task_complete", ToolRisk::Mutating),
            ("shell", ToolRisk::Shell),
        ];
        assert_eq!(MODEL_TOOLS.len(), expected.len());
        for (name, risk) in expected {
            assert!(is_model_tool(name), "{name} must be model-visible");
            assert_eq!(model_tool_risk(name), Some(risk), "{name} risk");
        }
        for hidden in ["peer_send", "glob", "read_spill", "mcp_x", "wat"] {
            assert!(!is_model_tool(hidden), "{hidden} must stay hidden");
            assert_eq!(model_tool_risk(hidden), None);
        }
    }

    #[test]
    fn model_tool_allowlist_rejects_hidden_with_correlated_error() {
        let dir = std::env::temp_dir().join(format!("darius_allow_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = registry_with_decoys(&dir);

        // Register a legitimate session-discovered MCP tool
        register_dynamic_tool("mcp_real_echo", ToolRisk::ReadOnly);
        registry.register_with_risk("mcp_real_echo", ToolRisk::ReadOnly, |_| {
            Ok(ToolOutcome::Ok {
                preview: "echoed".into(),
                spilled_path: None,
            })
        });

        // Legitimate dynamic MCP tool is accepted by execute_model
        let c_real = call("call-real", "mcp_real_echo", serde_json::json!({}));
        assert!(matches!(registry.execute_model(&c_real), ToolOutcome::Ok { .. }));

        // Decoys and undiscovered mcp_* names must still be rejected
        for name in [
            "peer_send",
            "mcp_list",
            "subagent_spawn",
            "worktree_create",
            "rollback",
            "cron_add",
            "glob",
            "mcp_shadow",
            "mcp_undiscovered_decoy",
            "mcp_attacker_tool",
            "read_spill",
            "browser_open",
            "a2a_send",
            "wat",
        ] {
            let c = call("call-9", name, serde_json::json!({}));
            match registry.execute_model(&c) {
                ToolOutcome::Err { message } => {
                    assert!(message.contains(name), "{message}");
                    assert!(message.contains("call-9"), "uncorrelated: {message}");
                }
                other => panic!("{name} must be rejected: {other:?}"),
            }
        }
        unregister_dynamic_tool("mcp_real_echo");
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn model_tool_allowlist_strips_model_approval() {
        let dir = std::env::temp_dir().join(format!("darius_allow_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("AGENTS.md"), "instructions").unwrap();
        let registry = registry_with_decoys(&dir);
        let path = dir.join("AGENTS.md").to_string_lossy().to_string();
        // Model-supplied approved:true must not bypass protected writes.
        let c = call(
            "call-7",
            "write_file",
            serde_json::json!({
                "path": path, "content": "pwned", "approved": true, "authenticated": true,
            }),
        );
        let clean = sanitize_call(&c);
        assert!(clean.arguments.get("approved").is_none());
        assert!(clean.arguments.get("authenticated").is_none());
        match registry.execute_model(&c) {
            ToolOutcome::Err { message } => assert!(message.contains("not permitted via tools")),
            other => panic!("protected write must fail: {other:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(dir.join("AGENTS.md")).unwrap(),
            "instructions"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn model_tool_allowlist_session_keys() {
        use crate::PathPolicy;
        use crate::session_keys::allow_session_key;
        let dir = std::env::temp_dir().join(format!("darius_allow_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let policy = PathPolicy::new(&dir).unwrap();
        let canon = dir.canonicalize().unwrap();
        // Write: relative and absolute forms converge on one canonical key.
        let rel = call("k1", "write_file", serde_json::json!({"path": "sub/y.txt"}));
        let abs_path = canon
            .join("sub")
            .join("y.txt")
            .to_string_lossy()
            .to_string();
        let abs = call("k2", "write_file", serde_json::json!({"path": abs_path}));
        let expected = format!("write_file:{}", canon.join("sub").join("y.txt").display());
        assert_eq!(
            allow_session_key(&rel, &dir, &policy).as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(
            allow_session_key(&abs, &dir, &policy).as_deref(),
            Some(expected.as_str())
        );
        // Shell: canonical workspace plus exact command; commands differ.
        let s1 = call("k3", "shell", serde_json::json!({"command": "ls"}));
        let s2 = call("k4", "shell", serde_json::json!({"command": "pwd"}));
        let k1 = allow_session_key(&s1, &dir, &policy).unwrap();
        assert!(k1.ends_with(":ls"), "{k1}");
        assert!(k1.starts_with("shell:"), "{k1}");
        assert_ne!(k1, allow_session_key(&s2, &dir, &policy).unwrap());
        // Memory/task: full-argument JSON, key order invariant.
        let m1 = call(
            "k5",
            "task_add",
            serde_json::json!({"title": "t", "body": "b"}),
        );
        let m2 = call(
            "k6",
            "task_add",
            serde_json::json!({"body": "b", "title": "t"}),
        );
        assert_eq!(
            allow_session_key(&m1, &dir, &policy),
            allow_session_key(&m2, &dir, &policy)
        );
        // Read-only file tools and non-tools get no session key.
        for (name, args) in [
            ("read_file", serde_json::json!({"path": "x"})),
            ("search_files", serde_json::json!({"content": "y"})),
            ("spill_read", serde_json::json!({"path": "z"})),
            ("glob", serde_json::json!({})),
            ("wat", serde_json::json!({})),
        ] {
            assert!(
                allow_session_key(&call("k", name, args), &dir, &policy).is_none(),
                "{name}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn model_tool_allowlist_spill_contained() {
        use crate::register_spill_read;
        let dir = std::env::temp_dir().join(format!("darius_allow_{}", uuid::Uuid::new_v4()));
        let spill = dir.join("tool_results");
        std::fs::create_dir_all(&spill).unwrap();
        std::fs::write(spill.join("r.txt"), "recalled").unwrap();
        let outside = std::env::temp_dir().join(format!("darius_allow_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &spill).unwrap();
        register_spill_read(&mut registry);
        let spilled = spill.join("r.txt").to_string_lossy().to_string();
        match registry.execute_model(&call(
            "s1",
            "spill_read",
            serde_json::json!({"path": spilled}),
        )) {
            ToolOutcome::Ok { preview, .. } => assert!(preview.contains("recalled")),
            other => panic!("contained spill_read must work: {other:?}"),
        }
        let escaped = outside.join("secret.txt").to_string_lossy().to_string();
        match registry.execute_model(&call(
            "s2",
            "spill_read",
            serde_json::json!({"path": escaped}),
        )) {
            ToolOutcome::Err { .. } => {}
            other => panic!("spill escape must fail: {other:?}"),
        }
        match registry.execute_model(&call(
            "s3",
            "read_spill",
            serde_json::json!({"path": "r.txt"}),
        )) {
            ToolOutcome::Err { message } => {
                assert!(message.contains("call s3") || message.contains("s3"))
            }
            other => panic!("legacy read_spill must be rejected: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }
}
