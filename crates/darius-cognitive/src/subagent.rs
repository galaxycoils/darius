use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Subagent identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubagentId(pub String);

impl SubagentId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for SubagentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SubagentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for SubagentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for SubagentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SubError {
    #[error("subagent not found: {0}")]
    NotFound(String),
    #[error("subagent terminated")]
    Terminated,
    #[error("schema violation: {0}")]
    SchemaViolation(String),
    #[error("execution error: {0}")]
    Execution(String),
}

/// Options for spawning a subagent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnOpts {
    pub max_iters: usize,
    pub output_schema: Option<serde_json::Value>,
}

impl Default for SpawnOpts {
    fn default() -> Self {
        Self {
            max_iters: 25,
            output_schema: None,
        }
    }
}

/// Partial or final execution result from a subagent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartialResult {
    pub completed_steps: usize,
    pub transcript: Vec<String>,
    pub output: Option<String>,
    pub terminated: bool,
}

/// Runtime interface for managing child subagents.
pub trait SubagentRuntime: Send + Sync {
    fn list_running(&self) -> Vec<SubagentId>;
    fn steer(&self, id: &SubagentId, message: &str) -> Result<(), SubError>;
    fn stop(&self, id: &SubagentId) -> Result<PartialResult, SubError>;
    fn spawn(&self, prompt: &str, opts: SpawnOpts) -> Result<SubagentId, SubError>;
}

/// In-process state of a managed subagent.
#[derive(Debug, Clone)]
struct SubagentState {
    id: SubagentId,
    #[allow(dead_code)]
    prompt: String,
    opts: SpawnOpts,
    completed_steps: usize,
    transcript: Vec<String>,
    output: Option<String>,
    running: bool,
}

/// Default in-process subagent runtime coordinator.
#[derive(Debug, Default, Clone)]
pub struct LocalSubagentRuntime {
    agents: Arc<Mutex<HashMap<String, SubagentState>>>,
}

impl LocalSubagentRuntime {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Record a step completion for an agent.
    pub fn record_step(&self, id: &SubagentId, step_summary: &str) -> Result<(), SubError> {
        let mut map = self.agents.lock().unwrap();
        let agent = map
            .get_mut(&id.0)
            .ok_or_else(|| SubError::NotFound(id.0.clone()))?;
        if !agent.running {
            return Err(SubError::Terminated);
        }
        agent.completed_steps += 1;
        agent.transcript.push(step_summary.to_string());
        Ok(())
    }

    /// Complete a subagent execution with an output payload, validating against schema if configured.
    pub fn complete(&self, id: &SubagentId, output: &str) -> Result<(), SubError> {
        let mut map = self.agents.lock().unwrap();
        let agent = map
            .get_mut(&id.0)
            .ok_or_else(|| SubError::NotFound(id.0.clone()))?;

        if let Some(ref schema) = agent.opts.output_schema {
            validate_json_schema(output, schema)?;
        }

        agent.output = Some(output.to_string());
        agent.running = false;
        Ok(())
    }
}

impl SubagentRuntime for LocalSubagentRuntime {
    fn list_running(&self) -> Vec<SubagentId> {
        let map = self.agents.lock().unwrap();
        map.values()
            .filter(|a| a.running)
            .map(|a| a.id.clone())
            .collect()
    }

    fn steer(&self, id: &SubagentId, message: &str) -> Result<(), SubError> {
        let mut map = self.agents.lock().unwrap();
        let agent = map
            .get_mut(&id.0)
            .ok_or_else(|| SubError::NotFound(id.0.clone()))?;
        if !agent.running {
            return Err(SubError::Terminated);
        }
        agent
            .transcript
            .push(format!("[steer]: {}", message.trim()));
        Ok(())
    }

    fn stop(&self, id: &SubagentId) -> Result<PartialResult, SubError> {
        let mut map = self.agents.lock().unwrap();
        let agent = map
            .get_mut(&id.0)
            .ok_or_else(|| SubError::NotFound(id.0.clone()))?;

        agent.running = false;
        Ok(PartialResult {
            completed_steps: agent.completed_steps,
            transcript: agent.transcript.clone(),
            output: agent.output.clone(),
            terminated: true,
        })
    }

    fn spawn(&self, prompt: &str, opts: SpawnOpts) -> Result<SubagentId, SubError> {
        let id = SubagentId::new();
        let state = SubagentState {
            id: id.clone(),
            prompt: prompt.to_string(),
            opts,
            completed_steps: 0,
            transcript: vec![format!("[prompt]: {}", prompt)],
            output: None,
            running: true,
        };
        let mut map = self.agents.lock().unwrap();
        map.insert(id.0.clone(), state);
        Ok(id)
    }
}

