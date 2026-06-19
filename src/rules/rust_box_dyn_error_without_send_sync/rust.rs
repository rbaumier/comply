//! rust-box-dyn-error-without-send-sync backend.
//!
//! Walks `generic_type` nodes whose constructor is `Box` and whose
//! sole type argument is a `dyn`-typed trait object referencing the
//! `Error` trait. We then check whether the trait object's bounds
//! include both `Send` and `Sync`. If either is missing, we flag it —
//! unless the bounds carry an explicit non-`'static` lifetime (e.g.
//! `Box<dyn Error + 'a>`), which marks a borrow-scoped error for which
//! the `+ 'static` remediation is inapplicable.
//!
//! The check is text-based on the trait-object substring because
//! tree-sitter-rust models `dyn Trait + Send + Sync` as a single
//! `dynamic_type` whose internal layout is grammar-version
//! dependent — substring matching is robust enough. To avoid false
//! positives we require the `Error` token to be the *primary* trait of
//! the outer `dyn` (`dyn Error ...` or `dyn ...::Error ...`), not merely
//! to appear somewhere inside an inner type's generics (e.g.
//! `dyn Future<Output = Result<_, Self::Error>>`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{is_in_test_context, is_under_tests_dir};

const KINDS: &[&str] = &["generic_type"];

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
        let source_bytes = ctx.source.as_bytes();
        let Some(type_node) = node.child_by_field_name("type") else {
            return;
        };
        let type_text = type_node.utf8_text(source_bytes).unwrap_or("");
        if type_text != "Box" {
            return;
        }
        let Some(args) = node.child_by_field_name("type_arguments") else {
            return;
        };
        let Ok(args_text) = args.utf8_text(source_bytes) else {
            return;
        };
        // We need a `dyn Error` type argument where `Error` is the primary
        // trait of the outer `dyn` — not `Error` buried inside an inner
        // type's generics (`dyn Future<Output = Result<_, Self::Error>>`).
        if !dyn_primary_trait_is_error(args_text) {
            return;
        }
        let has_send = args_text.contains("Send");
        let has_sync = args_text.contains("Sync");
        if has_send && has_sync {
            return;
        }
        // A non-`'static` lifetime bound (`Box<dyn Error + 'a>`) marks a
        // borrow-scoped error: it borrows from an input, so it cannot be
        // `'static`. The `+ 'static` remediation is impossible here.
        if has_non_static_lifetime(args_text) {
            return;
        }
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        let missing = match (has_send, has_sync) {
            (false, false) => "Send + Sync",
            (false, true) => "Send",
            (true, false) => "Sync",
            (true, true) => unreachable!(),
        };
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &node,
            "rust-box-dyn-error-without-send-sync",
            format!(
                "`Box<dyn Error>` is missing `{missing}` — the error can't \
                 cross thread boundaries. Add `+ Send + Sync + 'static` or \
                 use `anyhow::Error`."
            ),
            Severity::Warning,
        ));
    }
}

/// True when the outer `dyn` trait object's primary trait is the `Error`
/// trait (`dyn Error ...` or a path `dyn ...::Error ...`), as opposed to
/// `Error` merely appearing inside an inner type's generics
/// (`dyn Future<Output = Result<_, Self::Error>>`).
///
/// We locate the first standalone `dyn` keyword (boundary-checked so
/// `mydyn`/`dynamic` don't match), then read the primary trait path: the
/// text after `dyn`, trimmed, up to the first `<`, `+`, `>`, or whitespace.
fn dyn_primary_trait_is_error(args_text: &str) -> bool {
    let bytes = args_text.as_bytes();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        if &bytes[i..i + 3] == b"dyn" {
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let after_ok = i + 3 == bytes.len() || !is_ident_char(bytes[i + 3]);
            if before_ok && after_ok {
                let rest = args_text[i + 3..].trim_start();
                let path_end = rest
                    .find(|c: char| c == '<' || c == '+' || c == '>' || c.is_whitespace())
                    .unwrap_or(rest.len());
                let path = &rest[..path_end];
                return path == "Error" || path.ends_with("::Error");
            }
        }
        i += 1;
    }
    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Returns true if `args_text` (a type-position substring) carries a
/// lifetime bound whose name is not `static`. A `'` in a type is always a
/// lifetime (type position has no char literals), so we scan for a `'`
/// followed by an identifier and compare the name against `static`.
fn has_non_static_lifetime(args_text: &str) -> bool {
    let bytes = args_text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            let name_start = i + 1;
            let mut name_end = name_start;
            while name_end < bytes.len() && is_ident_char(bytes[name_end]) {
                name_end += 1;
            }
            if name_end > name_start && &bytes[name_start..name_end] != b"static" {
                return true;
            }
            i = name_end;
        } else {
            i += 1;
        }
    }
    false
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_bare_box_dyn_error() {
        let source = "fn f() -> Result<(), Box<dyn std::error::Error>> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_box_dyn_error_send_only() {
        let source = "fn f() -> Result<(), Box<dyn std::error::Error + Send>> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_box_dyn_error_send_sync() {
        let source = "fn f() -> Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_box_dyn_error_with_non_static_lifetime() {
        // `Box<dyn Error + 'a>` is a borrow-scoped error: it borrows from the
        // `&'a str` input, so it is intentionally not `'static`. The `+ 'static`
        // remediation is impossible here. (helix command_line.rs:805)
        let source =
            "fn parse(line: &'a str) -> Result<Self, Box<dyn Error + 'a>> { todo!() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_box_dyn_error_static_only() {
        // Only `'static` (no Send + Sync) is still a true positive: the error
        // can be made thread-safe, so the remediation applies.
        let source = "fn f() -> Result<(), Box<dyn std::error::Error + 'static>> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_box_dyn_other_trait() {
        let source = "fn f() -> Box<dyn Iterator<Item = u8>> { todo!() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_concrete_box() {
        let source = "fn f() -> Box<MyError> { todo!() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_dyn_my_error_subclass() {
        // `dyn MyError` should NOT match — only the standalone `Error` token does.
        let source = "fn f() -> Box<dyn MyError> { todo!() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_box_dyn_future_with_self_error_in_generics() {
        // axum from_fn.rs: the Box holds `dyn Future<...>`, not `dyn Error`.
        // `Error` appears only as `Self::Error` inside the Future's generics —
        // it is not the primary trait of the `dyn`, so it must not be flagged.
        // (Failed under the old `contains_word(args_text, "Error")` check.)
        let source = r#"
            impl Service<Request> for Next {
                type Response = Response;
                type Error = Infallible;
                type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_bare_box_dyn_error_no_path() {
        // Bare `dyn Error` (primary trait is the unqualified `Error` token),
        // missing both Send and Sync → still flagged.
        let source = "fn f() -> Result<(), Box<dyn Error>> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_bare_box_dyn_error_send_only_no_path() {
        // Bare `dyn Error + Send` (missing Sync) → still flagged.
        let source = "fn f() -> Result<(), Box<dyn Error + Send>> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_box_dyn_error_in_tokio_test() {
        let source = r#"
            #[tokio::test]
            async fn test() -> Result<(), Box<dyn std::error::Error>> {
                Ok(())
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_box_dyn_error_in_cfg_test_mod() {
        let source = r#"
            #[cfg(test)]
            mod tests {
                fn test_fn() -> Result<(), Box<dyn std::error::Error>> {
                    Ok(())
                }
            }
        "#;
        assert!(run_on(source).is_empty());
    }
}
