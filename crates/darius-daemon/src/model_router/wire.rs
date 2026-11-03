//! Encode domain turns to exact OpenAI-compatible wire DTOs.
use crate::opencode_affinity::session_token;
use darius_cognitive::{Message, ToolSpec, wire_messages, wire_tools};
use darius_core::providers::ProviderProfile;
use serde_json::{Value, json};

/// OpenRouter provider-routing prefs (3.6), sourced from the CLI
/// `[openrouter]` section (`sort`/`only`/`ignore`/`order`). Serialized as
/// the request-body `provider_routing` object per Hermes provider_routing,
/// and ONLY for the openrouter profile — every other profile drops it at
/// body-build (Nous Portal 400s on unknown body keys).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderRouting {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub only: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
}

impl ProviderRouting {
    /// True when no pref is set (transport then sends no key at all).
    pub fn is_empty(&self) -> bool {
        self.sort.as_deref().is_none_or(str::is_empty)
            && self.only.is_empty()
            && self.ignore.is_empty()
            && self.order.is_empty()
    }

    /// Body fragment, or `None` when empty. Empty-string `sort` counts as
    /// unset and is normalized away.
    fn body_value(&self) -> Option<Value> {
        if self.is_empty() {
            return None;
        }
        let mut map = serde_json::Map::new();
        if let Some(sort) = self.sort.as_deref().filter(|s| !s.is_empty()) {
            map.insert("sort".into(), Value::String(sort.into()));
        }
        if !self.only.is_empty() {
            map.insert("only".into(), json!(self.only));
        }
        if !self.ignore.is_empty() {
            map.insert("ignore".into(), json!(self.ignore));
        }
        if !self.order.is_empty() {
            map.insert("order".into(), json!(self.order));
        }
        Some(Value::Object(map))
    }
}

/// Nous request tags (3.7): every Nous-profile request carries product +
/// versioned-client attribution.
fn nous_tags() -> Value {
    json!([
        "product=darius",
        format!("client=darius-client-v{}", env!("CARGO_PKG_VERSION")),
    ])
}

/// Apply profile-gated body extras in place (3.6 + 3.7). The profile id —
/// not caller discipline — decides what is forwarded:
///
/// - `openrouter`: `provider_routing` from `routing` when non-empty.
/// - `nous`: `extra_body.tags` (product + versioned client) plus the
///   `session_id` sticky key (`session_token` of the Darius session id);
///   any `routing` prefs are NEVER forwarded.
/// - every other profile (and `None`): body untouched, routing dropped.
pub fn apply_body_extras(
    profile: Option<&ProviderProfile>,
    body: &mut Value,
    routing: Option<&ProviderRouting>,
    session_id: &str,
) {
    let Some(profile) = profile else {
        return;
    };
    let Some(map) = body.as_object_mut() else {
        return;
    };
    if profile.id == "openrouter" {
        if let Some(value) = routing.and_then(ProviderRouting::body_value) {
            map.insert("provider_routing".into(), value);
        }
        return;
    }
    if profile.id == "nous" {
        map.insert(
            "session_id".into(),
            Value::String(session_token(session_id)),
        );
        map.insert("extra_body".into(), json!({ "tags": nous_tags() }));
    }
}

pub fn encode_request(model: &str, msgs: &[Message], tools: &[ToolSpec], max_tokens: u64) -> Value {
    encode_request_with_profile(None, model, msgs, tools, max_tokens)
}

pub fn encode_stream_request(
    model: &str,
    msgs: &[Message],
    tools: &[ToolSpec],
    max_tokens: u64,
) -> Value {
    encode_stream_request_with_profile(None, model, msgs, tools, max_tokens)
}

/// Profile-aware encode: message quirks (3.3/3.4) are applied to the wire
/// values after [`wire_messages`] and before the body is built.
pub fn encode_request_with_profile(
    profile: Option<&ProviderProfile>,
    model: &str,
    msgs: &[Message],
    tools: &[ToolSpec],
    max_tokens: u64,
) -> Value {
    let mut messages = wire_messages(msgs);
    apply_message_quirks(profile, &mut messages);
    json!({
        "model": model,
        "messages": messages,
        "tools": wire_tools(tools),
        "max_tokens": max_tokens,
    })
}

pub fn encode_stream_request_with_profile(
    profile: Option<&ProviderProfile>,
    model: &str,
    msgs: &[Message],
    tools: &[ToolSpec],
    max_tokens: u64,
) -> Value {
    let mut val = encode_request_with_profile(profile, model, msgs, tools, max_tokens);
    if let Some(map) = val.as_object_mut() {
        map.insert("stream".into(), json!(true));
        map.insert("stream_options".into(), json!({"include_usage": true}));
    }
    val
}

