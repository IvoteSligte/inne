use std::io::Write;
use std::path::Path;

fn create_tests_for_directory(
    dir_name: &str,
    output_file_name: &str,
    assertion: &str,
) -> std::io::Result<()> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let cases_dir = Path::new(&manifest_dir).join(dir_name);
    println!(
        "cargo::rerun-if-changed={}",
        cases_dir.as_os_str().to_str().unwrap()
    );
    let mut test_cases = std::fs::read_dir(&cases_dir)?
        .map(|entry| {
            entry
                .unwrap_or_else(|err| panic!("Failed to register test case: {err}"))
                .path()
        })
        .collect::<Vec<_>>();
    test_cases.sort();

    let mut output_file = std::fs::File::create(Path::new(&out_dir).join(output_file_name))?;
    for test_dir_path in test_cases {
        assert!(
            test_dir_path.is_dir(),
            "Expected a test directory at {}",
            test_dir_path.display()
        );
        let test_path = test_dir_path.join("main.inne");
        assert!(
            test_path.is_file(),
            "Expected a file at {}",
            test_path.display()
        );
        let test_name = test_dir_path.file_stem().unwrap().to_str().unwrap();
        assert!(
            test_name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_'),
            "Invalid test name: '{}'",
            test_name
        );
        let test_path_str = test_path.as_os_str().to_str().unwrap();
        write!(
            output_file,
            "#[test]
fn {test_name}() {{
    let root_path = std::path::PathBuf::from(\"{test_path_str}\");
    let success = inne::compile_and_run(Some(root_path));
    {assertion}
}}
"
        )?;
    }
    Ok(())
}

// Generates a test case for each .inne file in ./test-cases/
fn main() -> std::io::Result<()> {
    create_tests_for_directory(
        "test-cases",
        "test_cases.rs",
        "assert!(success, \"Unexpected compilation or execution error\");",
    )?;
    create_tests_for_directory(
        "test-failure-cases",
        "test_failure_cases.rs",
        "assert!(!success, \"Expected compilation or execution error\");",
    )?;
    Ok(())
}
