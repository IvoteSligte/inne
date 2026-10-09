use std::sync::Arc;

pub mod ast;
mod lexer;
mod parser;

pub use lexer::{Span, Spanned, Token, lexer};
pub use parser::{ParseTree, parser};

pub type Source = Arc<String>;

fn main() {
    println!("Hello, world!");
}
