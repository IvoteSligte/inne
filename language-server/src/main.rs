use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

use inne::*;

// NOTE: The LSP protocol uses UTF-16 by default, so care should be taken when counting characters.

#[derive(Debug)]
struct File {
    /// Human-readable filename.
    _name: String,
    /// Absolute path of the file.
    _path: PathBuf,
    /// Source-code contained in the file.
    source: String,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    files: RwLock<HashMap<Uri, File>>,
}

impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL, // TODO: incremental
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
                                ],
                                token_modifiers: Vec::new(),
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
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
        let uri = params.text_document.uri;
        let Some(path) = uri.to_file_path().as_deref().map(Path::to_path_buf) else {
            eprintln!("The provided URI is not a file: '{}'", uri.as_str());
            return;
        };
        let source = params.text_document.text;
        let name = uri.as_str().to_string(); // TODO: better human-readable name
        eprintln!("Registered file: '{}'", uri.as_str());
        self.files.write().await.insert(
            uri,
            File {
                _name: name,
                _path: path,
                source,
            },
        );
    }

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::Array(vec![
            CompletionItem::new_simple("Hello".to_string(), "Some detail".to_string()),
            CompletionItem::new_simple("Bye".to_string(), "More detail".to_string()),
        ])))
    }

    async fn hover(&self, _: HoverParams) -> Result<Option<Hover>> {
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
        let Some(file) = read_guard.get(&params.text_document.uri) else {
            return Ok(None); // TODO: file-not-open error
        };
        let (Some(tokens), _) = inne::lex(&file.source) else {
            return Ok(None);
        };
        let data = to_semantic_tokens(
            &file.source,
            tokens.into_iter().filter_map(|(token, span)| {
                let token_type = match token {
                    Token::Ident(_) => 3,         // VARIABLE
                    Token::Int(_) => 1,           // NUMBER
                    Token::Bool(_) => 1,          // NUMBER
                    Token::String(_) => 4,        // STRING
                    Token::Op(_) => 2,            // OPERATOR
                    _ if token.is_keyword() => 0, // KEYWORD
                    _ => return None,
                };
                Some((token_type, span))
            }),
        );
        // let (Some(parse_tree), _) = inne::parse(&tokens, &file.source, &file.name) else {
        //     return Ok(None);
        // };
        eprintln!("Semantic tokens (full) provided.");
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    // TODO: async fn symbol()

    // TODO: go-to-definition

    // TODO: async fn formatting

    // TODO: inlay hint? probably not
}

fn to_semantic_tokens(
    source: &str,
    tokens: impl IntoIterator<Item = (u32, Span)>,
) -> Vec<SemanticToken> {
    let mut start_byte = 0usize;
    tokens
        .into_iter()
        .map(|(token_type, span)| {
            let mut delta_line = 0;
            let mut delta_start = 0; // character/column
            for c in source[start_byte..span.start].chars() {
                if c == '\n' {
                    delta_line += 1;
                    delta_start = 0;
                } else {
                    delta_start += 1;
                }
            }
            start_byte = span.start;
            let length = (span.end - span.start) as _;
            SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: 0,
            }
        })
        .collect()
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