/// Apply profile message quirks in place (copy-on-write: messages the active
/// quirks do not touch are never cloned or otherwise reallocated).
///
/// - 3.3 `strict_string_tool_content` (opencode-go): list-type `content` on
///   `role == "tool"` messages is flattened to a plain string before encode.
///   Only `role == "tool"` messages are visited, so user messages carrying
///   image parts pass through untouched.
/// - 3.4 `strip_tool_message_names` (nvidia): `name`/`tool_name` keys are
///   dropped from `role == "tool"` messages only.
pub fn apply_message_quirks(profile: Option<&ProviderProfile>, messages: &mut [Value]) {
    let Some(profile) = profile else {
        return;
    };
    if !profile.strict_string_tool_content && !profile.strip_tool_message_names {
        return;
    }
    for msg in messages.iter_mut() {
        if msg.get("role").and_then(|role| role.as_str()) != Some("tool") {
            continue;
        }
        if profile.strict_string_tool_content {
            flatten_tool_content(msg);
        }
        if profile.strip_tool_message_names
            && let Some(map) = msg.as_object_mut()
        {
            map.remove("name");
            map.remove("tool_name");
        }
    }
}

/// Flatten one tool message's list-type `content` to a plain string: bare
/// string parts are kept, `{"text": ...}` blocks contribute their text, and
/// non-text parts (no image parts belong in tool results) are dropped.
/// Non-array `content` is left untouched.
fn flatten_tool_content(msg: &mut Value) {
    let Some(content) = msg.get_mut("content") else {
        return;
    };
    let Value::Array(parts) = content else {
        return;
    };
    let mut flat = String::new();
    for part in parts.iter() {
        match part {
            Value::String(text) => flat.push_str(text),
            Value::Object(map) => {
                if let Some(Value::String(text)) = map.get("text") {
                    flat.push_str(text);
                }
            }
            _ => {}
        }
    }
    *content = Value::String(flat);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn strict_profile() -> &'static ProviderProfile {
        darius_core::providers::resolve("opencode-go").unwrap()
    }

    fn strip_profile() -> &'static ProviderProfile {
        darius_core::providers::resolve("nvidia").unwrap()
    }

    #[test]
    fn strict_profile_flattens_multipart_tool_content() {
        assert!(strict_profile().strict_string_tool_content);
        let mut messages = vec![json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "name": "read_file",
            "content": [
                {"type": "text", "text": "hello "},
                "world",
                {"type": "text", "text": "!"},
            ],
        })];
        apply_message_quirks(Some(strict_profile()), &mut messages);
        assert_eq!(messages[0]["content"], json!("hello world!"));
        assert_eq!(messages[0]["name"], json!("read_file"));
    }

    #[test]
    fn strict_profile_leaves_string_tool_content_alone() {
        let mut messages = vec![json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "name": "read_file",
            "content": "already flat",
        })];
        apply_message_quirks(Some(strict_profile()), &mut messages);
        assert_eq!(messages[0]["content"], json!("already flat"));
    }

    #[test]
    fn non_strict_profile_keeps_multipart_tool_content() {
        let mut messages = vec![json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "name": "read_file",
            "content": [{"type": "text", "text": "hello"}],
        })];
        let before = messages.clone();
        apply_message_quirks(Some(strip_profile()), &mut messages);
        assert_eq!(messages[0]["content"], before[0]["content"]);
    }

    #[test]
    fn strict_profile_leaves_user_images_untouched() {
        let mut messages = vec![json!({
            "role": "user",
            "content": [
                {"type": "text", "text": "look"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAA"}},
            ],
        })];
        let before = messages.clone();
        apply_message_quirks(Some(strict_profile()), &mut messages);
        assert_eq!(messages, before);
    }

    #[test]
    fn strip_profile_drops_tool_names_only() {
        assert!(strip_profile().strip_tool_message_names);
        let mut messages = vec![
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "name": "read_file",
                "tool_name": "read_file",
                "content": "out",
            }),
            json!({"role": "user", "name": "bob", "content": "hi"}),
        ];
        apply_message_quirks(Some(strip_profile()), &mut messages);
        assert!(messages[0].get("name").is_none());
        assert!(messages[0].get("tool_name").is_none());
        assert_eq!(messages[0]["tool_call_id"], json!("call_1"));
        assert_eq!(messages[0]["content"], json!("out"));
        assert_eq!(messages[1]["name"], json!("bob"));
    }

    #[test]
    fn non_strip_profile_keeps_tool_names() {
        let mut messages = vec![json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "name": "read_file",
            "content": "out",
        })];
        apply_message_quirks(Some(strict_profile()), &mut messages);
        assert_eq!(messages[0]["name"], json!("read_file"));
    }

    #[test]
    fn untouched_messages_are_not_reallocated() {
        // Pointer-stability proof for copy-on-write: the user message's
        // string buffer must keep its address through a strip pass.
        let mut messages = vec![
            json!({"role": "user", "content": "untouched user text"}),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "name": "read_file",
                "content": "out",
            }),
        ];
        let before = messages[0]["content"].as_str().unwrap().as_ptr();
        apply_message_quirks(Some(strip_profile()), &mut messages);
        let after = messages[0]["content"].as_str().unwrap().as_ptr();
        assert!(std::ptr::eq(before, after));
        assert_eq!(messages[0]["content"], json!("untouched user text"));
    }

    #[test]
    fn no_quirk_profile_is_a_no_op() {
        let mut messages = vec![
            json!({"role": "user", "content": "hi"}),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "name": "read_file",
                "content": [{"type": "text", "text": "x"}],
            }),
        ];
        let before = messages.clone();
        apply_message_quirks(None, &mut messages);
        assert_eq!(messages, before);
        let plain = darius_core::providers::resolve("openrouter").unwrap();
        assert!(!plain.strict_string_tool_content && !plain.strip_tool_message_names);
        apply_message_quirks(Some(plain), &mut messages);
        assert_eq!(messages, before);
    }

    #[test]
    fn profile_encode_threads_quirks_and_max_tokens() {
        let msgs = vec![Message::Tool {
            tool_call_id: "call_1".into(),
            name: "read_file".into(),
            content: "out".into(),
        }];
        let body = encode_request_with_profile(Some(strip_profile()), "m", &msgs, &[], 7);
        assert!(body["messages"][0].get("name").is_none());
        assert_eq!(body["max_tokens"], json!(7));
        // Legacy path preserves names (no profile quirks).
        let legacy = encode_request("m", &msgs, &[], 7);
        assert_eq!(legacy["messages"][0]["name"], json!("read_file"));
    }

    fn openrouter_profile() -> &'static darius_core::providers::ProviderProfile {
        darius_core::providers::resolve("openrouter").unwrap()
    }

    fn nous_profile() -> &'static darius_core::providers::ProviderProfile {
        darius_core::providers::resolve("nous").unwrap()
    }

    fn full_routing() -> ProviderRouting {
        ProviderRouting {
            sort: Some("throughput".into()),
            only: vec!["lambda".into()],
            ignore: vec!["azure".into()],
            order: vec!["lambda".into(), "deepinfra".into()],
        }
    }

    #[test]
    fn openrouter_forwards_full_routing_prefs() {
        let mut body = json!({"model": "m"});
        apply_body_extras(
            Some(openrouter_profile()),
            &mut body,
            Some(&full_routing()),
            "sess",
        );
        assert_eq!(
            body["provider_routing"],
            json!({
                "sort": "throughput",
                "only": ["lambda"],
                "ignore": ["azure"],
                "order": ["lambda", "deepinfra"],
            })
        );
        assert!(body.get("extra_body").is_none());
        assert!(body.get("session_id").is_none());
    }

    #[test]
    fn openrouter_partial_routing_omits_unset_keys() {
        let routing = ProviderRouting {
            sort: None,
            only: vec![],
            ignore: vec!["azure".into()],
            order: vec![],
        };
        let mut body = json!({"model": "m"});
        apply_body_extras(
            Some(openrouter_profile()),
            &mut body,
            Some(&routing),
            "sess",
        );
        assert_eq!(body["provider_routing"], json!({"ignore": ["azure"]}));
    }

    #[test]
    fn openrouter_empty_routing_sends_no_key() {
        for routing in [None, Some(ProviderRouting::default())] {
            let mut body = json!({"model": "m"});
            apply_body_extras(
                Some(openrouter_profile()),
                &mut body,
                routing.as_ref(),
                "sess",
            );
            assert!(body.get("provider_routing").is_none());
        }
    }

    #[test]
    fn nous_gets_tags_and_sticky_session_id_never_routing() {
        let expected_tags = json!([
            "product=darius",
            format!("client=darius-client-v{}", env!("CARGO_PKG_VERSION")),
        ]);
        let mut body = json!({"model": "m"});
        apply_body_extras(
            Some(nous_profile()),
            &mut body,
            Some(&full_routing()),
            "sess-7",
        );
        assert!(body.get("provider_routing").is_none());
        assert_eq!(body["extra_body"]["tags"], expected_tags);
        assert_eq!(
            body["session_id"],
            json!(crate::opencode_affinity::session_token("sess-7"))
        );
    }

    #[test]
    fn nous_session_id_stable_per_session() {
        let mut first = json!({"model": "m"});
        let mut second = json!({"model": "m"});
        let mut other = json!({"model": "m"});
        apply_body_extras(Some(nous_profile()), &mut first, None, "sess-same");
        apply_body_extras(Some(nous_profile()), &mut second, None, "sess-same");
        apply_body_extras(Some(nous_profile()), &mut other, None, "sess-other");
        assert_eq!(first["session_id"], second["session_id"]);
        assert_ne!(first["session_id"], other["session_id"]);
    }

    #[test]
    fn other_profiles_drop_routing_and_get_no_nous_extras() {
        for id in ["nvidia", "opencode-free", "kilocode"] {
            let profile = darius_core::providers::resolve(id).unwrap();
            let mut body = json!({"model": "m"});
            apply_body_extras(Some(profile), &mut body, Some(&full_routing()), "sess");
            assert!(body.get("provider_routing").is_none(), "{id}");
            assert!(body.get("extra_body").is_none(), "{id}");
            assert!(body.get("session_id").is_none(), "{id}");
        }
    }

    #[test]
    fn no_profile_leaves_body_untouched() {
        let mut body = json!({"model": "m"});
        let before = body.clone();
        apply_body_extras(None, &mut body, Some(&full_routing()), "sess");
        assert_eq!(body, before);
    }
}
