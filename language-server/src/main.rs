use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};
use std::time::Instant;

use diagnostics::extract_diagnostics;
use semantic_tokens::extract_semantic_tokens;
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::{self, Result};
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

use inne::*;

mod diagnostics;
mod semantic_tokens;

const MAX_SEMANTIC_TOKENS_LISTS: usize = 3;

// TODO: format-on-save

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

struct File {
    version: i32,
    uri: Uri,
    /// Human-readable filename.
    name: String,
    /// Absolute path of the file.
    _path: PathBuf,
    /// Source-code contained in the file.
    source: String,
    /// The last-computed list of semantic tokens and its `result_id`.
    semantic_tokens: HashMap<String, (Instant, Vec<SemanticToken>)>,
}

impl File {
    pub fn new(uri: &Uri, source: impl ToString, version: i32) -> Option<Self> {
        let source = source.to_string();
        let name = uri.as_str().to_string(); // TODO: better human-readable name
        let Some(path) = uri.to_file_path().as_deref().map(Path::to_path_buf) else {
            eprintln!("The provided URI is not a file: '{}'", uri.as_str());
            return None;
        };
        Some(Self {
            version,
            uri: uri.clone(),
            name,
            _path: path,
            source,
            semantic_tokens: HashMap::new(),
        })
    }

    pub fn analyze(&self) -> Vec<Error> {
        let mut all_errors = Vec::new();
        eprintln!("Lexing file '{}'", self.name);
        let (maybe_tokens, errors) = inne::lex(&self.source);
        all_errors.extend(errors);
        let Some(tokens) = maybe_tokens else {
            return all_errors;
        };
        eprintln!("Parsing file '{}'", self.name);
        let (maybe_ast, errors) = inne::parse(&tokens, &self.source, &self.name);
        all_errors.extend(errors);
        let Some(ast) = maybe_ast else {
            return all_errors;
        };
        eprintln!("Converting AST to HIR for file '{}'", self.name);
        let hir = match inne::hir_from_ast(&ast) {
            Ok(hir) => hir,
            Err(errors) => {
                all_errors.extend(errors);
                return all_errors;
            }
        };
        eprintln!("Converting HIR to Typed HIR for file '{}'", self.name);
        let _typed_hir = match inne::typed_hir_from_hir(hir) {
            Ok(typed_hir) => typed_hir,
            Err(errors) => {
                all_errors.extend(errors);
                return all_errors;
            }
        };
        eprintln!(
            "Analyzing file '{}' yielded {} errors",
            self.name,
            all_errors.len()
        );
        // NOTE: THIR -> MIR currently does not produce diagnostics, so it is skipped
        all_errors
    }
}

struct Backend {
    client: Client,
    files: RwLock<HashMap<Uri, File>>,
}

impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL, // TODO: incremental updates
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
        if let Some(file) = File::new(&uri, source, version) {
            eprintln!("Registered file: '{}'", uri.as_str());
            self.publish_diagnostics(&file).await;
            self.files.write().await.insert(uri, file);
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        eprintln!("Received did_change event.");
        let uri = params.text_document.uri;
        let mut write_guard = self.files.write().await;
        let Some(source) = params.content_changes.get(0).map(|change| &change.text) else {
            eprintln!(
                "did_change event did not contain any changes for file '{}'.",
                uri.as_str()
            );
            return;
        };
        let new_version = params.text_document.version;
        match write_guard.entry(uri.clone()) {
            Entry::Occupied(mut occupied_entry) => {
                let file = occupied_entry.get_mut();
                if file.version < new_version {
                    eprintln!("Updated file: '{}'", file.name);
                    file.source = source.to_owned();
                    file.version = new_version;
                    self.publish_diagnostics(&file).await;
                }
            }
            Entry::Vacant(vacant_entry) => {
                if let Some(file) = File::new(&uri, source, new_version) {
                    eprintln!("Registered file (from did_change): '{}'", file.name);
                    self.publish_diagnostics(&file).await;
                    vacant_entry.insert(file);
                }
            }
        }
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
        let guard = self.files.read().await;
        let Some(file) = guard.get(uri) else {
            eprintln!("Semantic tokens: File '{}' is not open.", uri.as_str());
            return Ok(None); // TODO: file-not-open error
        };
        let Some(tokens) = extract_semantic_tokens(file) else {
            return Ok(None);
        };
        let result_id = file.version.to_string();

        drop(guard); // Prevent permanent stall caused by trying to write while guard is held
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
        let guard = self.files.read().await;
        let Some(file) = guard.get(uri) else {
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
        let result_id = file.version.to_string();
        let edits = semantic_tokens::compute_delta(old_semantic_tokens, &tokens);

        drop(guard); // Prevent permanent stall caused by trying to write while guard is held
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
        let guard = self.files.read().await;
        let Some(file) = guard.get(&params.text_document.uri) else {
            return Err(jsonrpc::Error::invalid_params("Document not found."));
        };
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
    async fn publish_diagnostics(&self, file: &File) {
        eprintln!("Extracting diagnostics from file '{}'", file.name);
        let diagnostics = extract_diagnostics(file);
        self.client
            .publish_diagnostics(file.uri.clone(), diagnostics, Some(file.version))
            .await;
    }

    async fn store_semantic_tokens(
        &self,
        file_uri: &Uri,
        result_id: String,
        tokens: Vec<SemanticToken>,
    ) {
        let mut guard = self.files.write().await;
        let map = &mut guard.get_mut(file_uri).unwrap().semantic_tokens;
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

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        files: RwLock::new(HashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
