use std::sync::Arc;

mod lexer;
mod parser;

pub use lexer::{Span, Spanned, Token, lexer};

pub type Source = Arc<String>;

fn main() {
    println!("Hello, world!");
}
