use crate::{File, position_at_end};
use tower_lsp_server::ls_types::*;

pub fn extract_diagnostics(file: &File) -> Vec<Diagnostic> {
    let mut errors = file.analyze();
    if errors.is_empty() {
        return Vec::new();
    }
    errors.sort_by_key(|error| error.span.start);
    let mut line = 0u32;
    let mut character = 0u32;
    let mut offset = 0usize;
    errors
        .into_iter()
        .map(|error| {
            let start = position_at_end(
                &file.source[offset..error.span.start],
                &mut line,
                &mut character,
            );
            let end = position_at_end(
                &file.source[error.span.start..error.span.end],
                &mut line,
                &mut character,
            );
            offset = error.span.end;
            Diagnostic {
                range: Range { start, end },
                severity: Some(DiagnosticSeverity::ERROR),
                message: error.message,
                // related_information: todo!(), // TODO: error.contexts
                ..Default::default()
            }
        })
        .collect()
}
