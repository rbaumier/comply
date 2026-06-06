//! rust-select-without-biased backend.
//!
//! Walk every `macro_invocation`. If the macro's last name segment is
//! `select`, scan its raw text for the `biased;` keyword. We use a text
//! scan because tree-sitter doesn't parse the inside of a `select!`
//! invocation — its body is an unparsed `token_tree`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["macro_invocation"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        let Some(macro_name_node) = node.child_by_field_name("macro") else {
            return;
        };
        let Ok(macro_name) = macro_name_node.utf8_text(source) else {
            return;
        };
        let last_segment = macro_name.rsplit("::").next().unwrap_or(macro_name);
        if last_segment != "select" {
            return;
        }
        // Only flag tokio::select! — not futures::select!, crossbeam::select!, etc.
        let is_tokio = if macro_name.contains("::") {
            macro_name.starts_with("tokio::")
        } else {
            // Bare `select!` — only flag if file imports from tokio
            ctx.source_contains("use tokio::")
        };
        if !is_tokio {
            return;
        }
        let Ok(text) = node.utf8_text(source) else {
            return;
        };
        // `biased;` must appear before any branch arrow. A naive substring
        // check is enough — `biased` is not a regular identifier in the
        // tokio select! grammar so a false-positive elsewhere is unlikely.
        if has_biased_token(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-select-without-biased",
            "`tokio::select!` without `biased;` polls branches in random \
             order. Add `biased;` as the first directive so the cancel / \
             shutdown branch can't be starved."
                .into(),
            Severity::Warning,
        ));
    }
}

/// True if `text` contains `biased` followed by `;` at a token boundary.
/// We do not require it to be the first directive; users sometimes write
/// `// comment\nbiased;`. False positives from the literal substring
/// `biased` inside a string would require very contrived code, so the
/// simple check is acceptable.
fn has_biased_token(text: &str) -> bool {
    let mut rest = text;
    while let Some(idx) = rest.find("biased") {
        let after = &rest[idx + "biased".len()..];
        let trimmed = after.trim_start();
        if trimmed.starts_with(';') {
            // Look at the byte before `biased` to ensure it's not an
            // identifier suffix (`unbiased;` would otherwise match).
            let before_byte = rest.as_bytes().get(idx.wrapping_sub(1)).copied();
            let prev_ok = match before_byte {
                None => true,
                Some(b) => !(b.is_ascii_alphanumeric() || b == b'_'),
            };
            if prev_ok {
                return true;
            }
        }
        rest = &rest[idx + "biased".len()..];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_select_without_biased() {
        let src = r#"async fn f() { tokio::select! { _ = a => {}, _ = b => {} } }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_select_with_biased() {
        let src = r#"async fn f() { tokio::select! { biased; _ = a => {}, _ = b => {} } }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_flag_unprefixed_select_without_tokio_import() {
        let src = r#"async fn f() { select! { _ = a => {}, _ = b => {} } }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_unprefixed_select_with_tokio_import() {
        let src = r#"
use tokio::select;
async fn f() { select! { _ = a => {}, _ = b => {} } }
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_other_macro() {
        let src = r#"fn f() { println!("hi"); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_select_with_biased_no_qualifier() {
        let src = r#"async fn f() { select! { biased; _ = a => {} } }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_flag_futures_select() {
        let src = r#"async fn f() { futures::select! { a => {}, b => {} } }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_flag_crossbeam_select() {
        let src = r#"fn f() { crossbeam::select! { recv(r) -> msg => {} } }"#;
        assert!(run_on(src).is_empty());
    }
}
