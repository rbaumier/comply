//! Cross-backend scenarios for `catch-error-name`.
//!
//! Verifies that the TypeScript and Vue backends agree on the same
//! catch-parameter verdicts for identical try/catch snippets.

#![cfg(test)]

use crate::diagnostic::Diagnostic;

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_ts(src, &super::typescript::Check)
}

fn run_vue(body: &str) -> Vec<Diagnostic> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_vue_updated::language())
        .expect("vue grammar");
    let src = format!("<script>\n{body}\n</script>");
    let tree = parser.parse(&src, None).expect("parse");
    let path = std::path::PathBuf::from("t.vue");
    let ctx = crate::rules::backend::CheckCtx::for_test(&path, &src);
    use crate::rules::backend::AstCheck;
    super::vue::Check.check(&ctx, &tree)
}

#[test]
fn flags_catch_e_cross_backend() {
    let body = "try { f(); } catch (e) {}";
    assert_eq!(run_ts(body).len(), 1);
    assert_eq!(run_vue(body).len(), 1);
}

#[test]
fn flags_catch_err_cross_backend() {
    let body = "try { f(); } catch (err) {}";
    assert_eq!(run_ts(body).len(), 1);
    assert_eq!(run_vue(body).len(), 1);
}

#[test]
fn allows_catch_error_cross_backend() {
    let body = "try { f(); } catch (error) {}";
    assert!(run_ts(body).is_empty());
    assert!(run_vue(body).is_empty());
}

#[test]
fn allows_suffixed_error_cross_backend() {
    let body = "try { f(); } catch (parseError) {}";
    assert!(run_ts(body).is_empty());
    assert!(run_vue(body).is_empty());
}

#[test]
fn allows_bare_catch_cross_backend() {
    let body = "try { f(); } catch {}";
    assert!(run_ts(body).is_empty());
    assert!(run_vue(body).is_empty());
}
