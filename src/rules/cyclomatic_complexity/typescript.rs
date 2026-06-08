//! cyclomatic-complexity — flag functions with complexity > 10.
//!
//! Walks the AST and counts branching nodes (if, else if, for, while,
//! catch, case, ternary, &&, ||, ??) inside each function scope.

use crate::diagnostic::{Diagnostic, Severity};

/// Node kinds that represent function declarations/expressions.
const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function_declaration",
    "generator_function",
];

/// Node kinds that increment cyclomatic complexity.
///
/// Each switch statement counts as +1 (one decision point), regardless of how
/// many cases it has. Counting per-case would penalise exhaustive discriminated-union
/// switches for no real benefit.
const BRANCHING_KINDS: &[&str] = &[
    "if_statement",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
    "catch_clause",
    "switch_statement",
    "ternary_expression",
];

/// Binary operators that increment complexity.
const LOGICAL_OPS: &[&str] = &["&&", "||", "??"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if !FUNCTION_KINDS.contains(&node.kind()) {
        return;
    }

    // Extract function name for the message.
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>");

    let threshold = ctx.config.threshold("cyclomatic-complexity", "max", ctx.lang);
    // Count complexity: 1 base path + branching nodes.
    let complexity = 1 + count_complexity(node, source);

    if complexity > threshold {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "cyclomatic-complexity".into(),
            message: format!(
                "Function `{name}` has a cyclomatic complexity of {complexity} (max: {threshold}).",
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Recursively count branching nodes inside a function, stopping at
/// nested function boundaries.
fn count_complexity(node: tree_sitter::Node, source: &[u8]) -> usize {
    let mut count = 0;
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return 0;
    }
    loop {
        let child = cursor.node();

        // Don't recurse into nested functions.
        if FUNCTION_KINDS.contains(&child.kind()) {
            if !cursor.goto_next_sibling() {
                break;
            }
            continue;
        }

        if BRANCHING_KINDS.contains(&child.kind()) {
            count += 1;
        }

        // Count logical operators in binary expressions.
        if child.kind() == "binary_expression"
            && let Some(op) = child.child_by_field_name("operator")
        {
            let op_text = op.utf8_text(source).unwrap_or("");
            if LOGICAL_OPS.contains(&op_text) {
                count += 1;
            }
        }

        count += count_complexity(child, source);

        if !cursor.goto_next_sibling() {
            break;
        }
    }
    count
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_simple_function() {
        let src = r#"
function simple() {
    if (a) {
        return 1;
    }
    return 2;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_complex_function() {
        // 1 base + 16 if = 17 complexity (threshold 15)
        let src = r#"
function complex(x) {
    if (a) {}
    if (b) {}
    if (c) {}
    if (d) {}
    if (e) {}
    if (f) {}
    if (g) {}
    if (h) {}
    if (i) {}
    if (j) {}
    if (k) {}
    if (l) {}
    if (m) {}
    if (n) {}
    if (o) {}
    if (p) {}
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("17"));
    }

    #[test]
    fn no_fp_on_exhaustive_switch() {
        // Regression for #586: exhaustive switches over discriminated unions must
        // not trigger cyclomatic-complexity. The whole switch counts as +1,
        // regardless of the number of cases.
        let src = r#"
function fromElysiaError(error) {
    switch (error.code) {
        case 'NOT_FOUND': return 404;
        case 'UNAUTHORIZED': return 401;
        case 'FORBIDDEN': return 403;
        case 'BAD_REQUEST': return 400;
        case 'CONFLICT': return 409;
        case 'UNPROCESSABLE': return 422;
        case 'TOO_MANY_REQUESTS': return 429;
        case 'INTERNAL_SERVER_ERROR': return 500;
        case 'SERVICE_UNAVAILABLE': return 503;
        case 'VALIDATION': return 400;
        case 'PARSE': return 400;
        default: return 500;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn counts_logical_operators() {
        // 1 base + 1 if + 4 && = 6 — under threshold
        let src = r#"
function check(a, b, c, d, e) {
    if (a && b && c && d && e) {
        return true;
    }
    return false;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn counts_ternary() {
        // 1 base + 16 ternaries = 17 (threshold 15)
        let src = r#"
function ternaries(x) {
    const a = x ? 1 : 0;
    const b = x ? 1 : 0;
    const c = x ? 1 : 0;
    const d = x ? 1 : 0;
    const e = x ? 1 : 0;
    const f = x ? 1 : 0;
    const g = x ? 1 : 0;
    const h = x ? 1 : 0;
    const i = x ? 1 : 0;
    const j = x ? 1 : 0;
    const k = x ? 1 : 0;
    const l = x ? 1 : 0;
    const m = x ? 1 : 0;
    const n = x ? 1 : 0;
    const o = x ? 1 : 0;
    const p = x ? 1 : 0;
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }
}
