use crate::test_utils::parse_and_check_oxc;

use super::register;

fn check(source: &str) -> Vec<crate::diagnostic::Diagnostic> {
    parse_and_check_oxc(register(), source, "test.ts")
}

#[test]
fn flags_variable_arrow_with_single_return() {
    let source = r#"
const f = () => { return compute(); };
"#;
    let diagnostics = check(source);
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn flags_callback_arrow_with_single_return() {
    // Regression: attach-gamme-product.ts:39
    let source = r#"
const ids = items.map((x) => { return x.field; });
"#;
    let diagnostics = check(source);
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn flags_object_literal_return() {
    let source = r#"
const f = () => { return { a: 1 }; };
"#;
    let diagnostics = check(source);
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn ignores_already_concise_arrow() {
    let source = r#"
const f = () => compute();
const g = (x) => x.field;
"#;
    let diagnostics = check(source);
    assert_eq!(diagnostics.len(), 0);
}

#[test]
fn ignores_bare_return() {
    let source = r#"
const f = () => { return; };
"#;
    let diagnostics = check(source);
    assert_eq!(diagnostics.len(), 0);
}

#[test]
fn ignores_multi_statement_block() {
    let source = r#"
const f = () => { doSomething(); return value; };
"#;
    let diagnostics = check(source);
    assert_eq!(diagnostics.len(), 0);
}

#[test]
fn ignores_empty_block() {
    let source = r#"
const f = () => {};
"#;
    let diagnostics = check(source);
    assert_eq!(diagnostics.len(), 0);
}

#[test]
fn ignores_block_with_comment() {
    let source = r#"
const f = () => {
  // keep this note
  return value;
};
"#;
    let diagnostics = check(source);
    assert_eq!(diagnostics.len(), 0);
}
