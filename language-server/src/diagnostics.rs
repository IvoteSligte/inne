use std::{collections::HashMap, time::Instant};

use crate::{File, Project, position_at_end};
use inne::Error;
use tower_lsp_server::ls_types::*;

pub fn extract_diagnostics(project: &Project) -> Vec<(&File, Vec<Diagnostic>)> {
    let start_instant = Instant::now();
    let project_errors = project.analyze();
    let end_instant = Instant::now();
    let duration = end_instant - start_instant;
    eprintln!(
        "Extracted {} diagnostics ({duration:?}) for project",
        project_errors.len(),
    );
    group_errors_by_file(&project.files, project_errors)
        .into_iter()
        .map(|(file, errors)| (file, errors_to_diagnostics(errors)))
        .collect()
}

fn errors_to_diagnostics(mut errors: Vec<Error<'_>>) -> Vec<Diagnostic> {
    errors.sort_by_key(|error| error.span.start);
    let mut line = 0u32;
    let mut character = 0u32;
    let mut offset = 0usize;
    errors
        .into_iter()
        .filter_map(|error| {
            if error.span.start < offset {
                eprintln!("WARN: Overlapping diagnostics. The previous diagnostic ended at {offset}, but this one starts at {}", error.span.start);
                return None;
            }
            let start = position_at_end(
                &error.source()[offset..error.span.start],
                &mut line,
                &mut character,
            );
            let end = position_at_end(
                &error.source()[error.span.start..error.span.end],
                &mut line,
                &mut character,
            );
            offset = error.span.end;
            Some(Diagnostic {
                range: Range { start, end },
                severity: Some(DiagnosticSeverity::ERROR),
                message: error.message,
                // related_information: todo!(), // TODO: error.contexts
                ..Default::default()
            })
        })
        .collect::<Vec<_>>()
}

/// Assumes that `File::name` is unique for each file.
fn group_errors_by_file<'file, 'src>(
    files: &'file [File],
    errors: Vec<Error<'src>>,
) -> Vec<(&'file File, Vec<Error<'src>>)> {
    let mut filename_map = HashMap::<&str, Vec<Error<'_>>>::new();
    for error in errors {
        filename_map
            .entry(error.span.filename)
            .or_default()
            .push(error);
    }
    // FIXME: this assert has triggered; probably because of invalid URI comparisons.
    debug_assert!(
        filename_map.len() <= files.len(),
        "Somehow, there are errors for more unique files than there are known files in the project."
    );
    files
        .iter()
        .map(|file| {
            let errors = filename_map.remove(file.name.as_str()).unwrap_or_default();
            (file, errors)
        })
        .collect()
}
