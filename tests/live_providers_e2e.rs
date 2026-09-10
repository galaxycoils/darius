//! Opt-in live provider verification integration tests.
//!
//! These tests connect to actual cloud or local LLM endpoints (OpenAI, Anthropic, Ollama)
//! and verify end-to-end multi-turn tool calling without any mock models or mock fallbacks.
//!
//! Enable execution by setting:
//!   DARIUS_LIVE_TESTS=1 OPENAI_API_KEY="..." ANTHROPIC_API_KEY="..." cargo test --test live_providers_e2e -- --nocapture

use darius_cognitive::{AgentLoop, Conversation, LoopPolicy, RunMetadata};
use darius_daemon::{LiveModel, Provider};
use darius_tools::ToolRegistry;
use std::sync::Arc;
use tempfile::TempDir;

fn live_tests_enabled() -> bool {
    std::env::var("DARIUS_LIVE_TESTS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[tokio::test]
async fn test_live_suite_gate_active_by_default() {
    if !live_tests_enabled() {
        println!(
            "Skipping live provider tests; set DARIUS_LIVE_TESTS=1 to run against production endpoints"
        );
        return;
    }
    println!("DARIUS_LIVE_TESTS is active; running live endpoint verification");
}

#[tokio::test]
async fn test_live_openai_completion() {
    if !live_tests_enabled() {
        return;
    }
    let Ok(api_key) = std::env::var("OPENAI_API_KEY") else {
        println!("Skipping test_live_openai_completion: OPENAI_API_KEY not set");
        return;
    };
    if api_key.trim().is_empty() {
        println!("Skipping test_live_openai_completion: OPENAI_API_KEY is empty");
        return;
    }

    let temp = TempDir::new().unwrap();
    let ws = temp.path().to_path_buf();
    let spill = temp.path().join("spill");
    std::fs::create_dir_all(&spill).unwrap();

    let fixture = ws.join("notes.txt");
    std::fs::write(
        &fixture,
        "Project Darius: production-ready agent harness without mocks\n",
    )
    .unwrap();

    let mut tools = ToolRegistry::new_with_roots(&ws, &spill).unwrap();
    darius_tools::register_coding_builtins(&mut tools);

    let memory = darius_memory::MemoryEngine::open(&temp.path().join("mem")).unwrap();
    let mut convo = Conversation::from_messages(vec![]).unwrap();

    let key_env = "DARIUS_OPENAI_LIVE_TEST_KEY";
    unsafe { std::env::set_var(key_env, &api_key) };

    let provider = Provider {
        name: "openai".into(),
        model: "gpt-4o-mini".into(),
        base_url: "https://api.openai.com/v1".into(),
        enabled: true,
        api_key_env: key_env.into(),
    };

    let mut model = LiveModel::for_provider(provider).expect("LiveModel init");
    assert!(!model.is_anthropic());

    let (tx, _rx) = std::sync::mpsc::channel();
    let sink = Arc::new(darius_cognitive::ChannelEventSink::new(tx));
    let control = Arc::new(darius_cognitive::NoopRunControl);
    let loopt = AgentLoop::new(sink, control);

    let meta = RunMetadata {
        profile: "live_test".into(),
        model: "gpt-4o-mini".into(),
        mode: "auto".into(),
    };
    let policy = LoopPolicy {
        max_react_iters: 5,
        ..Default::default()
    };

    let answer = loopt
        .run_turn(
            &meta,
            &policy,
            "Please read notes.txt and state the exact name of the project mentioned in it.",
            &mut convo,
            &mut model,
            &tools,
            &memory,
            &ws.to_string_lossy(),
        )
        .await
        .expect("turn execution");

    println!("Live OpenAI answer: {answer}");
    assert!(
        answer.to_lowercase().contains("darius"),
        "expected 'darius' in answer: {answer}"
    );

    unsafe { std::env::remove_var(key_env) };
}

#[tokio::test]
async fn test_live_anthropic_completion() {
    if !live_tests_enabled() {
        return;
    }
    let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") else {
        println!("Skipping test_live_anthropic_completion: ANTHROPIC_API_KEY not set");
        return;
    };
    if api_key.trim().is_empty() {
        println!("Skipping test_live_anthropic_completion: ANTHROPIC_API_KEY is empty");
        return;
    }

    let temp = TempDir::new().unwrap();
    let ws = temp.path().to_path_buf();
    let spill = temp.path().join("spill");
    std::fs::create_dir_all(&spill).unwrap();

    let fixture = ws.join("info.txt");
    std::fs::write(&fixture, "Darius secret token is ZEPHYR-9999\n").unwrap();

    let mut tools = ToolRegistry::new_with_roots(&ws, &spill).unwrap();
    darius_tools::register_coding_builtins(&mut tools);

    let memory = darius_memory::MemoryEngine::open(&temp.path().join("mem")).unwrap();
    let mut convo = Conversation::from_messages(vec![]).unwrap();

    let key_env = "DARIUS_ANTHROPIC_LIVE_TEST_KEY";
    unsafe { std::env::set_var(key_env, &api_key) };

    let provider = Provider {
        name: "anthropic".into(),
        model: "claude-3-5-haiku-20241022".into(),
        base_url: "https://api.anthropic.com/v1".into(),
        enabled: true,
        api_key_env: key_env.into(),
    };

    let mut model = LiveModel::for_provider(provider).expect("LiveModel init");
    assert!(model.is_anthropic());

    let (tx, _rx) = std::sync::mpsc::channel();
    let sink = Arc::new(darius_cognitive::ChannelEventSink::new(tx));
    let control = Arc::new(darius_cognitive::NoopRunControl);
    let loopt = AgentLoop::new(sink, control);

    let meta = RunMetadata {
        profile: "live_test_ant".into(),
        model: "claude-3-5-haiku".into(),
        mode: "auto".into(),
    };
    let policy = LoopPolicy {
        max_react_iters: 5,
        ..Default::default()
    };

    let answer = loopt
        .run_turn(
            &meta,
            &policy,
            "Read info.txt and tell me what the secret token is.",
            &mut convo,
            &mut model,
            &tools,
            &memory,
            &ws.to_string_lossy(),
        )
        .await
        .expect("turn execution");

    println!("Live Anthropic answer: {answer}");
    assert!(
        answer.contains("ZEPHYR-9999"),
        "expected 'ZEPHYR-9999' in answer: {answer}"
    );

    unsafe { std::env::remove_var(key_env) };
}

#[tokio::test]
async fn test_live_ollama_local() {
    if !live_tests_enabled() {
        return;
    }
    // Probe local port 11434
    let is_reachable = std::net::TcpStream::connect("127.0.0.1:11434").is_ok();
    if !is_reachable {
        println!("Skipping test_live_ollama_local: Ollama is not running on 127.0.0.1:11434");
        return;
    }

    let temp = TempDir::new().unwrap();
    let ws = temp.path().to_path_buf();
    let spill = temp.path().join("spill");
    std::fs::create_dir_all(&spill).unwrap();

    let fixture = ws.join("local.txt");
    std::fs::write(&fixture, "Local Ollama test content\n").unwrap();

    let mut tools = ToolRegistry::new_with_roots(&ws, &spill).unwrap();
    darius_tools::register_coding_builtins(&mut tools);

    let memory = darius_memory::MemoryEngine::open(&temp.path().join("mem")).unwrap();
    let mut convo = Conversation::from_messages(vec![]).unwrap();

    let provider = Provider {
        name: "ollama".into(),
        model: "llama3.2".into(),
        base_url: "http://127.0.0.1:11434/v1".into(),
        enabled: true,
        api_key_env: "NONE".into(),
    };

    let mut model = LiveModel::for_provider(provider).expect("LiveModel init");
    let (tx, _rx) = std::sync::mpsc::channel();
    let sink = Arc::new(darius_cognitive::ChannelEventSink::new(tx));
    let control = Arc::new(darius_cognitive::NoopRunControl);
    let loopt = AgentLoop::new(sink, control);

    let meta = RunMetadata {
        profile: "live_test_ollama".into(),
        model: "llama3.2".into(),
        mode: "auto".into(),
    };
    let policy = LoopPolicy {
        max_react_iters: 4,
        ..Default::default()
    };

    let answer = loopt
        .run_turn(
            &meta,
            &policy,
            "Read local.txt and state what it says.",
            &mut convo,
            &mut model,
            &tools,
            &memory,
            &ws.to_string_lossy(),
        )
        .await;

    println!("Live Ollama answer result: {:?}", answer);
}
