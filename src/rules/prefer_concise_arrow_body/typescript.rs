//! Tests for prefer-concise-arrow-body (oxc backend).

use super::oxc_typescript::Check;
use crate::rules::test_helpers::run_oxc;

fn run(src: &str) -> Vec<crate::diagnostic::Diagnostic> {
    run_oxc(&Check, src)
}

#[test]
fn flags_variable_arrow_with_single_return() {
    let d = run("const f = () => { return compute(); };");
    assert_eq!(d.len(), 1);
}

#[test]
fn flags_callback_arrow_with_single_return() {
    // Regression: attach-gamme-product.ts:39
    let d = run("const ids = items.map((x) => { return x.field; });");
    assert_eq!(d.len(), 1);
}

#[test]
fn flags_object_literal_return() {
    let d = run("const f = () => { return { a: 1 }; };");
    assert_eq!(d.len(), 1);
}

#[test]
fn ignores_already_concise_arrow() {
    let d = run("const f = () => compute(); const g = (x) => x.field;");
    assert_eq!(d.len(), 0);
}

#[test]
fn ignores_bare_return() {
    let d = run("const f = () => { return; };");
    assert_eq!(d.len(), 0);
}

#[test]
fn ignores_multi_statement_block() {
    let d = run("const f = () => { doSomething(); return value; };");
    assert_eq!(d.len(), 0);
}

#[test]
fn ignores_empty_block() {
    let d = run("const f = () => {};");
    assert_eq!(d.len(), 0);
}

#[test]
fn ignores_block_with_comment() {
    let d = run("const f = () => {\n  // keep this note\n  return value;\n};");
    assert_eq!(d.len(), 0);
}
