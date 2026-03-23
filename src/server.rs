use rmcp::{
    ErrorData as McpError, Json, RoleServer, ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::*,
    prompt, prompt_handler, prompt_router, schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde_json::json;

use crate::cli::ObsidianCli;
use crate::types::*;

// ── Tool argument types ──────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FilePathArg {
    /// File path relative to vault root (e.g., "notes/my-note.md", "projects/todo.md")
    pub file: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListFilesArgs {
    /// Optional sort order: "name", "modified", "created", "size" (default: "name")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    /// Maximum number of files to return (e.g., "20"). Default: all files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreateFileArgs {
    /// File path to create, relative to vault root (e.g., "notes/new-note.md")
    pub name: String,
    /// Initial content for the file (markdown)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Template name to use (from vault templates folder)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AppendPrependArgs {
    /// File path relative to vault root (e.g., "daily/2026-03-19.md")
    pub file: String,
    /// Content to append/prepend (markdown)
    pub content: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MoveFileArgs {
    /// Current file path relative to vault root
    pub file: String,
    /// Destination path (e.g., "archive/old-note.md"). Wikilinks are auto-updated.
    pub to: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RenameFileArgs {
    /// Current file path relative to vault root
    pub file: String,
    /// New name for the file
    pub to: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// Search query string (full-text search across all notes)
    pub query: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetPropertyArgs {
    /// File path relative to vault root
    pub file: String,
    /// Property name (frontmatter key, e.g., "status", "tags", "date")
    pub name: String,
    /// Property value (string, number, boolean, or array as JSON)
    pub value: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RemovePropertyArgs {
    /// File path relative to vault root
    pub file: String,
    /// Property name to remove from frontmatter
    pub name: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct TagArg {
    /// Tag name without # prefix (e.g., "project", "status/active")
    pub tag: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RenameTagArgs {
    /// Current tag name without # prefix
    pub old: String,
    /// New tag name without # prefix
    pub new: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DailyContentArgs {
    /// Content to append/prepend to today's daily note (markdown)
    pub content: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CommandArg {
    /// Obsidian command ID (e.g., "app:toggle-left-sidebar"). Use list first to discover IDs.
    pub id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalArg {
    /// JavaScript code to execute in Obsidian's context. DANGEROUS: can modify vault data.
    pub code: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PatchFileArgs {
    /// File path relative to vault root
    pub file: String,
    /// Exact text to find
    pub find: String,
    /// Replacement text
    pub replace: String,
    /// Replace all occurrences (default: first only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace_all: Option<bool>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReplaceLineArgs {
    /// File path relative to vault root
    pub file: String,
    /// 1-based line number to replace
    pub line: u32,
    /// Expected current content of the target line
    pub expected: String,
    /// New content for the target line
    pub replace: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct FindRelatedArgs {
    pub topic: String,
}

#[derive(Clone)]
pub struct ObsidianServer {
    cli: ObsidianCli,
    dangerous_enabled: bool,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

fn cli_err(e: crate::cli::CliError) -> String {
    e.to_tool_error()
}

fn eval_modify_js(path: &str, content: &str) -> String {
    let payload = json!({ "path": path, "content": content });
    let encoded = payload
        .to_string()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    format!(
        r#"(async()=>{{const d=JSON.parse('{encoded}');const f=app.vault.getAbstractFileByPath(d.path);if(!f)throw new Error('File not found: '+d.path);await app.vault.modify(f,d.content);}})()"#
    )
}

fn eval_patch_js(path: &str, find: &str, replace: &str, replace_all: bool) -> String {
    let payload = json!({ "path": path, "find": find, "replace": replace });
    let encoded = payload
        .to_string()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    let method = if replace_all { "replaceAll" } else { "replace" };
    format!(
        r#"(async()=>{{const d=JSON.parse('{encoded}');const f=app.vault.getAbstractFileByPath(d.path);if(!f)throw new Error('File not found: '+d.path);let c=await app.vault.read(f);c=c.{method}(d.find,d.replace);await app.vault.modify(f,c);}})()"#
    )
}

fn eval_replace_line_js(path: &str, line: u32, expected: &str, replace: &str) -> String {
    let payload = json!({
        "path": path,
        "line": line,
        "expected": expected,
        "replace": replace
    });
    let encoded = payload
        .to_string()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    format!(
        r#"(async()=>{{const d=JSON.parse('{encoded}');const f=app.vault.getAbstractFileByPath(d.path);if(!f)throw new Error('File not found: '+d.path);let c=await app.vault.read(f);const lines=c.split(/\r?\n/);const idx=d.line-1;if(!Number.isInteger(d.line)||d.line<1)throw new Error('Line number must be >= 1');if(idx>=lines.length)throw new Error('Line '+d.line+' out of range (file has '+lines.length+' lines)');if(lines[idx]!==d.expected)throw new Error('Line '+d.line+' conflict: expected "'+d.expected+'" but found "'+lines[idx]+'"');lines[idx]=d.replace;const eol=c.includes('\r\n')?'\r\n':'\n';await app.vault.modify(f,lines.join(eol));}})()"#
    )
}

fn parse_json_array_as_strings(val: &serde_json::Value) -> Vec<String> {
    val.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_as_search_results(val: &serde_json::Value) -> Vec<SearchFileResult> {
    if let Ok(results) = serde_json::from_value::<Vec<SearchFileResult>>(val.clone()) {
        return results;
    }
    parse_json_array_as_strings(val)
        .into_iter()
        .map(|file| SearchFileResult {
            file,
            matches: vec![],
        })
        .collect()
}

fn parse_as_links(val: &serde_json::Value) -> Vec<LinkInfo> {
    if let Ok(links) = serde_json::from_value::<Vec<LinkInfo>>(val.clone()) {
        return links;
    }
    parse_json_array_as_strings(val)
        .into_iter()
        .map(|target| LinkInfo {
            target,
            display_text: None,
        })
        .collect()
}

#[tool_router]
impl ObsidianServer {
    pub fn new(cli: ObsidianCli) -> Self {
        let dangerous_enabled = std::env::var("ENABLE_DANGEROUS_TOOLS").is_ok();
        Self {
            cli,
            dangerous_enabled,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    // ── Files & Folders (10 tools) ───────────────────────────────────

    /// List all files in the vault. Returns file paths sorted by name.
    #[tool(
        name = "list_files",
        description = "List all files in the vault. Returns file paths. Optionally sort by 'name', 'modified', 'created', or 'size' and limit results."
    )]
    async fn list_files(
        &self,
        Parameters(args): Parameters<ListFilesArgs>,
    ) -> Result<Json<FileList>, String> {
        let mut cli_args = vec!["files"];
        let sort_arg;
        if let Some(ref sort) = args.sort {
            sort_arg = format!("sort={}", sort);
            cli_args.push(&sort_arg);
        }
        let limit_arg;
        if let Some(limit) = args.limit {
            limit_arg = format!("limit={}", limit);
            cli_args.push(&limit_arg);
        }
        let result = self.cli.run(&cli_args).await.map_err(cli_err)?;
        let files = parse_json_array_as_strings(&result);
        let count = files.len();
        Ok(Json(FileList { files, count }))
    }

    /// List all folders in the vault.
    #[tool(
        name = "list_folders",
        description = "List all folders in the vault. Returns the folder hierarchy as a list of paths."
    )]
    async fn list_folders(&self) -> Result<Json<FolderList>, String> {
        let result = self.cli.run(&["folders"]).await.map_err(cli_err)?;
        let folders = parse_json_array_as_strings(&result);
        let count = folders.len();
        Ok(Json(FolderList { folders, count }))
    }

    /// Read a file's full content.
    #[tool(
        name = "read_file",
        description = "Read a file's full content from the vault. Returns markdown including frontmatter. Path is relative to vault root (e.g., 'notes/my-note.md')."
    )]
    async fn read_file(
        &self,
        Parameters(args): Parameters<FilePathArg>,
    ) -> Result<Json<FileContent>, String> {
        let file_arg = format!("file={}", args.file);
        let result = self
            .cli
            .run_raw(&["read", &file_arg])
            .await
            .map_err(cli_err)?;
        Ok(Json(FileContent {
            path: args.file,
            content: result,
        }))
    }

    /// Get file metadata.
    #[tool(
        name = "file_info",
        description = "Get metadata for a file: path, name, size, created/modified dates. Path is relative to vault root."
    )]
    async fn file_info(
        &self,
        Parameters(args): Parameters<FilePathArg>,
    ) -> Result<Json<FileInfo>, String> {
        let file_arg = format!("file={}", args.file);
        let result = self.cli.run(&["file", &file_arg]).await.map_err(cli_err)?;
        let info = serde_json::from_value(result)
            .map_err(|e| format!("Failed to parse file info: {}", e))?;
        Ok(Json(info))
    }

    /// Create a new note.
    #[tool(
        name = "create_file",
        description = "Create a new note in the vault. Provide a path (e.g., 'projects/new-idea.md'), optional content, and optional template name."
    )]
    async fn create_file(
        &self,
        Parameters(args): Parameters<CreateFileArgs>,
    ) -> Result<Json<FileCreated>, String> {
        let path_arg = if args.name.contains('/') || args.name.contains('\\') {
            format!("path={}", args.name)
        } else {
            format!("name={}", args.name)
        };
        let mut cli_args = vec!["create", &path_arg];
        let content_arg;
        if let Some(ref content) = args.content {
            content_arg = format!("content={}", content);
            cli_args.push(&content_arg);
        }
        let template_arg;
        if let Some(ref template) = args.template {
            template_arg = format!("template={}", template);
            cli_args.push(&template_arg);
        }
        self.cli.run(&cli_args).await.map_err(cli_err)?;
        Ok(Json(FileCreated {
            path: args.name,
            message: "File created successfully".into(),
        }))
    }

    /// Append content to a file.
    #[tool(
        name = "append_to_file",
        description = "Append content to the end of a file. Content is added after existing text. Path relative to vault root."
    )]
    async fn append_to_file(
        &self,
        Parameters(args): Parameters<AppendPrependArgs>,
    ) -> Result<Json<FileModified>, String> {
        let file_arg = format!("file={}", args.file);
        let content_arg = format!("content={}", args.content);
        self.cli
            .run(&["append", &file_arg, &content_arg])
            .await
            .map_err(cli_err)?;
        Ok(Json(FileModified {
            path: args.file,
            message: "Content appended successfully".into(),
        }))
    }

    /// Prepend content to a file (after frontmatter).
    #[tool(
        name = "prepend_to_file",
        description = "Prepend content to a file, inserted after any existing frontmatter. Path relative to vault root."
    )]
    async fn prepend_to_file(
        &self,
        Parameters(args): Parameters<AppendPrependArgs>,
    ) -> Result<Json<FileModified>, String> {
        let file_arg = format!("file={}", args.file);
        let content_arg = format!("content={}", args.content);
        self.cli
            .run(&["prepend", &file_arg, &content_arg])
            .await
            .map_err(cli_err)?;
        Ok(Json(FileModified {
            path: args.file,
            message: "Content prepended successfully".into(),
        }))
    }

    #[tool(
        name = "write_file",
        description = "Replace the entire content of an existing file. Read first, modify, then write back."
    )]
    async fn write_file(
        &self,
        Parameters(args): Parameters<AppendPrependArgs>,
    ) -> Result<Json<FileModified>, String> {
        let js = eval_modify_js(&args.file, &args.content);
        let code_arg = format!("code={js}");
        self.cli.run(&["eval", &code_arg]).await.map_err(cli_err)?;
        Ok(Json(FileModified {
            path: args.file,
            message: "File updated".into(),
        }))
    }

    #[tool(
        name = "patch_file",
        description = "Find and replace text within a file. Set replace_all=true for all occurrences."
    )]
    async fn patch_file(
        &self,
        Parameters(args): Parameters<PatchFileArgs>,
    ) -> Result<Json<PatchResult>, String> {
        let replace_all = args.replace_all.unwrap_or(false);
        let js = eval_patch_js(&args.file, &args.find, &args.replace, replace_all);
        let code_arg = format!("code={js}");
        self.cli.run(&["eval", &code_arg]).await.map_err(cli_err)?;
        Ok(Json(PatchResult {
            path: args.file,
            replacements: if replace_all { 0 } else { 1 },
            message: "Patch applied".into(),
        }))
    }

    #[tool(
        name = "replace_line",
        description = "Replace exactly one 1-based line in a file. Fails if the current line content does not match the expected text."
    )]
    async fn replace_line(
        &self,
        Parameters(args): Parameters<ReplaceLineArgs>,
    ) -> Result<Json<FileModified>, String> {
        let js = eval_replace_line_js(&args.file, args.line, &args.expected, &args.replace);
        let code_arg = format!("code={js}");
        self.cli.run(&["eval", &code_arg]).await.map_err(cli_err)?;
        Ok(Json(FileModified {
            path: args.file,
            message: format!("Line {} replaced", args.line),
        }))
    }

    #[tool(
        name = "move_file",
        description = "Move a file to a new location in the vault. All wikilinks pointing to this file are automatically updated. Both paths relative to vault root."
    )]
    async fn move_file(
        &self,
        Parameters(args): Parameters<MoveFileArgs>,
    ) -> Result<Json<FileModified>, String> {
        let file_arg = format!("file={}", args.file);
        let to_arg = format!("to={}", args.to);
        self.cli
            .run(&["move", &file_arg, &to_arg])
            .await
            .map_err(cli_err)?;
        Ok(Json(FileModified {
            path: args.to,
            message: format!("File moved from '{}'. Wikilinks updated.", args.file),
        }))
    }

    /// Rename a file.
    #[tool(
        name = "rename_file",
        description = "Rename a file in the vault. Path relative to vault root."
    )]
    async fn rename_file(
        &self,
        Parameters(args): Parameters<RenameFileArgs>,
    ) -> Result<Json<FileModified>, String> {
        let file_arg = format!("file={}", args.file);
        let to_arg = format!("to={}", args.to);
        self.cli
            .run(&["rename", &file_arg, &to_arg])
            .await
            .map_err(cli_err)?;
        Ok(Json(FileModified {
            path: args.to,
            message: format!("File renamed from '{}'", args.file),
        }))
    }

    /// Delete a file (moves to trash).
    #[tool(
        name = "delete_file",
        description = "Delete a file from the vault (moves to system trash, recoverable). DESTRUCTIVE operation. Path relative to vault root."
    )]
    async fn delete_file(
        &self,
        Parameters(args): Parameters<FilePathArg>,
    ) -> Result<Json<SuccessResult>, String> {
        let file_arg = format!("file={}", args.file);
        self.cli
            .run(&["delete", &file_arg])
            .await
            .map_err(cli_err)?;
        Ok(Json(SuccessResult {
            success: true,
            message: format!("File '{}' moved to trash", args.file),
        }))
    }

    // ── Search (2 tools) ─────────────────────────────────────────────

    /// Full-text search across the vault.
    #[tool(
        name = "search",
        description = "Full-text search across all notes in the vault. Returns matching file paths."
    )]
    async fn search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<Json<SearchResults>, String> {
        let query_arg = format!("query={}", args.query);
        let result = self
            .cli
            .run(&["search", &query_arg])
            .await
            .map_err(cli_err)?;
        let results = parse_as_search_results(&result);
        let total_matches = results.len();
        Ok(Json(SearchResults {
            query: args.query,
            results,
            total_matches,
        }))
    }

    /// Search with surrounding context (grep-like).
    #[tool(
        name = "search_context",
        description = "Search with surrounding context lines (grep-like). Returns matches with context around each hit."
    )]
    async fn search_context(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<Json<SearchResults>, String> {
        let query_arg = format!("query={}", args.query);
        let result = self
            .cli
            .run(&["search:context", &query_arg])
            .await
            .map_err(cli_err)?;
        let results = parse_as_search_results(&result);
        let total_matches = results.len();
        Ok(Json(SearchResults {
            query: args.query,
            results,
            total_matches,
        }))
    }

    // ── Graph & Links (5 tools) ──────────────────────────────────────

    /// Get outgoing links from a file.
    #[tool(
        name = "get_links",
        description = "Get all outgoing links from a file. Shows what this note links to. Path relative to vault root."
    )]
    async fn get_links(
        &self,
        Parameters(args): Parameters<FilePathArg>,
    ) -> Result<Json<LinkList>, String> {
        let file_arg = format!("file={}", args.file);
        let result = self.cli.run(&["links", &file_arg]).await.map_err(cli_err)?;
        let links = parse_as_links(&result);
        let count = links.len();
        Ok(Json(LinkList {
            file: args.file,
            links,
            count,
        }))
    }

    /// Get incoming links (backlinks) to a file.
    #[tool(
        name = "get_backlinks",
        description = "Get all incoming links (backlinks) to a file. Shows which notes reference this one. Path relative to vault root."
    )]
    async fn get_backlinks(
        &self,
        Parameters(args): Parameters<FilePathArg>,
    ) -> Result<Json<BacklinkList>, String> {
        let file_arg = format!("file={}", args.file);
        let result = self
            .cli
            .run(&["backlinks", &file_arg])
            .await
            .map_err(cli_err)?;
        let backlinks = parse_as_links(&result);
        let count = backlinks.len();
        Ok(Json(BacklinkList {
            file: args.file,
            backlinks,
            count,
        }))
    }

    /// Get unresolved links (links pointing to non-existent files).
    #[tool(
        name = "get_unresolved_links",
        description = "List all unresolved links in the vault -- links pointing to notes that don't exist yet. Useful for finding gaps."
    )]
    async fn get_unresolved_links(&self) -> Result<Json<FilePathList>, String> {
        let result = self.cli.run(&["unresolved"]).await.map_err(cli_err)?;
        let files = parse_json_array_as_strings(&result);
        let count = files.len();
        Ok(Json(FilePathList { files, count }))
    }

    /// Get orphan files (no incoming links).
    #[tool(
        name = "get_orphans",
        description = "List orphan notes -- files with no incoming links from other notes. These may be forgotten or disconnected."
    )]
    async fn get_orphans(&self) -> Result<Json<FilePathList>, String> {
        let result = self.cli.run(&["orphans"]).await.map_err(cli_err)?;
        let files = parse_json_array_as_strings(&result);
        let count = files.len();
        Ok(Json(FilePathList { files, count }))
    }

    /// Get dead-end files (no outgoing links).
    #[tool(
        name = "get_deadends",
        description = "List dead-end notes -- files with no outgoing links to other notes. Consider adding connections."
    )]
    async fn get_deadends(&self) -> Result<Json<FilePathList>, String> {
        let result = self.cli.run(&["deadends"]).await.map_err(cli_err)?;
        let files = parse_json_array_as_strings(&result);
        let count = files.len();
        Ok(Json(FilePathList { files, count }))
    }

    // ── Properties (4 tools) ─────────────────────────────────────────

    /// Read frontmatter properties of a file.
    #[tool(
        name = "get_properties",
        description = "Read all frontmatter properties from a file. Returns key-value pairs from the YAML frontmatter block."
    )]
    async fn get_properties(
        &self,
        Parameters(args): Parameters<FilePathArg>,
    ) -> Result<Json<PropertyList>, String> {
        let file_arg = format!("file={}", args.file);
        let result = self
            .cli
            .run(&["properties", &file_arg])
            .await
            .map_err(cli_err)?;
        let properties: Vec<PropertyValue> = if let Some(obj) = result.as_object() {
            obj.iter()
                .map(|(k, v)| PropertyValue {
                    name: k.clone(),
                    value: serde_json::to_string(v).unwrap_or_default(),
                })
                .collect()
        } else {
            serde_json::from_value(result).unwrap_or_default()
        };
        Ok(Json(PropertyList {
            file: args.file,
            properties,
        }))
    }

    /// Set a frontmatter property.
    #[tool(
        name = "set_property",
        description = "Set a frontmatter property on a file. Creates the property if it doesn't exist, updates it if it does."
    )]
    async fn set_property(
        &self,
        Parameters(args): Parameters<SetPropertyArgs>,
    ) -> Result<Json<SuccessResult>, String> {
        let file_arg = format!("file={}", args.file);
        let name_arg = format!("name={}", args.name);
        let value_arg = format!("value={}", args.value);
        self.cli
            .run(&["property:set", &file_arg, &name_arg, &value_arg])
            .await
            .map_err(cli_err)?;
        Ok(Json(SuccessResult {
            success: true,
            message: format!("Property '{}' set on '{}'", args.name, args.file),
        }))
    }

    /// Remove a frontmatter property.
    #[tool(
        name = "remove_property",
        description = "Remove a frontmatter property from a file. The key and its value are deleted from the YAML block."
    )]
    async fn remove_property(
        &self,
        Parameters(args): Parameters<RemovePropertyArgs>,
    ) -> Result<Json<SuccessResult>, String> {
        let file_arg = format!("file={}", args.file);
        let name_arg = format!("name={}", args.name);
        self.cli
            .run(&["property:remove", &file_arg, &name_arg])
            .await
            .map_err(cli_err)?;
        Ok(Json(SuccessResult {
            success: true,
            message: format!("Property '{}' removed from '{}'", args.name, args.file),
        }))
    }

    /// Get file aliases.
    #[tool(
        name = "get_aliases",
        description = "Get all aliases for a file from its frontmatter. Aliases are alternative names for linking."
    )]
    async fn get_aliases(
        &self,
        Parameters(args): Parameters<FilePathArg>,
    ) -> Result<Json<AliasList>, String> {
        let file_arg = format!("file={}", args.file);
        let result = self
            .cli
            .run(&["aliases", &file_arg])
            .await
            .map_err(cli_err)?;
        let aliases = parse_json_array_as_strings(&result);
        Ok(Json(AliasList {
            file: args.file,
            aliases,
        }))
    }

    // ── Tags (3 tools) ───────────────────────────────────────────────

    /// List all tags in the vault with counts.
    #[tool(
        name = "list_tags",
        description = "List all tags used across the vault with their usage counts. Tags include both frontmatter tags and inline #tags."
    )]
    async fn list_tags(&self) -> Result<Json<TagList>, String> {
        let result = self.cli.run(&["tags"]).await.map_err(cli_err)?;
        let tags: Vec<TagCount> = serde_json::from_value(result.clone()).unwrap_or_else(|_| {
            parse_json_array_as_strings(&result)
                .into_iter()
                .map(|tag| TagCount { tag, count: 0 })
                .collect()
        });
        let total = tags.len();
        Ok(Json(TagList { tags, total }))
    }

    /// List files with a specific tag.
    #[tool(
        name = "files_by_tag",
        description = "List all files tagged with a specific tag. Provide tag name without # prefix (e.g., 'project' not '#project')."
    )]
    async fn files_by_tag(
        &self,
        Parameters(args): Parameters<TagArg>,
    ) -> Result<Json<TagFiles>, String> {
        let tag_arg = format!("tag={}", args.tag);
        let result = self.cli.run(&["tag", &tag_arg]).await.map_err(cli_err)?;
        let files = parse_json_array_as_strings(&result);
        let count = files.len();
        Ok(Json(TagFiles {
            tag: args.tag,
            files,
            count,
        }))
    }

    /// Rename a tag vault-wide.
    #[tool(
        name = "rename_tag",
        description = "Bulk rename a tag across the entire vault. Updates all occurrences in frontmatter and inline tags. Provide names without # prefix."
    )]
    async fn rename_tag(
        &self,
        Parameters(args): Parameters<RenameTagArgs>,
    ) -> Result<Json<SuccessResult>, String> {
        let old_arg = format!("old={}", args.old);
        let new_arg = format!("new={}", args.new);
        self.cli
            .run(&["tags:rename", &old_arg, &new_arg])
            .await
            .map_err(cli_err)?;
        Ok(Json(SuccessResult {
            success: true,
            message: format!("Tag '{}' renamed to '{}' vault-wide", args.old, args.new),
        }))
    }

    // ── Daily Notes (4 tools) ────────────────────────────────────────

    /// Open or create today's daily note.
    #[tool(
        name = "daily_open",
        description = "Open or create today's daily note in Obsidian. Creates the note using the daily note template if it doesn't exist."
    )]
    async fn daily_open(&self) -> Result<Json<DailyNoteResult>, String> {
        let result = self.cli.run(&["daily"]).await.map_err(cli_err)?;
        let path = result
            .as_str()
            .or_else(|| result.get("path").and_then(|p| p.as_str()))
            .unwrap_or("daily note")
            .to_string();
        Ok(Json(DailyNoteResult {
            path,
            message: "Daily note opened".into(),
        }))
    }

    /// Read today's daily note content.
    #[tool(
        name = "daily_read",
        description = "Read the content of today's daily note. Returns the full markdown content."
    )]
    async fn daily_read(&self) -> Result<Json<FileContent>, String> {
        let content = self.cli.run_raw(&["daily:read"]).await.map_err(cli_err)?;
        let path = self
            .cli
            .run_raw(&["daily:path"])
            .await
            .map_or_else(|_| "daily note".into(), |p| p.trim().to_string());
        Ok(Json(FileContent { path, content }))
    }

    /// Append to today's daily note.
    #[tool(
        name = "daily_append",
        description = "Append content to the end of today's daily note. Creates the daily note first if it doesn't exist."
    )]
    async fn daily_append(
        &self,
        Parameters(args): Parameters<DailyContentArgs>,
    ) -> Result<Json<DailyNoteResult>, String> {
        let content_arg = format!("content={}", args.content);
        self.cli
            .run(&["daily:append", &content_arg])
            .await
            .map_err(cli_err)?;
        Ok(Json(DailyNoteResult {
            path: "today's daily note".into(),
            message: "Content appended to daily note".into(),
        }))
    }

    /// Prepend to today's daily note.
    #[tool(
        name = "daily_prepend",
        description = "Prepend content to today's daily note (inserted after frontmatter). Creates the daily note first if it doesn't exist."
    )]
    async fn daily_prepend(
        &self,
        Parameters(args): Parameters<DailyContentArgs>,
    ) -> Result<Json<DailyNoteResult>, String> {
        let content_arg = format!("content={}", args.content);
        self.cli
            .run(&["daily:prepend", &content_arg])
            .await
            .map_err(cli_err)?;
        Ok(Json(DailyNoteResult {
            path: "today's daily note".into(),
            message: "Content prepended to daily note".into(),
        }))
    }

    // ── Misc (6 tools) ───────────────────────────────────────────────

    /// Get heading structure of a file.
    #[tool(
        name = "get_outline",
        description = "Get the heading outline of a file. Returns headings with their levels (H1-H6) and line numbers."
    )]
    async fn get_outline(
        &self,
        Parameters(args): Parameters<FilePathArg>,
    ) -> Result<Json<OutlineResult>, String> {
        let file_arg = format!("file={}", args.file);
        let result = self
            .cli
            .run(&["outline", &file_arg])
            .await
            .map_err(cli_err)?;
        let headings: Vec<OutlineEntry> =
            serde_json::from_value(result.clone()).unwrap_or_else(|_| {
                parse_json_array_as_strings(&result)
                    .into_iter()
                    .map(|heading| OutlineEntry {
                        heading,
                        level: 0,
                        line: None,
                    })
                    .collect()
            });
        Ok(Json(OutlineResult {
            file: args.file,
            headings,
        }))
    }

    /// List all tasks in the vault.
    #[tool(
        name = "list_tasks",
        description = "List all tasks (checkboxes) across the vault. Returns task text, source file, completion status, and line number."
    )]
    async fn list_tasks(&self) -> Result<Json<VaultTaskList>, String> {
        let result = self.cli.run(&["tasks"]).await.map_err(cli_err)?;
        let tasks: Vec<TaskItem> = serde_json::from_value(result.clone()).unwrap_or_else(|_| {
            parse_json_array_as_strings(&result)
                .into_iter()
                .map(|text| TaskItem {
                    text,
                    file: String::new(),
                    completed: false,
                    line: None,
                })
                .collect()
        });
        let total = tasks.len();
        Ok(Json(VaultTaskList { tasks, total }))
    }

    /// Get word count for a file.
    #[tool(
        name = "get_wordcount",
        description = "Get the word and character count for a file. Path relative to vault root."
    )]
    async fn get_wordcount(
        &self,
        Parameters(args): Parameters<FilePathArg>,
    ) -> Result<Json<WordCountResult>, String> {
        let file_arg = format!("file={}", args.file);
        let result = self
            .cli
            .run(&["wordcount", &file_arg])
            .await
            .map_err(cli_err)?;
        let wc: WordCountResult = serde_json::from_value(result.clone()).unwrap_or_else(|_| {
            let raw = parse_json_array_as_strings(&result).join(" ");
            let words = raw
                .split_whitespace()
                .find_map(|w| w.parse::<u64>().ok())
                .unwrap_or(0);
            WordCountResult {
                file: args.file.clone(),
                words,
                characters: None,
            }
        });
        Ok(Json(wc))
    }

    /// List available templates.
    #[tool(
        name = "list_templates",
        description = "List all available templates in the vault's templates folder. Use template names with create_file."
    )]
    async fn list_templates(&self) -> Result<Json<TemplateList>, String> {
        let result = self.cli.run(&["templates"]).await.map_err(cli_err)?;
        let templates = parse_json_array_as_strings(&result);
        let count = templates.len();
        Ok(Json(TemplateList { templates, count }))
    }

    #[tool(
        name = "list_commands",
        description = "List all available Obsidian commands with their IDs."
    )]
    async fn list_commands(&self) -> Result<Json<CommandList>, String> {
        let result = self.cli.run(&["commands"]).await.map_err(cli_err)?;
        let commands = parse_json_array_as_strings(&result);
        let count = commands.len();
        Ok(Json(CommandList { commands, count }))
    }

    #[tool(
        name = "execute_command",
        description = "Execute an Obsidian command by its ID. DANGEROUS: requires ENABLE_DANGEROUS_TOOLS=true. Use list_commands to discover IDs."
    )]
    async fn execute_command(
        &self,
        Parameters(args): Parameters<CommandArg>,
    ) -> Result<Json<CommandResult>, String> {
        if !self.dangerous_enabled {
            return Err(
                "execute_command is disabled. Set ENABLE_DANGEROUS_TOOLS=true to enable.".into(),
            );
        }
        let id_arg = format!("id={}", args.id);
        self.cli.run(&["command", &id_arg]).await.map_err(cli_err)?;
        Ok(Json(CommandResult {
            command: args.id,
            message: "Command executed".into(),
        }))
    }

    /// Execute JavaScript in Obsidian.
    #[tool(
        name = "eval_js",
        description = "Execute JavaScript code in Obsidian's runtime context. DANGEROUS: can read/write vault data, access Obsidian internals. Requires ENABLE_DANGEROUS_TOOLS=true."
    )]
    async fn eval_js(
        &self,
        Parameters(args): Parameters<EvalArg>,
    ) -> Result<Json<EvalResult>, String> {
        if !self.dangerous_enabled {
            return Err("eval_js is disabled. Set ENABLE_DANGEROUS_TOOLS=true to enable.".into());
        }
        let code_arg = format!("code={}", args.code);
        let result = self.cli.run(&["eval", &code_arg]).await.map_err(cli_err)?;
        Ok(Json(EvalResult {
            result: serde_json::to_string(&result).unwrap_or_default(),
        }))
    }
}

