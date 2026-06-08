//! no-redundant-jump TypeScript / JavaScript / TSX backend.
//!
//! See the crate-level docblock in `mod.rs` for the full algorithm. A
//! `return;` (without argument) is redundant iff walking up from it
//! reaches a callable boundary through tail positions only. Same logic
//! for `continue;` with a loop boundary.

use crate::diagnostic::{Diagnostic, Severity};

#[derive(Copy, Clone, PartialEq, Eq)]
enum JumpKind {
    Return,
    Continue,
}

const CALLABLE_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function",
    "generator_function_declaration",
];

const LOOP_KINDS: &[&str] = &[
    "for_statement",
    "for_in_statement",
    "for_of_statement",
    "while_statement",
    "do_statement",
];

const TAIL_WRAPPERS: &[&str] = &[
    "if_statement",
    "else_clause",
    "switch_case",
    "switch_default",
    "expression_statement",
];

crate::ast_check! { on ["return_statement", "continue_statement"] => |node, _source, ctx, diagnostics|
    let kind = match node.kind() {
        "return_statement" => {
            // `return;` has no argument child. `return x;` has one.
            if node.named_child_count() != 0 {
                return;
            }
            JumpKind::Return
        }
        "continue_statement" => {
            // Labeled `continue label;` has a label child — skip.
            if node.named_child_count() != 0 {
                return;
            }
            JumpKind::Continue
        }
        _ => return,
    };

    if !is_redundant(node, kind) {
        return;
    }

    let pos = node.start_position();
    let keyword = match kind {
        JumpKind::Return => "return;",
        JumpKind::Continue => "continue;",
    };
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-redundant-jump".into(),
        message: format!(
            "Redundant `{keyword}` \u{2014} execution already falls through here."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

fn is_redundant(start: tree_sitter::Node, kind: JumpKind) -> bool {
    let mut node = start;
    loop {
        let Some(parent) = node.parent() else {
            return false;
        };
        let pk = parent.kind();

        if CALLABLE_KINDS.contains(&pk) {
            return kind == JumpKind::Return;
        }
        if LOOP_KINDS.contains(&pk) {
            return kind == JumpKind::Continue;
        }
        if pk == "statement_block" {
            if !is_last_named_child(parent, node) {
                return false;
            }
            node = parent;
            continue;
        }
        if TAIL_WRAPPERS.contains(&pk) {
            // switch_case: require the jump to be the last statement of
            // the case block (fall-through semantics mean a jump before
            // more code is NOT redundant).
            if (pk == "switch_case" || pk == "switch_default") && !is_last_named_child(parent, node)
            {
                return false;
            }
            node = parent;
            continue;
        }
        return false;
    }
}

fn is_last_named_child(parent: tree_sitter::Node, child: tree_sitter::Node) -> bool {
    let count = parent.named_child_count();
    if count == 0 {
        return false;
    }
    parent.named_child(count - 1).map(|n| n.id()) == Some(child.id())
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
    fn flags_redundant_return_at_fn_end() {
        let src = "function foo() {\n  doStuff();\n  return;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("return;"));
    }

    #[test]
    fn flags_redundant_continue_at_loop_end() {
        let src = "for (const x of xs) {\n  doStuff();\n  continue;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("continue;"));
    }

    #[test]
    fn allows_return_before_more_code() {
        let src = "function foo(x) {\n  if (x) {\n    return;\n  }\n  bar();\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_nested_if_guard_with_more_fn_body() {
        let src = r#"
function check(isArrow, node) {
    if (isArrow) {
        const parent = node.parent;
        if (!parent) return;
        if (parent.kind !== "vd") return;
        const name = parent.name;
        if (!name.startsWith("A")) {
            return;
        }
    }

    const stack = [node];
    while (stack.length) {
        doStuff();
        stack.pop();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_return_with_value() {
        let src = "function f() { doStuff(); return 42; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_arrow_function_return_value() {
        let src = "const f = () => { doStuff(); return 42; };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_return_at_arrow_fn_end() {
        let src = "const f = () => { doStuff(); return; };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_labeled_continue() {
        // Labeled continues change behavior; don't flag them.
        let src = "outer: for (const x of xs) { for (const y of ys) { continue outer; } }";
        assert!(run_on(src).is_empty());
    }
}
