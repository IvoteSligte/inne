use parser::{LexError, ParseError};

use ariadne::{Color, FnCache, Label, Report, ReportKind, Source};

pub use parser::Span;

pub struct Error<'a> {
    pub message: String,
    pub span: Span<'a>,
    pub contexts: Vec<(String, Span<'a>)>,
}

impl<'a> Error<'a> {
    pub fn from_lex_error((message, span): LexError<'a>) -> Self {
        Self {
            message: format!("{message}"),
            span,
            contexts: Vec::new(),
        }
    }

    pub fn from_parse_error((message, span): ParseError<'a>) -> Self {
        Self {
            message,
            span,
            contexts: Vec::new(),
        }
    }

    pub fn source(&self) -> &'a str {
        self.span.source
    }

    pub fn eprint(&self) {
        let Span {
            filename, source, ..
        } = self.span;
        // ariadne::sources has lifetime issues that are resolved by inlining it.
        let sources = {
            let iter = [(filename, source)];
            FnCache::new((move |id| Err(format!("Failed to fetch source '{}'", id))) as fn(&_) -> _)
                .with_sources(
                    iter.into_iter()
                        .map(|(id, s)| (id, Source::from(s)))
                        .collect(),
                )
        };
        Report::build(ReportKind::Error, (filename, self.span.into_range()))
            .with_config(ariadne::Config::new().with_index_type(ariadne::IndexType::Byte))
            .with_message(&self.message)
            .with_label(
                Label::new((filename, self.span.into_range()))
                    .with_message(&self.message)
                    .with_color(Color::Red),
            )
            .with_labels(self.contexts.iter().map(|(message, span)| {
                Label::new((filename, span.into_range()))
                    .with_message(message)
                    .with_color(Color::Yellow)
            }))
            .finish()
            .eprint(sources)
            .unwrap()
    }
}

/// Helper type for managing error messages.
pub struct Errors<'a> {
    inner: Vec<Error<'a>>,
}

impl<'a> Errors<'a> {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn add(&mut self, message: impl ToString, span: Span<'a>) {
        self.add_with_contexts(message, span, Vec::new());
    }

    pub fn add_with_contexts(
        &mut self,
        message: impl ToString,
        span: Span<'a>,
        contexts: Vec<(String, Span<'a>)>,
    ) {
        self.inner.push(Error {
            message: message.to_string(),
            span,
            contexts,
        });
    }

    pub fn add_with_context(
        &mut self,
        message: impl ToString,
        span: Span<'a>,
        context_message: impl ToString,
        context_span: Span<'a>,
    ) {
        self.add_with_contexts(
            message,
            span,
            vec![(context_message.to_string(), context_span)],
        );
    }

    pub fn into_inner(self) -> Vec<Error<'a>> {
        self.inner
    }
}