// ── Prompts ──────────────────────────────────────────────────────────

#[prompt_router]
impl ObsidianServer {
    /// Summarize today's daily note
    #[prompt(
        name = "daily_summary",
        description = "Read today's daily note and ask for a summary. Great for end-of-day reviews."
    )]
    async fn daily_summary(
        &self,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let content = self
            .cli
            .run_raw(&["daily:read"])
            .await
            .unwrap_or_else(|_| "(Could not read daily note -- it may not exist yet)".into());
        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Here is today's daily note from my Obsidian vault. Please provide a concise summary highlighting key points, decisions, and action items:\n\n{}",
                content
            ),
        )])
    }

    /// Find notes related to a given topic
    #[prompt(
        name = "find_related",
        description = "Search the vault for notes related to a topic and suggest connections."
    )]
    async fn find_related(
        &self,
        Parameters(args): Parameters<FindRelatedArgs>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let query_arg = format!("query={}", args.topic);
        let results = self
            .cli
            .run(&["search", &query_arg])
            .await
            .unwrap_or_else(|_| json!([]));
        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "I searched my Obsidian vault for '{}' and found these results:\n\n{}\n\nPlease analyze these notes and suggest:\n1. Key connections between them\n2. Potential new notes or links to create\n3. Any knowledge gaps I should fill",
                args.topic,
                serde_json::to_string_pretty(&results).unwrap_or_default()
            ),
        )])
    }

    /// Review orphaned notes
    #[prompt(
        name = "review_orphans",
        description = "List notes with no incoming links and suggest how to integrate them into the knowledge graph."
    )]
    async fn review_orphans(
        &self,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let orphans = self
            .cli
            .run(&["orphans"])
            .await
            .unwrap_or_else(|_| json!([]));
        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "These notes in my Obsidian vault have no incoming links (orphans):\n\n{}\n\nPlease review this list and suggest:\n1. Which notes could be linked from other relevant notes\n2. Which notes might be outdated and candidates for archiving\n3. How to better integrate these into the knowledge graph",
                serde_json::to_string_pretty(&orphans).unwrap_or_default()
            ),
        )])
    }
}

