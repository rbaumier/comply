//! rust-assert-eq-with-bool-literal backend.
//!
//! Walks `macro_invocation` nodes whose macro is `assert_eq!`,
//! `assert_ne!`, `debug_assert_eq!`, or `debug_assert_ne!`, and
//! flags any invocation whose argument list contains a `true` or
//! `false` literal token. Tree-sitter doesn't expose macro args
//! as a structured AST, so we tokenise the raw text crudely:
//! split on `,` outside of nested parens/brackets and check each
//! piece's trimmed value.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["macro_invocation"];

const ASSERT_EQ_MACROS: &[&str] = &[
    "assert_eq",
    "assert_ne",
    "debug_assert_eq",
    "debug_assert_ne",
];

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
        let Some(macro_name) = node.child_by_field_name("macro") else {
            return;
        };
        let name = macro_name.utf8_text(source_bytes).unwrap_or("");
        let bare = name.rsplit("::").next().unwrap_or(name);
        if !ASSERT_EQ_MACROS.contains(&bare) {
            return;
        }
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        // Extract the token tree: between the first `(` / `[` / `{` and
        // the matching closer.
        let Some(open) = text.find(['(', '[', '{']) else {
            return;
        };
        let inner = &text[open + 1..text.len().saturating_sub(1)];
        let args = split_top_level(inner);
        // We only check the first two args (lhs/rhs); a third is the
        // optional format string for the failure message.
        let to_check: Vec<&str> = args.iter().take(2).map(|s| s.trim()).collect();
        let has_bool_literal = to_check.iter().any(|a| *a == "true" || *a == "false");
        if !has_bool_literal {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &node,
            "rust-assert-eq-with-bool-literal",
            format!(
                "`{bare}!` with a boolean literal — use `assert!(cond)` or \
                 `assert!(!cond)` instead. The eq-form produces a worse \
                 failure message."
            ),
            Severity::Warning,
        ));
    }
}

fn split_top_level(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut current = String::new();
    let mut in_string = false;
    let mut prev = '\0';
    for ch in text.chars() {
        if in_string {
            current.push(ch);
            if ch == '"' && prev != '\\' {
                in_string = false;
            }
            prev = ch;
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                current.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
        prev = ch;
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
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
    fn flags_assert_eq_with_true() {
        let source = "fn f(x: bool) { assert_eq!(x, true); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_assert_eq_with_false() {
        let source = "fn f(x: bool) { assert_eq!(x, false); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_assert_ne_with_bool_literal() {
        let source = "fn f(x: bool) { assert_ne!(x, true); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_assert_eq_with_non_bool_literals() {
        let source = "fn f(x: u8) { assert_eq!(x, 5); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_plain_assert() {
        let source = "fn f(x: bool) { assert!(x); }";
        assert!(run_on(source).is_empty());
    }
}
