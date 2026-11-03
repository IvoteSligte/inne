use inne::*;

fn eprint_errors(errors: &[Error], stage: &str) {
    for error in errors {
        error.eprint(stage);
    }
}

fn run(filename: &str, source: &str) {
    let (maybe_tokens, errors) = lex(source, filename);
    eprint_errors(&errors, "lexing");

    let tokens = maybe_tokens.expect("Failed to lex source");
    let (maybe_ast, errors) = parse(&tokens, source);
    eprint_errors(&errors, "parsing");

    let ast = maybe_ast.expect("Failed to parse source");
    let hir = hir_from_ast_modules(&ast).unwrap_or_else(|errors| {
        eprint_errors(&errors, "symbol resolution");
        panic!("Failed to lower AST to HIR");
    });
    let typed_hir = typed_hir_from_hir(hir).unwrap_or_else(|errors| {
        eprint_errors(&errors, "type checking");
        panic!("Failed to lower HIR to Typed-HIR");
    });
    let mir = mir_from_typed_hir(&typed_hir).unwrap_or_else(|errors| {
        eprint_errors(&errors, "control-flow analysis");
        panic!("Failed to lower Typed-HIR to MIR");
    });
    Interpreter::run(&mir);
}

include!(concat!(env!("OUT_DIR"), "/test_cases.rs"));
