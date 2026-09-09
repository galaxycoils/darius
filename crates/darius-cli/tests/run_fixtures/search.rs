use super::*;
#[test]
fn test_run_search_files_goal_succeeds() {
    let ctx = TestContext::new();
    let provider = FakeProvider::start_strict("search-secret");
    ctx.write_profile_config("default", provider.url(), KEY_ENV);
    std::fs::create_dir(ctx.workspace.path().join("src")).unwrap();
    std::fs::write(
        ctx.workspace.path().join("src/alpha.rs"),
        "SEARCH_TOKEN_Z9\n",
    )
    .unwrap();
    provider.push_tool_call(
        "search-z9",
        "search_files",
        serde_json::json!({"content":"SEARCH_TOKEN_Z9"}),
    );
    provider.push_text("done");
    ctx.command()
        .env(KEY_ENV, "search-secret")
        .args(["run", "search workspace"])
        .assert()
        .success();
    let requests = provider.recorded_requests();
    assert_eq!(requests.len(), 2);
    let tools: Vec<_> = requests[1].body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .collect();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["tool_call_id"], "search-z9");
    let content = tools[0]["content"].as_str().unwrap();
    assert!(content.contains("src/alpha.rs"), "{content}");
    provider.assert_clean();
    ctx.assert_clean_home("test_run_search_files_goal_succeeds");
}
