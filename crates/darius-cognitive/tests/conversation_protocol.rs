//! Task 3.1 RED: correlated cancellable conversation protocol.
use darius_cognitive::{AsyncModel, Conversation, Message, ModelOutput, ToolSpec, TurnContext};
use darius_tools::ToolCall;

fn call(id: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: "read_file".into(),
        arguments: serde_json::json!({"path": "x.txt"}),
    }
}

fn valid_flow() -> Vec<Message> {
    vec![
        Message::System {
            content: "sys".into(),
        },
        Message::User {
            content: "hi".into(),
        },
        Message::Assistant {
            content: None,
            tool_calls: vec![call("c1")],
        },
        Message::Tool {
            tool_call_id: "c1".into(),
            name: "read_file".into(),
            content: "ok".into(),
        },
    ]
}

#[test]
fn conversation_protocol_roles_serialize_with_tags() {
    for (msg, role) in [
        (
            Message::System {
                content: "s".into(),
            },
            "system",
        ),
        (
            Message::User {
                content: "u".into(),
            },
            "user",
        ),
        (
            Message::Assistant {
                content: None,
                tool_calls: vec![],
            },
            "assistant",
        ),
        (
            Message::Tool {
                tool_call_id: "c1".into(),
                name: "read_file".into(),
                content: "r".into(),
            },
            "tool",
        ),
    ] {
        let v = serde_json::to_value(&msg).unwrap();
        assert_eq!(v.get("role").and_then(|r| r.as_str()), Some(role));
    }
}

#[test]
fn conversation_protocol_accepts_valid_tool_flow() {
    let convo = Conversation::from_messages(valid_flow()).unwrap();
    assert_eq!(convo.messages().len(), 4);
}

#[test]
fn conversation_protocol_rejects_empty_tool_id() {
    let mut msgs = valid_flow();
    msgs[2] = Message::Assistant {
        content: None,
        tool_calls: vec![call("")],
    };
    assert!(Conversation::from_messages(msgs).is_err());
}

#[test]
fn conversation_protocol_rejects_duplicate_tool_ids() {
    let mut msgs = valid_flow();
    msgs.push(Message::Assistant {
        content: None,
        tool_calls: vec![call("c1")],
    });
    assert!(Conversation::from_messages(msgs).is_err());
}

#[test]
fn conversation_protocol_rejects_orphan_tool_result() {
    let mut msgs = valid_flow();
    msgs[3] = Message::Tool {
        tool_call_id: "ghost".into(),
        name: "read_file".into(),
        content: "r".into(),
    };
    assert!(Conversation::from_messages(msgs).is_err());
}

#[test]
fn conversation_protocol_requires_result_for_every_call() {
    let msgs = valid_flow()[..3].to_vec();
    assert!(Conversation::from_messages(msgs).is_err());
}

#[test]
fn conversation_protocol_turn_context_fresh_sixty_second_deadline() {
    let ctx = TurnContext::new();
    assert!(!ctx.is_cancelled());
    assert!(!ctx.is_expired());
    let remaining = ctx.deadline_duration();
    assert!(remaining.as_secs() <= 60 && remaining.as_secs() >= 55);
    ctx.cancel();
    assert!(ctx.is_cancelled());
}

#[test]
fn conversation_protocol_tool_spec_shape() {
    let spec = ToolSpec {
        name: "read_file".into(),
        description: "read".into(),
        parameters: serde_json::json!({"type": "object"}),
    };
    assert_eq!(spec.name, "read_file");
}

#[test]
fn conversation_protocol_output_rejects_empty_tool_id() {
    let out = ModelOutput {
        content: None,
        tool_calls: vec![call("")],
    };
    assert!(out.validate().is_err());
}

#[test]
fn conversation_protocol_output_rejects_duplicate_tool_id() {
    let out = ModelOutput {
        content: None,
        tool_calls: vec![call("c1"), call("c1")],
    };
    assert!(out.validate().is_err());
}

#[tokio::test]
async fn conversation_protocol_legacy_adapter_delegates() {
    let mut adapter =
        darius_cognitive::LegacyModelAdapter::new(Box::new(darius_cognitive::MockModel::new(
            "{\"tasks\":[{\"title\":\"t\"}]}".into(),
            vec!["DONE".into()],
        )));
    let ctx = TurnContext::new();
    let out = adapter
        .complete(&valid_flow()[..2], &[], &ctx)
        .await
        .unwrap();
    assert!(out.content.is_some());
    assert!(out.tool_calls.is_empty());
}

#[tokio::test]
async fn conversation_protocol_legacy_adapter_honors_cancel() {
    let mut adapter = darius_cognitive::LegacyModelAdapter::new(Box::new(
        darius_cognitive::MockModel::new("p".into(), vec!["r".into()]),
    ));
    let ctx = TurnContext::new();
    ctx.cancel();
    assert!(
        adapter
            .complete(&valid_flow()[..2], &[], &ctx)
            .await
            .is_err()
    );
}
