use super::tool_evidence::*;
use super::*;
#[test]
fn tui_memory_tools_roundtrip_persists_approved_record() {
    let server = Server::scripted(false, |r| match results(&r).len() {
        0 => reply(
            "remember",
            "memory_remember",
            json!({"body":"AGENT_MEM_UNIQUE","kind":"fact"}),
        ),
        1 => reply(
            "search",
            "memory_search",
            json!({"text":"AGENT_MEM_UNIQUE"}),
        ),
        2 => json!({"role":"assistant","content":"done"}),
        _ => panic!("unexpected model continuation"),
    });
    let mut h = Harness::new(&server.url, None);
    h.submit("remember then search");
    h.permit(PermissionChoice::AllowOnce);
    finish(&mut h);
    server.requests.recv_timeout(BOUND).unwrap();
    let remembered = server.requests.recv_timeout(BOUND).unwrap();
    assert!(
        result(&remembered, "remember", "memory_remember").contains("remembered: AGENT_MEM_UNIQUE")
    );
    let searched = server.requests.recv_timeout(BOUND).unwrap();
    assert!(result(&searched, "search", "memory_search").contains("AGENT_MEM_UNIQUE"));
    h.shutdown();
    let profile = h.temp.path().join("home/profiles/test");
    assert!(profile.join("memory.db").is_file());
    let memory = darius_memory::MemoryEngine::open(&profile).unwrap();
    assert_eq!(memory.record_count().unwrap(), 1);
    let found = memory
        .search(&darius_memory::SearchQuery {
            text: Some("AGENT_MEM_UNIQUE".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].body, "AGENT_MEM_UNIQUE");
    assert_eq!(found[0].source.as_deref(), Some("tool"));
}
