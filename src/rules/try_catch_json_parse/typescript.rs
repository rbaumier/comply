//! try-catch-json-parse backend — flag `JSON.parse(...)` not wrapped in a
//! `try` statement.
//!
//! Detection: for each `call_expression` whose callee matches `JSON.parse`,
//! walk up the ancestors. If no `try_statement` body encloses the call
//! (within the enclosing function), emit a diagnostic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function",
    "generator_function_declaration",
];

const KINDS: &[&str] = &["call_expression"];

fn is_inside_try_body(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "try_statement"
            && let Some(body) = n.child_by_field_name("body")
        {
            let ns = node.start_byte();
            let ne = node.end_byte();
            if ns >= body.start_byte() && ne <= body.end_byte() {
                return true;
            }
        }
        // Don't stop at function boundaries — a top-level try around the
        // module still protects calls inside nested arrow callbacks? No —
        // if the call is inside a nested function, the outer try can't
        // catch it unless the function is awaited/called within the try.
        // Stop at function boundaries for safety.
        if FUNCTION_KINDS.contains(&n.kind()) {
            return false;
        }
        current = n.parent();
    }
    false
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["JSON.parse"])
    }

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
        let Some(callee) = node.child_by_field_name("function") else { return };
        if callee.kind() != "member_expression" {
            return;
        }
        let Ok(text) = callee.utf8_text(source) else { return };
        if text != "JSON.parse" {
            return;
        }
        if is_inside_try_body(node) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "try-catch-json-parse".into(),
            message: "`JSON.parse` can throw on invalid input — wrap it in \
                      try/catch or use a safe parser (Zod, schema validator)."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_bare_json_parse() {
        let d = run_on("const data = JSON.parse(input);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "try-catch-json-parse");
    }

    #[test]
    fn flags_inside_function() {
        let d = run_on("function f(s) { return JSON.parse(s); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_inside_try() {
        assert!(
            run_on("try { const data = JSON.parse(input); } catch (e) { log(e); }").is_empty()
        );
    }

    #[test]
    fn flags_when_try_only_around_outer_fn() {
        // The try is in the outer fn; the parse is inside a nested arrow.
        // That try can't catch it — flag the parse.
        let d = run_on(
            "function outer() { try { arr.map((s) => JSON.parse(s)); } catch (e) {} }",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_non_json_parse() {
        assert!(run_on("const data = myParse(input);").is_empty());
    }
}
