//! no-new-regex-with-variable backend for Rust.
//!
//! Flags `Regex::new(variable)` / `RegexBuilder::new(variable)` where the
//! argument is not a string literal. User-controlled patterns open the
//! door to ReDoS via exponential backtracking. The fix: use a literal
//! `Regex::new(r"...")`, or a vetted safe-regex library.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        // Match `Regex::new`, `RegexBuilder::new`, `regex::Regex::new`, etc.
        let Ok(fn_text) = function.utf8_text(source_bytes) else {
            return;
        };
        if !fn_text.ends_with("Regex::new") && !fn_text.ends_with("RegexBuilder::new") {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let Some(first_arg) = args.named_child(0) else {
            return;
        };
        if is_string_literal(first_arg) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-new-regex-with-variable".into(),
            message: "`Regex::new(variable)` — ReDoS risk. A crafted \
                      pattern can freeze the thread via exponential \
                      backtracking. Use a literal `r\"...\"` pattern."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn is_string_literal(node: tree_sitter::Node) -> bool {
    // Accept `"..."`, `r"..."`, and `&"..."` / `&r"..."`.
    match node.kind() {
        "string_literal" | "raw_string_literal" => true,
        "reference_expression" => {
            node.named_child(0)
                .is_some_and(|inner| matches!(inner.kind(), "string_literal" | "raw_string_literal"))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn flags_regex_with_variable() {
        assert_eq!(run_on("fn f() { let r = Regex::new(&input); }").len(), 1);
    }

    #[test]
    fn allows_regex_with_literal() {
        assert!(run_on("fn f() { let r = Regex::new(r\"^foo\"); }").is_empty());
    }

    #[test]
    fn allows_regex_with_plain_string() {
        assert!(run_on("fn f() { let r = Regex::new(\"^foo\"); }").is_empty());
    }
}
