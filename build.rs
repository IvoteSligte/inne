use std::io::Write;
use std::{ffi::OsStr, path::Path};

// Generates a test case for each .inne file in ./test-cases/
fn main() -> std::io::Result<()> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let cases_dir = Path::new(&manifest_dir).join("test-cases");
    println!("cargo::rerun-if-changed={}", cases_dir.display());

    let mut test_cases = std::fs::read_dir(&cases_dir)?
        .map(|file| {
            file.unwrap_or_else(|err| panic!("Failed to open test case: {err}"))
                .path()
        })
        .filter(|path| path.extension() == Some(OsStr::new("inne")))
        .collect::<Vec<_>>();
    test_cases.sort();

    let mut output_file = std::fs::File::create(Path::new(&out_dir).join("test_cases.rs"))?;
    for test_path in test_cases {
        let test_name = test_path.file_stem().unwrap().to_str().unwrap();
        assert!(
            test_name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_'),
            "Invalid test name: '{}'",
            test_name
        );
        let test_file = std::fs::read_to_string(&test_path)?;

        write!(
            output_file,
            "#[test]
fn {test_name}() {{
    run({test_name:?}, {test_file:?})
}}
"
        )?;
    }

    Ok(())
}
