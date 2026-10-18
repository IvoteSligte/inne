use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};

use semantic_tokens::extract_semantic_tokens;
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

use inne::*;

mod semantic_tokens;

// NOTE: The LSP protocol uses UTF-16 by default, so care should be taken when counting characters.

/// Token types.
mod tt {
    pub const KEYWORD: u32 = 0;
    pub const OPERATOR: u32 = 1;
    pub const NUMBER: u32 = 2;
    pub const VARIABLE: u32 = 3;
    pub const STRING: u32 = 4;
    pub const FUNCTION: u32 = 5;
    pub const PARAMETER: u32 = 6;
    pub const TYPE: u32 = 7;
}

struct File {
    version: i32,
    /// Human-readable filename.
    name: String,
    /// Absolute path of the file.
    _path: PathBuf,
    /// Source-code contained in the file.
    source: String,
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
            name,
            _path: path,
            source,
        })
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
                                ],
                                token_modifiers: Vec::new(),
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: None,
                        inter_file_dependencies: true,
                        workspace_diagnostics: true,
                        ..Default::default()
                    },
                )),
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
        if let Some(file) = File::new(&uri, source, params.text_document.version) {
            eprintln!("Registered file: '{}'", uri.as_str());
            self.files.write().await.insert(uri, file);
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        eprintln!("Received did_change event.");
        let uri = params.text_document.uri;
        let mut write_guard = self.files.write().await;
        let Some(source) = params.content_changes.get(0).map(|change| &change.text) else {
            eprintln!("did_change event did not contain any changes.");
            return;
        };
        let new_version = params.text_document.version;
        match write_guard.entry(uri.clone()) {
            Entry::Occupied(mut occupied_entry) => {
                let entry = occupied_entry.get_mut();
                if entry.version < new_version {
                    eprintln!("Updated file: '{}'", uri.as_str());
                    entry.source = source.to_owned();
                    entry.version = new_version;
                }
            }
            Entry::Vacant(vacant_entry) => {
                if let Some(file) = File::new(&uri, source, new_version) {
                    eprintln!("Registered file (from did_change): '{}'", uri.as_str());
                    vacant_entry.insert(file);
                }
            }
        };
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
        let read_guard = self.files.read().await;
        let uri = &params.text_document.uri;
        let Some(file) = read_guard.get(uri) else {
            eprintln!("Semantic tokens: File '{}' is not open.", uri.as_str());
            return Ok(None); // TODO: file-not-open error
        };
        Ok(extract_semantic_tokens(file))
    }

    // TODO: async fn symbol()

    // TODO: go-to-definition

    // TODO: async fn formatting

    // TODO: inlay hint? probably not
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