/// Validate raw JSON string against a basic JSON schema definition.
pub fn validate_json_schema(raw_json: &str, schema: &serde_json::Value) -> Result<(), SubError> {
    let value: serde_json::Value = serde_json::from_str(raw_json)
        .map_err(|e| SubError::SchemaViolation(format!("invalid JSON payload: {e}")))?;

    validate_value(&value, schema)
}

fn validate_value(value: &serde_json::Value, schema: &serde_json::Value) -> Result<(), SubError> {
    if let Some(expected_type) = schema.get("type").and_then(|v| v.as_str()) {
        match expected_type {
            "object" if !value.is_object() => {
                return Err(SubError::SchemaViolation(format!(
                    "expected object, got {value:?}"
                )));
            }
            "array" if !value.is_array() => {
                return Err(SubError::SchemaViolation(format!(
                    "expected array, got {value:?}"
                )));
            }
            "string" if !value.is_string() => {
                return Err(SubError::SchemaViolation(format!(
                    "expected string, got {value:?}"
                )));
            }
            "number" if !value.is_number() => {
                return Err(SubError::SchemaViolation(format!(
                    "expected number, got {value:?}"
                )));
            }
            "boolean" if !value.is_boolean() => {
                return Err(SubError::SchemaViolation(format!(
                    "expected boolean, got {value:?}"
                )));
            }
            _ => {}
        }
    }

    if let Some(required) = schema.get("required").and_then(|v| v.as_array())
        && let Some(obj) = value.as_object()
    {
        for req_key in required {
            if let Some(k) = req_key.as_str()
                && !obj.contains_key(k)
            {
                return Err(SubError::SchemaViolation(format!(
                    "missing required field '{k}'"
                )));
            }
        }
    }

    if let (Some(props), Some(obj)) = (
        schema.get("properties").and_then(|v| v.as_object()),
        value.as_object(),
    ) {
        for (prop_key, prop_schema) in props {
            if let Some(prop_val) = obj.get(prop_key) {
                validate_value(prop_val, prop_schema)?;
            }
        }
    }

    Ok(())
}

