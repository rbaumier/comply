//! rust-box-dyn-error-without-send-sync backend.
//!
//! Walks `generic_type` nodes whose constructor is `Box` and whose
//! sole type argument is a `dyn`-typed trait object referencing the
//! `Error` trait. We then check whether the trait object's bounds
//! include both `Send` and `Sync`. If either is missing, we flag it.
//!
//! The check is text-based on the trait-object substring because
//! tree-sitter-rust models `dyn Trait + Send + Sync` as a single
//! `dynamic_type` whose internal layout is grammar-version
//! dependent — substring matching is robust enough and avoids
//! false positives by anchoring on the literal `Error` token.

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
        // We need a `dyn ... Error` type argument. We match `Error` as a
        // standalone token (not `MyError`) by checking the boundary char.
        if !args_text.contains("dyn") || !contains_word(args_text, "Error") {
            return;
        }
        let has_send = args_text.contains("Send");
        let has_sync = args_text.contains("Sync");
        if has_send && has_sync {
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

/// Returns true if `needle` appears in `haystack` as a standalone token
/// (preceded and followed by a non-identifier character or string boundary).
fn contains_word(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut i = 0;
    while i + needle_bytes.len() <= bytes.len() {
        if &bytes[i..i + needle_bytes.len()] == needle_bytes {
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let after_idx = i + needle_bytes.len();
            let after_ok = after_idx == bytes.len() || !is_ident_char(bytes[after_idx]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
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
