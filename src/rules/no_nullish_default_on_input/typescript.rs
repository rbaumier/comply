//! no-nullish-default-on-input backend — reject `x ?? default` / `x || fallback`
//! on function parameters.
//!
//! Why: using `x ?? 0` or `x || []` on an external input silently paves
//! over invalid values. If the caller passes garbage, the function happily
//! runs with `0` or `[]` and the bug surfaces far from where it was
//! introduced. The correct response is to validate at the boundary and
//! reject the call with a Result error.
//!
//! Detection: walk `binary_expression` nodes whose operator is `??` or
//! `||` and whose left operand is an identifier matching a function
//! parameter name in scope. Cheap heuristic: collect parameter names from
//! every enclosing function node, then flag any `param ?? x` / `param || x`
//! pattern inside that function body.

use std::collections::HashSet;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

/// Visit-time state: collected parameter names plus pending candidate
/// `binary_expression` nodes. Parameters are gathered as we encounter them
/// during the single AST walk; `binary_expression` candidates are recorded
/// the same way and resolved in `finish`, since a binary expression may
/// appear before all of its enclosing function's parameters have been seen
/// in pre-order traversal (e.g. parameters come first, but defaults can
/// reference identifiers introduced later in unrelated scopes — using
/// `finish` keeps the logic tolerant of traversal order).
#[derive(Default)]
struct State {
    params: HashSet<String>,
    candidates: Vec<Candidate>,
}

struct Candidate {
    op_text: String,
    name: String,
    line: usize,
    column: usize,
}

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "required_parameter",
            "optional_parameter",
            "binary_expression",
        ])
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(State::default()))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };
        let kind = node.kind();
        if kind == "required_parameter" || kind == "optional_parameter" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier"
                    && let Ok(name) = child.utf8_text(source_bytes)
                {
                    state.params.insert(name.to_string());
                }
            }
            return;
        }
        // binary_expression
        let Some(op) = node.child_by_field_name("operator") else {
            return;
        };
        let Ok(op_text) = op.utf8_text(source_bytes) else {
            return;
        };
        if op_text != "??" && op_text != "||" {
            return;
        }
        let Some(left) = node.child_by_field_name("left") else {
            return;
        };
        if left.kind() != "identifier" {
            return;
        }
        let Ok(name) = left.utf8_text(source_bytes) else {
            return;
        };
        let pos = node.start_position();
        state.candidates.push(Candidate {
            op_text: op_text.to_string(),
            name: name.to_string(),
            line: pos.row + 1,
            column: pos.column + 1,
        });
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(state) = state.and_then(|s| s.downcast::<State>().ok()) else {
            return;
        };
        if state.params.is_empty() {
            return;
        }
        for c in &state.candidates {
            if !state.params.contains(&c.name) {
                continue;
            }
            let op_text = &c.op_text;
            let name = &c.name;
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: c.line,
                column: c.column,
                rule_id: "no-nullish-default-on-input".into(),
                message: format!(
                    "Using '{op_text}' to default a function parameter '{name}' \
                     silently paves over invalid input. Validate at the \
                     boundary and return a Result error instead."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_nullish_coalesce_on_param() {
        assert_eq!(
            run_on("function f(x: number) { const v = x ?? 0; return v; }").len(),
            1
        );
    }

    #[test]
    fn flags_logical_or_on_param() {
        assert_eq!(
            run_on("function f(items: number[]) { const v = items || []; return v; }").len(),
            1
        );
    }

    #[test]
    fn allows_default_on_local_variable() {
        // `local` is not a parameter name in this file.
        assert!(run_on("function f() { const local: number | null = null; const v = local ?? 0; return v; }").is_empty());
    }

    #[test]
    fn allows_nullish_on_property_access() {
        assert!(run_on("function f(opts: { x?: number }) { return opts.x ?? 0; }").is_empty());
    }
}
