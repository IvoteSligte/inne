use parser::Rich;

use ariadne::{Color, Label, Report, ReportKind, sources};

pub use parser::Span;

pub struct Error<'a> {
    pub filename: &'a str,
    pub message: String,
    pub span: Span,
    pub contexts: Vec<(String, Span)>,
}

pub fn print_parse_errors<E>(
    errors: Vec<Rich<'_, E>>,
    filename: &'static str,
    source: &str,
    stage: &'static str,
) where
    E: std::fmt::Display,
{
    for err in errors {
        Report::build(ReportKind::Error, (filename, err.span().into_range()))
            .with_config(ariadne::Config::new().with_index_type(ariadne::IndexType::Byte))
            .with_message(format!("during {stage}, {}", err))
            .with_label(
                Label::new((filename, err.span().into_range()))
                    .with_message(err.reason())
                    .with_color(Color::Red),
            )
            .with_labels(err.contexts().map(|(label, span)| {
                Label::new((filename, span.into_range()))
                    .with_message(format!("while parsing this {label}"))
                    .with_color(Color::Yellow)
            }))
            .finish()
            .eprint(sources([(filename, source)]))
            .unwrap()
    }
}

pub fn print_ir_errors<'a>(
    errors: impl IntoIterator<Item = &'a Error<'a>>,
    source: &str,
    filename: &'static str,
    stage: &str,
) {
    for err in errors {
        Report::build(ReportKind::Error, (filename, err.span.into_range()))
            .with_config(ariadne::Config::new().with_index_type(ariadne::IndexType::Byte))
            .with_message(format!("during {stage}, {}", err.message))
            .with_label(
                Label::new((filename, err.span.into_range()))
                    .with_message(&err.message)
                    .with_color(Color::Red),
            )
            .with_labels(err.contexts.iter().map(|(message, span)| {
                Label::new((filename, span.into_range()))
                    .with_message(message)
                    .with_color(Color::Yellow)
            }))
            .finish()
            .eprint(sources([(filename, &source)]))
            .unwrap()
    }
}
