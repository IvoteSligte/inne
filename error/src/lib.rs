use parser::{LexError, Rich};

use ariadne::{Color, FnCache, Label, Report, ReportKind, Source};

pub use parser::Span;

pub struct Error {
    pub message: String,
    pub span: Span,
    pub contexts: Vec<(String, Span)>,
}

impl Error {
    pub fn from_lex_error((message, span): LexError) -> Self {
        Self {
            message: format!("{message}"),
            span,
            contexts: Vec::new(),
        }
    }

    pub fn from_parse_error<E>(error: Rich<'_, E>) -> Self
    where
        E: std::fmt::Display,
    {
        Self {
            message: format!("{}", error.reason()),
            span: *error.span(),
            contexts: error
                .contexts()
                .map(|(label, span)| {
                    let message = format!("while parsing this {label}");
                    (message, *span)
                })
                .collect(),
        }
    }

    pub fn eprint(&self, source: &str, filename: &str, stage: &str) {
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
            .with_message(format!("during {stage}, {}", self.message))
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
pub struct Errors {
    inner: Vec<Error>,
}

impl Errors {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn add(&mut self, message: impl ToString, span: Span) {
        self.add_with_contexts(message, span, Vec::new());
    }

    pub fn add_with_contexts(
        &mut self,
        message: impl ToString,
        span: Span,
        contexts: Vec<(String, Span)>,
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
        span: Span,
        context_message: impl ToString,
        context_span: Span,
    ) {
        self.add_with_contexts(
            message,
            span,
            vec![(context_message.to_string(), context_span)],
        );
    }

    pub fn into_inner(self) -> Vec<Error> {
        self.inner
    }
}
