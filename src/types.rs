use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FileInfo {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FileList {
    pub files: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FolderList {
    pub folders: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FileContent {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FileCreated {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FileModified {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchMatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchFileResult {
    pub file: String,
    pub matches: Vec<SearchMatch>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchResults {
    pub query: String,
    pub results: Vec<SearchFileResult>,
    pub total_matches: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct LinkInfo {
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_text: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct LinkList {
    pub file: String,
    pub links: Vec<LinkInfo>,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BacklinkList {
    pub file: String,
    pub backlinks: Vec<LinkInfo>,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FilePathList {
    pub files: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PropertyValue {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PropertyList {
    pub file: String,
    pub properties: Vec<PropertyValue>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AliasList {
    pub file: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TagCount {
    pub tag: String,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TagList {
    pub tags: Vec<TagCount>,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TagFiles {
    pub tag: String,
    pub files: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DailyNoteResult {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct OutlineEntry {
    pub heading: String,
    pub level: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct OutlineResult {
    pub file: String,
    pub headings: Vec<OutlineEntry>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskItem {
    pub text: String,
    pub file: String,
    pub completed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VaultTaskList {
    pub tasks: Vec<TaskItem>,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TemplateList {
    pub templates: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WordCountResult {
    pub file: String,
    pub words: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub characters: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CommandList {
    pub commands: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CommandResult {
    pub command: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvalResult {
    pub result: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SuccessResult {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PatchResult {
    pub path: String,
    pub replacements: usize,
    pub message: String,
}