// ── ServerHandler ────────────────────────────────────────────────────

#[tool_handler]
#[prompt_handler]
impl ServerHandler for ObsidianServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .enable_logging()
                .build(),
        )
        .with_server_info(Implementation::from_build_env())
        .with_instructions(format!(
            "Obsidian vault MCP server for vault '{}'. \
             Use tools to read, create, search, and manage notes in the vault. \
             Use resources (vault://files, vault://folders, vault://tags) to browse vault structure without side effects. \
             File paths are always relative to vault root (e.g., 'notes/my-note.md'). \
             Obsidian app must be running with CLI enabled (v1.12+). \
             Dangerous tools (eval_js, execute_command) are {}.",
            self.cli.vault_name(),
            if self.dangerous_enabled { "ENABLED" } else { "disabled" }
        ))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![
                RawResource::new("vault://files", "Vault Files")
                    .with_description("List of all files in the vault with paths")
                    .with_mime_type("application/json")
                    .no_annotation(),
                RawResource::new("vault://folders", "Folder Structure")
                    .with_description("Folder hierarchy of the vault")
                    .with_mime_type("application/json")
                    .no_annotation(),
                RawResource::new("vault://tags", "All Tags")
                    .with_description("All tags in the vault with usage counts")
                    .with_mime_type("application/json")
                    .no_annotation(),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let uri = request.uri.as_str();
        match uri {
            "vault://files" => {
                let result = self
                    .cli
                    .run(&["files"])
                    .await
                    .map_err(|e| McpError::internal_error(e.to_tool_error(), None))?;
                Ok(ReadResourceResult::new(vec![ResourceContents::text(
                    serde_json::to_string_pretty(&result).unwrap_or_default(),
                    &request.uri,
                )]))
            }
            "vault://folders" => {
                let result = self
                    .cli
                    .run(&["folders"])
                    .await
                    .map_err(|e| McpError::internal_error(e.to_tool_error(), None))?;
                Ok(ReadResourceResult::new(vec![ResourceContents::text(
                    serde_json::to_string_pretty(&result).unwrap_or_default(),
                    &request.uri,
                )]))
            }
            "vault://tags" => {
                let result = self
                    .cli
                    .run(&["tags"])
                    .await
                    .map_err(|e| McpError::internal_error(e.to_tool_error(), None))?;
                Ok(ReadResourceResult::new(vec![ResourceContents::text(
                    serde_json::to_string_pretty(&result).unwrap_or_default(),
                    &request.uri,
                )]))
            }
            _ => Err(McpError::resource_not_found(
                "resource_not_found",
                Some(json!({ "uri": uri })),
            )),
        }
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: Vec::new(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn set_level(
        &self,
        _request: SetLevelRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        Ok(())
    }
}
