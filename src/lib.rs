pub mod ast;
pub mod hir;
pub mod interpreter;
pub mod mir;
pub mod typed_hir;

pub use ast::{Ast, ast_from_parse_tree};
pub use hir::{Hir, hir_from_ast};
pub use inne_parser as parser;
pub use interpreter::Interpreter;
pub use mir::{Mir, mir_from_typed_hir};
pub use parser::{
    Input as _, ParseTree, Parser as _, Rich, Span, Spanned, Token, create_lexer, create_parser,
};
pub use typed_hir::{TypedHir, typed_hir_from_hir};

pub struct Error<'a> {
    pub filename: &'a str,
    pub message: String,
    pub span: Span,
    pub contexts: Vec<(String, Span)>,
}
