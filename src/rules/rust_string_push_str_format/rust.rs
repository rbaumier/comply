//! rust-string-push-str-format backend.
//!
//! Detects `s.push_str(&format!(...))`. The argument to `push_str` must be a
//! `&format!(...)` reference expression — i.e. a `reference_expression`
//! whose value is a `macro_invocation` of `format`. We do not flag bare
//! `format!(...)` because that would be a type error (`push_str` takes
//! `&str`, not `String`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["call_expression"];

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
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "field_expression" {
            return;
        }
        let Some(field) = function.child_by_field_name("field") else {
            return;
        };
        let Ok(field_text) = field.utf8_text(source) else {
            return;
        };
        if field_text != "push_str" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        // Look at the first positional argument inside arguments().
        let mut cursor = args.walk();
        let first = args.named_children(&mut cursor).next();
        let Some(first_arg) = first else { return };
        if !is_ref_to_format_macro(first_arg, source) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-string-push-str-format",
            "`s.push_str(&format!(...))` allocates a throwaway `String`. \
             Use `write!(s, \"...\")` to format directly into the buffer."
                .into(),
            Severity::Warning,
        ));
    }
}

/// True if `node` is `&format!(...)` — a reference_expression wrapping a
/// `format` macro_invocation.
fn is_ref_to_format_macro(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "reference_expression" {
        return false;
    }
    let Some(value) = node.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "macro_invocation" {
        return false;
    }
    let Some(macro_name) = value.child_by_field_name("macro") else {
        return false;
    };
    macro_name.utf8_text(source).map(|t| t == "format").unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_push_str_with_format() {
        let src = r#"fn f() { let mut s = String::new(); s.push_str(&format!("{}", 1)); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_push_str_with_format_multi_arg() {
        let src = r#"fn f() { let mut s = String::new(); s.push_str(&format!("{}-{}", a, b)); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_push_str_with_literal() {
        let src = r#"fn f() { let mut s = String::new(); s.push_str("hello"); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_push_str_with_variable() {
        let src = r#"fn f(x: &str) { let mut s = String::new(); s.push_str(x); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_write_macro() {
        let src = r#"fn f() { let mut s = String::new(); write!(s, "{}", 1).unwrap(); }"#;
        assert!(run_on(src).is_empty());
    }
}
