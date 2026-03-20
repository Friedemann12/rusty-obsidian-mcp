use serde_json::json;

#[allow(dead_code)]
#[path = "../src/types.rs"]
mod types;

use types::*;

#[test]
fn file_list_from_json_array() {
    let raw = json!(["notes/hello.md", "projects/todo.md", "README.md"]);
    let files: Vec<String> = raw.as_array().unwrap().iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    assert_eq!(files.len(), 3);
    assert_eq!(files[0], "notes/hello.md");
}

#[test]
fn file_info_full() {
    let raw = json!({
        "path": "notes/hello.md",
        "name": "hello",
        "size": 1234,
        "created": "2026-01-15T10:30:00Z",
        "modified": "2026-03-19T14:00:00Z"
    });
    let info: FileInfo = serde_json::from_value(raw).unwrap();
    assert_eq!(info.path, "notes/hello.md");
    assert_eq!(info.name.as_deref(), Some("hello"));
    assert_eq!(info.size, Some(1234));
}

#[test]
fn file_info_minimal() {
    let raw = json!({ "path": "test.md" });
    let info: FileInfo = serde_json::from_value(raw).unwrap();
    assert_eq!(info.path, "test.md");
    assert!(info.name.is_none());
}

#[test]
fn search_results_structured() {
    let raw = json!([
        { "file": "notes/rust.md", "matches": [
            { "line": 5, "text": "Rust is great" },
            { "text": "Using Rust for MCP" }
        ]},
        { "file": "projects/mcp.md", "matches": [
            { "line": 12, "text": "MCP server in Rust" }
        ]}
    ]);
    let results: Vec<SearchFileResult> = serde_json::from_value(raw).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].matches[0].line, Some(5));
    assert!(results[0].matches[1].line.is_none());
}

#[test]
fn tag_counts() {
    let raw = json!([
        { "tag": "project", "count": 15 },
        { "tag": "status/active", "count": 8 }
    ]);
    let tags: Vec<TagCount> = serde_json::from_value(raw).unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].count, 15);
}

#[test]
fn properties_as_object_to_string_values() {
    let raw = json!({
        "title": "My Note",
        "tags": ["rust", "mcp"],
        "draft": false
    });
    let properties: Vec<PropertyValue> = raw.as_object().unwrap()
        .iter()
        .map(|(k, v)| PropertyValue { name: k.clone(), value: v.to_string() })
        .collect();
    assert_eq!(properties.len(), 3);
    let tags_prop = properties.iter().find(|p| p.name == "tags").unwrap();
    assert!(tags_prop.value.contains("rust"));
}

#[test]
fn outline_entries() {
    let raw = json!([
        { "heading": "Introduction", "level": 1, "line": 3 },
        { "heading": "Conclusion", "level": 1 }
    ]);
    let entries: Vec<OutlineEntry> = serde_json::from_value(raw).unwrap();
    assert_eq!(entries[0].line, Some(3));
    assert!(entries[1].line.is_none());
}

#[test]
fn task_items() {
    let raw = json!([
        { "text": "Buy groceries", "file": "daily/2026-03-19.md", "completed": false, "line": 15 },
        { "text": "Review PR", "file": "projects/mcp.md", "completed": true, "line": 42 }
    ]);
    let tasks: Vec<TaskItem> = serde_json::from_value(raw).unwrap();
    assert!(!tasks[0].completed);
    assert!(tasks[1].completed);
}

#[test]
fn link_info() {
    let raw = json!([
        { "target": "notes/other.md", "display_text": "Other Note" },
        { "target": "projects/todo.md" }
    ]);
    let links: Vec<LinkInfo> = serde_json::from_value(raw).unwrap();
    assert_eq!(links[0].display_text.as_deref(), Some("Other Note"));
    assert!(links[1].display_text.is_none());
}

#[test]
fn wordcount() {
    let raw = json!({ "file": "notes/hello.md", "words": 350, "characters": 2100 });
    let wc: WordCountResult = serde_json::from_value(raw).unwrap();
    assert_eq!(wc.words, 350);
    assert_eq!(wc.characters, Some(2100));
}

#[test]
fn success_result_serialize() {
    let result = SuccessResult { success: true, message: "Done".into() };
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["success"], true);
}

#[test]
fn file_info_skips_none_fields() {
    let info = FileInfo { path: "test.md".into(), name: None, size: None, created: None, modified: None };
    let json = serde_json::to_value(&info).unwrap();
    assert!(json.get("name").is_none());
    assert_eq!(json["path"], "test.md");
}

#[test]
fn plain_text_lines_to_string_vec() {
    let array = json!(["folder_a", "folder_b/sub", "folder_c"]);
    let strings: Vec<String> = array.as_array().unwrap()
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    assert_eq!(strings, vec!["folder_a", "folder_b/sub", "folder_c"]);
}

#[test]
fn search_fallback_from_string_array() {
    let val = json!(["notes/a.md", "notes/b.md"]);
    let results: Vec<SearchFileResult> = serde_json::from_value::<Vec<SearchFileResult>>(val.clone())
        .unwrap_or_else(|_| {
            val.as_array().unwrap().iter()
                .filter_map(|v| v.as_str())
                .map(|file| SearchFileResult { file: file.into(), matches: vec![] })
                .collect()
        });
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].file, "notes/a.md");
    assert!(results[0].matches.is_empty());
}

#[test]
fn patch_result_serialize() {
    let result = PatchResult { path: "notes/a.md".into(), replacements: 1, message: "Patched".into() };
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["replacements"], 1);
}
