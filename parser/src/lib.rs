pub mod ast;
pub mod diagnosis;
pub mod lexer;
pub mod parser;
pub mod token;
mod util;

pub use ast::*;
pub use lexer::*;
pub use parser::*;
pub use token::*;

#[derive(Clone, Copy, Eq, Ord, Hash)]
pub struct Span<'src> {
    pub start: usize,
    pub end: usize,
    pub filename: &'src str,
    pub source: &'src str,
}

// Manual impl to prevent comparing `source` fields.
impl<'src> PartialOrd for Span<'src> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.start.partial_cmp(&other.start) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.end.partial_cmp(&other.end) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.filename.partial_cmp(&other.filename)
    }
}

// Manual impl to prevent comparing `source` fields.
impl<'src> PartialEq for Span<'src> {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start && self.end == other.end && self.filename == other.filename
    }
}

impl<'src> Span<'src> {
    pub fn new(start: usize, end: usize, filename: &'src str, source: &'src str) -> Self {
        Self {
            start,
            end,
            filename,
            source,
        }
    }

    pub fn union(self, other: Self) -> Self {
        debug_assert_eq!(self.filename, other.filename);
        Self {
            start: self.start,
            end: other.end,
            filename: self.filename,
            source: self.source,
        }
    }

    pub fn contains(&self, offset: usize) -> bool {
        self.start <= offset && self.end > offset
    }

    pub fn into_range(self) -> std::ops::Range<usize> {
        self.into()
    }
}

impl std::fmt::Debug for Span<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as std::fmt::Display>::fmt(self, f)
    }
}

impl std::fmt::Display for Span<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.start.fmt(f)?;
        f.write_str("..")?;
        self.end.fmt(f)
    }
}

impl From<Span<'_>> for std::ops::Range<usize> {
    fn from(Span { start, end, .. }: Span) -> Self {
        Self { start, end }
    }
}

pub type Spanned<'a, T> = (T, Span<'a>);
