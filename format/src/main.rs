use std::{ffi::OsStr, path::PathBuf};

use clap::Parser;

use inne_format::*;

#[derive(clap::Parser)]
struct Args {
    files: Vec<PathBuf>,

    #[arg(long)]
    dry_run: bool,
}

fn main() {
    let args = Args::parse();

    for path in args.files {
        if path.extension() != Some(OsStr::new("inne")) {
            eprintln!(
                "File '{}' does not end in '.inne'. Skipping.",
                path.display()
            );
            continue;
        }
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("Failed to open file: {error}");
                continue;
            }
        };
        let mut output = String::with_capacity(source.len());
        match format(&source, &mut output) {
            Some(Ok(())) => {}
            Some(Err(error)) => {
                eprintln!("Failed to write to file '{}': {error}", path.display());
                continue;
            }
            None => {
                eprintln!(
                    "Failed to format file '{}' due to parse errors.",
                    path.display()
                );
                continue;
            }
        }
        eprintln!("--- Successfully formatted file '{}' ---", path.display());
        if args.dry_run {
            println!("{output}");
            continue;
        }
        if let Err(error) = std::fs::write(&path, output) {
            eprintln!("Failed to write to file '{}': {error}", path.display());
        }
    }
}
