use darius_tools::{
    ToolCall, ToolOutcome, ToolRegistry, model_schemas::model_schemas, read_file,
    register_spill_read, search_files,
};

fn schema<'a>(schemas: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    schemas
        .iter()
        .find(|schema| schema["name"] == name)
        .unwrap_or_else(|| panic!("missing schema {name}"))
}

#[test]
fn model_schemas_are_exact_closed_objects() {
    let schemas = model_schemas();
    assert_eq!(schemas.len(), 11);
    for item in &schemas {
        assert_eq!(item["parameters"]["type"], "object", "{item}");
        assert_eq!(
            item["parameters"]["additionalProperties"], false,
            "{} accepts unknown arguments",
            item["name"]
        );
    }
}

#[test]
fn model_schemas_expose_exact_tool_arguments() {
    let schemas = model_schemas();
    let expected: &[(&str, &[&str])] = &[
        ("read_file", &["limit", "offset", "path"]),
        ("search_files", &["content", "dir", "limit", "pattern"]),
        ("write_file", &["content", "path"]),
        ("memory_search", &["text"]),
        ("memory_pack", &[]),
        ("spill_read", &["limit", "offset", "path"]),
        ("memory_remember", &["body", "kind", "title"]),
        ("task_list", &[]),
        ("task_add", &["title"]),
        ("task_complete", &["id"]),
        ("shell", &["command"]),
    ];
    for (name, expected_properties) in expected {
        let properties = schema(&schemas, name)["parameters"]["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{name} properties"));
        let mut actual: Vec<_> = properties.keys().map(String::as_str).collect();
        actual.sort_unstable();
        assert_eq!(&actual, expected_properties, "{name}");
    }
}

#[test]
fn model_schemas_bound_paging_and_spill_numbers() {
    let schemas = model_schemas();
    let read = &schema(&schemas, "read_file")["parameters"]["properties"];
    assert_eq!(read["offset"]["minimum"], 1);
    assert_eq!(read["limit"]["minimum"], 1);
    assert_eq!(read["limit"]["maximum"], read_file::MAX_LIMIT);
    let search = &schema(&schemas, "search_files")["parameters"]["properties"];
    assert_eq!(search["limit"]["minimum"], 1);
    assert_eq!(search["limit"]["maximum"], search_files::MAX_RESULTS);
    let spill = &schema(&schemas, "spill_read")["parameters"]["properties"];
    assert_eq!(spill["offset"]["minimum"], 0);
    assert_eq!(spill["limit"]["minimum"], 1);
    assert_eq!(spill["limit"]["maximum"], darius_tools::PREVIEW_CEILING);
}

#[test]
fn search_schema_requires_a_query_or_file_pattern() {
    let schemas = model_schemas();
    let any_of = schema(&schemas, "search_files")["parameters"]["anyOf"]
        .as_array()
        .expect("search anyOf");
    assert_eq!(
        any_of,
        &vec![
            serde_json::json!({"required": ["content"]}),
            serde_json::json!({"required": ["pattern"]}),
        ]
    );
}

#[test]
fn memory_kind_schema_matches_executor_values() {
    let schemas = model_schemas();
    assert_eq!(
        schema(&schemas, "memory_remember")["parameters"]["properties"]["kind"]["enum"],
        serde_json::json!(["fact", "decision", "preference", "episode", "note"])
    );
}

#[test]
fn spill_read_clamps_runtime_limit_to_schema_maximum() {
    let dir = std::env::temp_dir().join(format!("darius_spill_limit_{}", uuid::Uuid::new_v4()));
    let spill = dir.join("tool_results");
    std::fs::create_dir_all(&spill).unwrap();
    let path = spill.join("large.txt");
    std::fs::write(&path, "x".repeat(darius_tools::PREVIEW_CEILING + 100)).unwrap();
    let mut registry = ToolRegistry::new_with_roots(&dir, &spill).unwrap();
    register_spill_read(&mut registry);
    let call = ToolCall {
        id: "bounded-spill".into(),
        name: "spill_read".into(),
        arguments: serde_json::json!({
            "path": path,
            "limit": darius_tools::PREVIEW_CEILING * 2,
        }),
    };

    let ToolOutcome::Ok { preview, .. } = registry.execute(&call) else {
        panic!("spill_read failed");
    };
    assert_eq!(preview.chars().count(), darius_tools::PREVIEW_CEILING);
    std::fs::remove_dir_all(dir).unwrap();
}
