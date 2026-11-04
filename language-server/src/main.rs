use std::time::Instant;

use diagnostics::extract_diagnostics;
use semantic_tokens::extract_semantic_tokens;
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::{self, Result};
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

mod diagnostics;
mod project;
mod semantic_tokens;

use project::{File, Project, find_root, get_file, uri_to_path};

const MAX_SEMANTIC_TOKENS_LISTS: usize = 3;

// TODO: format-on-save
// TODO: watch project files that the client has not opened; with workspace/didChangeWatchedFiles

// NOTE: The LSP protocol uses UTF-16 by default, so care should be taken when counting characters.

/// Token types. Must be the same as in the [SemanticTokenLegend].
mod tt {
    pub const KEYWORD: u32 = 0;
    pub const OPERATOR: u32 = 1;
    pub const NUMBER: u32 = 2;
    pub const VARIABLE: u32 = 3;
    pub const STRING: u32 = 4;
    pub const FUNCTION: u32 = 5;
    pub const PARAMETER: u32 = 6;
    pub const TYPE: u32 = 7;
    pub const COMMENT: u32 = 8;
    pub const DELIMITER: u32 = 9;
    pub const STRUCT: u32 = 10;
    pub const ENUM: u32 = 11;
    pub const PROPERTY: u32 = 12;
    pub const ENUM_MEMBER: u32 = 13;
    pub const NAMESPACE: u32 = 14;
}

pub fn position_at_end(source: &str, line: &mut u32, character: &mut u32) -> Position {
    for c in source.chars() {
        if c == '\n' {
            *line += 1;
            *character = 0;
        } else if c != '\r' {
            *character += 1;
        }
    }
    Position {
        line: *line,
        character: *character,
    }
}

pub fn offset_at_position(source: &str, position: Position) -> Option<usize> {
    if position.line == 0 && position.character == 0 {
        return Some(0);
    }
    let mut line = 0;
    let mut character = 0;
    let mut byte = 0;
    for c in source.chars() {
        if c == '\n' {
            line += 1;
            character = 0;
        } else if c != '\r' {
            character += 1;
        }
        byte += c.len_utf8();
        if line == position.line && character == position.character {
            return Some(byte);
        }
        if line > position.line {
            break;
        }
    }
    None
}

struct Backend {
    client: Client,
    projects: RwLock<Vec<Project>>,
}

impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions::default()),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: vec![
                                    SemanticTokenType::KEYWORD,
                                    SemanticTokenType::OPERATOR,
                                    SemanticTokenType::NUMBER,
                                    SemanticTokenType::VARIABLE,
                                    SemanticTokenType::STRING,
                                    SemanticTokenType::FUNCTION,
                                    SemanticTokenType::PARAMETER,
                                    SemanticTokenType::TYPE,
                                    SemanticTokenType::COMMENT,
                                    SemanticTokenType::new("delimiter"),
                                    SemanticTokenType::STRUCT,
                                    SemanticTokenType::ENUM,
                                    SemanticTokenType::PROPERTY,
                                    SemanticTokenType::ENUM_MEMBER,
                                    SemanticTokenType::NAMESPACE,
                                ],
                                token_modifiers: Vec::new(),
                            },
                            full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                            ..Default::default()
                        },
                    ),
                ),
                document_formatting_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        eprintln!("Received did_open event.");
        let uri = params.text_document.uri;
        let source = params.text_document.text;
        let version = params.text_document.version;
        let Some(path) = uri_to_path(&uri) else {
            eprintln!("The provided URI is not a file: '{}'", uri.as_str());
            return;
        };
        let Some(root_path) = find_root(path) else {
            return;
        };
        let mut projects = self.projects.write().await;
        for project in projects.iter_mut() {
            if let Some(file) = project.get_file_mut(&uri) {
                file.source = source;
                file.version = Some(version);
                eprintln!("Updated file: '{}'", uri.as_str());
                self.publish_diagnostics(&project).await;
                return;
            }
        }
        if let Some(project) = Project::new(&root_path) {
            eprintln!("Registered file: '{}'", uri.as_str());
            self.publish_diagnostics(&project).await;
            projects.push(project);
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        eprintln!("Received did_change event.");
        let start_instant = Instant::now();
        let uri = params.text_document.uri;
        let mut projects = self.projects.write().await;
        let Some(project) = projects
            .iter_mut()
            .find(|project| project.get_file(&uri).is_some())
        else {
            eprintln!(
                "ERROR: Unopened file received did_change notification: '{}'",
                uri.as_str()
            );
            return;
        };
        let file = project.get_file_mut(&uri).unwrap();
        let new_version = params.text_document.version;
        if file.version.is_some_and(|version| version >= new_version) {
            return;
        }
        let source = &mut file.source;
        for change in params.content_changes {
            match change.range {
                Some(range) => {
                    assert!(range.start <= range.end);
                    let Some((start, end)) = offset_at_position(source, range.start)
                        .zip(offset_at_position(source, range.end))
                    else {
                        eprintln!("ERROR: Invalid change range: {range:?}");
                        return;
                    };
                    source.replace_range(start..end, &change.text);
                }
                None => *source = change.text,
            }
        }
        file.version = Some(new_version);
        let end_instant = Instant::now();
        eprintln!(
            "Updated file ({:?}): '{}'",
            end_instant - start_instant,
            file.name
        );
        self.publish_diagnostics(project).await;
    }

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        // TODO

        Ok(Some(CompletionResponse::Array(vec![
            CompletionItem::new_simple("Hello".to_string(), "Some detail".to_string()),
            CompletionItem::new_simple("Bye".to_string(), "More detail".to_string()),
        ])))
    }

    async fn hover(&self, _: HoverParams) -> Result<Option<Hover>> {
        // TODO: show type and docstring in hover window
        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String("You're hovering!".to_string())),
            range: None,
        }))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        eprintln!("Semantic tokens (full) requested.");
        let uri = &params.text_document.uri;
        let projects_guard = self.projects.read().await;
        let Some(file) = get_file(&projects_guard, uri) else {
            eprintln!("Semantic tokens: File '{}' is not open.", uri.as_str());
            return Ok(None); // TODO: file-not-open error
        };
        let Some(tokens) = extract_semantic_tokens(file) else {
            return Ok(None);
        };
        let result_id = file
            .version
            .expect("Requested semantic tokens for an unopened file")
            .to_string();

        drop(projects_guard); // Prevent permanent stall caused by trying to write while guard is held
        self.store_semantic_tokens(uri, result_id.clone(), tokens.clone())
            .await;
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: Some(result_id),
            data: tokens,
        })))
    }

    async fn semantic_tokens_full_delta(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> Result<Option<SemanticTokensFullDeltaResult>> {
        eprintln!(
            "Semantic tokens (delta, previous_result_id {}) requested.",
            params.previous_result_id
        );
        let uri = &params.text_document.uri;
        let projects_guard = self.projects.read().await;
        let Some(file) = get_file(&projects_guard, uri) else {
            eprintln!("Semantic tokens: File '{}' is not open.", uri.as_str());
            return Ok(None); // TODO: file-not-open error
        };
        let Some((_, old_semantic_tokens)) = file.semantic_tokens.get(&params.previous_result_id)
        else {
            eprintln!(
                "Semantic tokens: previous tokens not stored for result_id '{}'",
                params.previous_result_id
            );
            return Err(jsonrpc::Error::internal_error());
        };
        let Some(tokens) = extract_semantic_tokens(file) else {
            return Ok(None);
        };
        let result_id = file
            .version
            .expect("Requested semantic tokens for an unopened file")
            .to_string();
        let edits = semantic_tokens::compute_delta(old_semantic_tokens, &tokens);

        drop(projects_guard); // Prevent permanent stall caused by trying to write while guard is held
        self.store_semantic_tokens(uri, result_id.clone(), tokens.clone())
            .await;
        eprintln!(
            "Semantic tokens (delta, result_id {result_id}) computed {} edits",
            edits.len(),
        );
        Ok(Some(SemanticTokensFullDeltaResult::TokensDelta(
            SemanticTokensDelta {
                result_id: Some(result_id),
                edits,
            },
        )))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let projects = self.projects.read().await;
        let file = get_file(&projects, &params.text_document.uri).unwrap();
        let mut output = String::with_capacity(file.source.len());
        match format::format(&file.source, &mut output) {
            Some(Ok(())) => Ok(Some(vec![TextEdit {
                // Overwrite the entire file.
                // TODO: only report changes
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: position_at_end(&file.source, &mut 0, &mut 0),
                },
                new_text: output,
            }])),
            Some(Err(error)) => Err(jsonrpc::Error {
                message: format!("Failed to write to temporary string. Error: {error}").into(),
                ..jsonrpc::Error::internal_error()
            }),
            None => {
                eprintln!("Failed to format file '{}' due to parse errors.", file.name);
                Ok(None)
            }
        }
    }

    // TODO: async fn symbol()

    // TODO: go-to-definition

    // TODO: inlay hint? probably not
}

impl Backend {
    async fn publish_diagnostics(&self, project: &Project) {
        eprintln!("Extracting diagnostics");
        for (file, diagnostics) in extract_diagnostics(project) {
            self.client
                .publish_diagnostics(file.uri.clone(), diagnostics, file.version)
                .await;
        }
    }

    async fn store_semantic_tokens(
        &self,
        file_uri: &Uri,
        result_id: String,
        tokens: Vec<SemanticToken>,
    ) {
        for project in self.projects.write().await.iter_mut() {
            let map = &mut project.get_file_mut(file_uri).unwrap().semantic_tokens;
            if map.len() > MAX_SEMANTIC_TOKENS_LISTS {
                let (oldest_key, _) = map
                    .iter()
                    .min_by_key(|(_, (timestamp, _))| timestamp)
                    .unwrap();
                map.remove(&oldest_key.clone());
            }
            map.insert(result_id.clone(), (Instant::now(), tokens.clone()));
        }
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        projects: RwLock::new(Vec::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
