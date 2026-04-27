//! Detect `use(...)` calls whose enclosing context (walking up to the
//! function body) crosses an `if_statement`, `ternary_expression`, or any
//! loop construct.

use crate::diagnostic::{Diagnostic, Severity};

const CONDITIONAL_KINDS: &[&str] = &[
    "if_statement",
    "ternary_expression",
    "for_statement",
    "for_in_statement",
    "for_of_statement",
    "while_statement",
    "do_statement",
    "switch_statement",
];

fn is_use_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" { return false; }
    let Some(callee) = node.child_by_field_name("function") else { return false };
    if callee.kind() != "identifier" { return false; }
    callee.utf8_text(source).map(|t| t == "use").unwrap_or(false)
}

fn is_inside_conditional(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(parent) = cur {
        // Stop walking once we hit a function-like boundary — hook rules
        // only care about the enclosing function body.
        if matches!(
            parent.kind(),
            "function_declaration"
                | "function_expression"
                | "arrow_function"
                | "method_definition"
                | "program"
        ) {
            return false;
        }
        if CONDITIONAL_KINDS.contains(&parent.kind()) {
            return true;
        }
        // `&&` / `||` short-circuiting is conditional too.
        if parent.kind() == "binary_expression" {
            // We can't access fields easily; check if the operator text is && or ||.
            let mut c = parent.walk();
            for child in parent.children(&mut c) {
                let k = child.kind();
                if k == "&&" || k == "||" || k == "??" {
                    return true;
                }
            }
        }
        cur = parent.parent();
    }
    false
}

crate::ast_check! {
    on ["call_expression"]
    => |node, source, ctx, diagnostics|
    if !is_use_call(node, source) { return; }
    if !is_inside_conditional(node) { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`use(...)` is a hook — it cannot be called conditionally or inside a loop. \
                  Lift the call to the top of the component."
            .into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_use_in_if() {
        let src = "function C({p, x}: any) { if (x) { const v = use(p); return v; } return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_use_in_ternary() {
        let src = "function C({p, x}: any) { const v = x ? use(p) : null; return v; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_use_in_loop() {
        let src = "function C({ps}: any) { for (const p of ps) { use(p); } return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_at_top_level() {
        let src = "function C({p}: any) { const v = use(p); return v; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_use_inside_jsx_attr() {
        // JSX attribute is not a conditional context.
        let src = "function C({p}: any) { const v = use(p); return <div>{v}</div>; }";
        assert!(run(src).is_empty());
    }
}