/// Register subagent tools on a tool registry.
pub fn register_subagent_builtins(
    registry: &mut darius_tools::ToolRegistry,
    runtime: Arc<dyn SubagentRuntime>,
) {
    let r_spawn = runtime.clone();
    registry.register_with_risk(
        "subagent_spawn",
        darius_tools::ToolRisk::Mutating,
        move |call| {
            let prompt = call
                .arguments
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if prompt.is_empty() {
                return Err(darius_tools::ToolError::InvalidArgs(
                    "prompt required".into(),
                ));
            }

            let max_iters = call
                .arguments
                .get("max_iters")
                .and_then(|v| v.as_u64())
                .unwrap_or(25) as usize;

            let output_schema = call.arguments.get("schema").cloned();

            let id = r_spawn
                .spawn(
                    prompt,
                    SpawnOpts {
                        max_iters,
                        output_schema,
                    },
                )
                .map_err(|e| darius_tools::ToolError::Task(e.to_string()))?;

            Ok(darius_tools::ToolOutcome::Ok {
                preview: format!("spawned subagent {}", id),
                spilled_path: None,
            })
        },
    );

    let r_list = runtime.clone();
    registry.register_with_risk(
        "subagent_list",
        darius_tools::ToolRisk::ReadOnly,
        move |_| {
            let list = r_list.list_running();
            let formatted = list
                .iter()
                .map(|id| format!("- {}", id))
                .collect::<Vec<_>>()
                .join("\n");
            let preview = if formatted.is_empty() {
                "no running subagents".to_string()
            } else {
                format!("Running subagents ({}):\n{}", list.len(), formatted)
            };
            Ok(darius_tools::ToolOutcome::Ok {
                preview,
                spilled_path: None,
            })
        },
    );

    let r_steer = runtime.clone();
    registry.register_with_risk(
        "subagent_steer",
        darius_tools::ToolRisk::Mutating,
        move |call| {
            let id_str = call
                .arguments
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let message = call
                .arguments
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if id_str.is_empty() || message.is_empty() {
                return Err(darius_tools::ToolError::InvalidArgs(
                    "id and message required".into(),
                ));
            }

            r_steer
                .steer(&SubagentId(id_str.to_string()), message)
                .map_err(|e| darius_tools::ToolError::Task(e.to_string()))?;

            Ok(darius_tools::ToolOutcome::Ok {
                preview: format!("steered subagent {id_str}"),
                spilled_path: None,
            })
        },
    );

    let r_stop = runtime;
    registry.register_with_risk(
        "subagent_stop",
        darius_tools::ToolRisk::Mutating,
        move |call| {
            let id_str = call
                .arguments
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if id_str.is_empty() {
                return Err(darius_tools::ToolError::InvalidArgs("id required".into()));
            }

            let partial = r_stop
                .stop(&SubagentId(id_str.to_string()))
                .map_err(|e| darius_tools::ToolError::Task(e.to_string()))?;

            Ok(darius_tools::ToolOutcome::Ok {
                preview: format!(
                    "stopped subagent {id_str} (completed steps: {})",
                    partial.completed_steps
                ),
                spilled_path: None,
            })
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_list_steer_stop_lifecycle() {
        let runtime = LocalSubagentRuntime::new();
        let opts = SpawnOpts {
            max_iters: 10,
            output_schema: None,
        };

        let id = runtime.spawn("analyze security logs", opts).unwrap();
        let running = runtime.list_running();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0], id);

        runtime.record_step(&id, "inspected log batch 1").unwrap();
        runtime.steer(&id, "focus on auth failures").unwrap();

        let partial = runtime.stop(&id).unwrap();
        assert!(partial.terminated);
        assert_eq!(partial.completed_steps, 1);
        assert!(
            partial
                .transcript
                .iter()
                .any(|t| t.contains("auth failures"))
        );

        assert!(runtime.list_running().is_empty());
        assert_eq!(runtime.steer(&id, "another"), Err(SubError::Terminated));
    }

    #[test]
    fn output_schema_validation_accepts_valid_and_rejects_invalid() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["summary", "score"],
            "properties": {
                "summary": { "type": "string" },
                "score": { "type": "number" }
            }
        });

        let valid_payload = r#"{"summary": "all tests green", "score": 98.5}"#;
        assert!(validate_json_schema(valid_payload, &schema).is_ok());

        let missing_field_payload = r#"{"summary": "missing score"}"#;
        match validate_json_schema(missing_field_payload, &schema) {
            Err(SubError::SchemaViolation(msg)) => assert!(msg.contains("score")),
            other => panic!("expected SchemaViolation, got {other:?}"),
        }

        let wrong_type_payload = r#"{"summary": 12345, "score": 98.5}"#;
        match validate_json_schema(wrong_type_payload, &schema) {
            Err(SubError::SchemaViolation(msg)) => assert!(msg.contains("expected string")),
            other => panic!("expected SchemaViolation, got {other:?}"),
        }

        let non_json = "not json at all";
        assert!(matches!(
            validate_json_schema(non_json, &schema),
            Err(SubError::SchemaViolation(_))
        ));
    }

    #[test]
    fn test_subagent_builtins_tool_execution() {
        let temp_dir =
            std::env::temp_dir().join(format!("darius_sub_tool_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let mut registry = darius_tools::ToolRegistry::new(&temp_dir).unwrap();
        let runtime = Arc::new(LocalSubagentRuntime::new());
        register_subagent_builtins(&mut registry, runtime);

        // 1. Spawn subagent
        let spawn_call = darius_tools::ToolCall {
            id: "c1".into(),
            name: "subagent_spawn".into(),
            arguments: serde_json::json!({
                "prompt": "run security audit",
                "max_iters": 15
            }),
        };
        let outcome = registry.execute(&spawn_call);
        let spawned_preview = match outcome {
            darius_tools::ToolOutcome::Ok { preview, .. } => preview,
            darius_tools::ToolOutcome::Err { message } => panic!("spawn failed: {message}"),
        };
        assert!(spawned_preview.contains("spawned subagent"));

        // Extract ID from preview
        let id_str = spawned_preview.strip_prefix("spawned subagent ").unwrap();

        // 2. List subagents
        let list_call = darius_tools::ToolCall {
            id: "c2".into(),
            name: "subagent_list".into(),
            arguments: serde_json::json!({}),
        };
        let outcome = registry.execute(&list_call);
        match outcome {
            darius_tools::ToolOutcome::Ok { preview, .. } => {
                assert!(preview.contains(id_str));
            }
            darius_tools::ToolOutcome::Err { message } => panic!("list failed: {message}"),
        }

        // 3. Steer subagent
        let steer_call = darius_tools::ToolCall {
            id: "c3".into(),
            name: "subagent_steer".into(),
            arguments: serde_json::json!({
                "id": id_str,
                "message": "check openssl version"
            }),
        };
        let outcome = registry.execute(&steer_call);
        assert!(matches!(outcome, darius_tools::ToolOutcome::Ok { .. }));

        // 4. Stop subagent
        let stop_call = darius_tools::ToolCall {
            id: "c4".into(),
            name: "subagent_stop".into(),
            arguments: serde_json::json!({
                "id": id_str
            }),
        };
        let outcome = registry.execute(&stop_call);
        match outcome {
            darius_tools::ToolOutcome::Ok { preview, .. } => {
                assert!(preview.contains("stopped subagent"));
            }
            darius_tools::ToolOutcome::Err { message } => panic!("stop failed: {message}"),
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
