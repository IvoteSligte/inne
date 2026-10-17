use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

use inne::*;

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

#[derive(Debug)]
struct File {
    /// Human-readable filename.
    name: String,
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
                name,
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
        let uri = &params.text_document.uri;
        let Some(file) = read_guard.get(uri) else {
            eprintln!("Semantic tokens: File '{}' is not open.", uri.as_str());
            return Ok(None); // TODO: file-not-open error
        };
        if file.source.is_empty() {
            eprintln!("Semantic tokens: File '{}' is empty.", uri.as_str());
            return Ok(None);
        }
        let (Some(tokens), _) = inne::lex(&file.source) else {
            eprintln!("Semantic tokens: Failed to lex file '{}'.", file.name);
            return Ok(None);
        };
        // First pass reports tokens according to tokens.
        // Using a BTreeMap instead of HashMap to ensure ordering is retained.
        let mut semantic_tokens = BTreeMap::from_iter(tokens.iter().filter_map(|(token, span)| {
            let token_type = match token {
                Token::Ident(_) => tt::VARIABLE, // default for identifiers
                Token::Int(_) => tt::NUMBER,
                Token::Bool(_) => tt::NUMBER,
                Token::String(_) => tt::STRING,
                Token::Op(_) => tt::OPERATOR,
                _ if token.is_keyword() => tt::KEYWORD,
                _ => return None,
            };
            Some((*span, token_type))
        }));
        // Second pass overrides identifiers with their specialised types (if applicable)
        if let (Some(parse_tree), _) = inne::parse(&tokens, &file.source, &file.name) {
            for stmt in &parse_tree.stmts {
                stmt_extract_idents(stmt, &mut |token_type, span| {
                    semantic_tokens.insert(span, token_type);
                });
            }
            eprintln!("Semantic tokens: Failed to parse file '{}'.", file.name);
        }
        let data = to_semantic_tokens(&file.source, semantic_tokens);
        eprintln!("Semantic tokens (full) provided {} tokens.", data.len());
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

fn stmt_extract_idents(stmt: &parser::Stmt, set: &mut impl FnMut(u32, Span)) {
    match stmt {
        parser::Stmt::Func((_, name_span), params, return_type, body) => {
            set(tt::FUNCTION, *name_span);
            for param in params {
                let (_, name_span) = param.0;
                set(tt::PARAMETER, name_span);
                let (_, type_span) = param.1;
                set(tt::TYPE, type_span);
            }
            if let Some((_, type_span)) = return_type {
                set(tt::TYPE, *type_span);
            }
            for stmt in body {
                stmt_extract_idents(stmt, set);
            }
        }
        parser::Stmt::VarDecl((_, name_span), (expr, _)) => {
            set(tt::VARIABLE, *name_span);
            expr_extract_idents(expr, set);
        }
        parser::Stmt::Expr((expr, _)) => expr_extract_idents(expr, set),
        parser::Stmt::Assign((target, _), _, (expr, _)) => {
            expr_extract_idents(target, set);
            expr_extract_idents(expr, set);
        }
        parser::Stmt::If((cond, _), then, else_ifs, r#else) => {
            expr_extract_idents(cond, set);
            for stmt in then {
                stmt_extract_idents(stmt, set);
            }
            for ((cond, _), then) in else_ifs {
                expr_extract_idents(cond, set);
                for stmt in then {
                    stmt_extract_idents(stmt, set);
                }
            }
            if let Some(r#else) = r#else {
                for stmt in r#else {
                    stmt_extract_idents(stmt, set);
                }
            }
        }
        parser::Stmt::For(_, (iter, _), stmts) => {
            expr_extract_idents(iter, set);
            for stmt in stmts {
                stmt_extract_idents(stmt, set);
            }
        }
        parser::Stmt::While((cond, _), stmts) => {
            expr_extract_idents(cond, set);
            for stmt in stmts {
                stmt_extract_idents(stmt, set);
            }
        }
        parser::Stmt::Loop(stmts) => {
            for stmt in stmts {
                stmt_extract_idents(stmt, set);
            }
        }
        parser::Stmt::Return((Some((expr, _)), _)) => {
            expr_extract_idents(expr, set);
        }
        parser::Stmt::Return((None, _)) => {}
        parser::Stmt::Break(_) => {}
        parser::Stmt::Continue(_) => {}
    }
}

fn expr_extract_idents(expr: &parser::Expr, set: &mut impl FnMut(u32, Span)) {
    match expr {
        parser::Expr::Binary(left, _, right) => {
            expr_extract_idents(&left.0, set);
            expr_extract_idents(&right.0, set);
        }
        parser::Expr::Unary(_, value) => expr_extract_idents(&value.0, set),
        parser::Expr::Call((_, name_span), (args, _)) => {
            set(tt::FUNCTION, *name_span);
            for (arg, _) in args {
                expr_extract_idents(arg, set);
            }
        }
        parser::Expr::Var(_) => {} // VARIABLE is the default
        parser::Expr::Int(_) => {}
        parser::Expr::Bool(_) => {}
        parser::Expr::String(_) => {}
        parser::Expr::Range(start, end) => {
            expr_extract_idents(&start.0, set);
            expr_extract_idents(&end.0, set);
        }
    }
}

fn to_semantic_tokens(
    source: &str,
    tokens: impl IntoIterator<Item = (Span, u32)>,
) -> Vec<SemanticToken> {
    let mut start_byte = 0usize;
    tokens
        .into_iter()
        .map(|(span, token_type)| {
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
