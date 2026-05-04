//! no-redundant-jump OxcCheck backend — flag bare `return;` or
//! `continue;` at the tail of the enclosing callable / loop.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

#[derive(Copy, Clone, PartialEq, Eq)]
enum JumpKind {
    Return,
    Continue,
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ReturnStatement, AstType::ContinueStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (kind, offset) = match node.kind() {
            AstKind::ReturnStatement(ret) => {
                // Only bare `return;` — skip `return expr;`
                if ret.argument.is_some() {
                    return;
                }
                (JumpKind::Return, ret.span().start)
            }
            AstKind::ContinueStatement(cont) => {
                // Skip labeled `continue label;`
                if cont.label.is_some() {
                    return;
                }
                (JumpKind::Continue, cont.span().start)
            }
            _ => return,
        };

        if !is_redundant(node.id(), kind, semantic) {
            return;
        }

        let keyword = match kind {
            JumpKind::Return => "return;",
            JumpKind::Continue => "continue;",
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Redundant `{keyword}` \u{2014} execution already falls through here."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_redundant(
    start_id: oxc_semantic::NodeId,
    kind: JumpKind,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current = start_id;
    loop {
        let parent_id = nodes.parent_id(current);
        if parent_id == current {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            // Function boundaries
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return kind == JumpKind::Return;
            }
            // Loop boundaries
            AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => {
                return kind == JumpKind::Continue;
            }
            // Block: the jump must be the last statement
            AstKind::FunctionBody(body) => {
                if !is_last_statement_in_block(&body.statements, current, nodes) {
                    return false;
                }
                current = parent_id;
            }
            AstKind::BlockStatement(block) => {
                if !is_last_statement_in_block(&block.body, current, nodes) {
                    return false;
                }
                current = parent_id;
            }
            // Tail wrappers: if/else, switch_case
            AstKind::IfStatement(_)
            | AstKind::SwitchCase(_)
            | AstKind::ExpressionStatement(_) => {
                current = parent_id;
            }
            _ => return false,
        }
    }
}

/// Check if the node identified by `child_id` corresponds to the last
/// statement in the given statement list.
fn is_last_statement_in_block(
    stmts: &oxc_allocator::Vec<Statement>,
    child_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    let Some(last) = stmts.last() else {
        return false;
    };
    // The child_id might be the statement itself or something inside it.
    // Walk from child_id up until we find a node whose parent is the block,
    // then check if that node's span matches the last statement's span.
    let last_span = last.span();
    let mut cur = child_id;
    loop {
        let n = nodes.get_node(cur);
        if n.kind().span() == last_span {
            return true;
        }
        let pid = nodes.parent_id(cur);
        if pid == cur {
            return false;
        }
        cur = pid;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
        let src = "outer: for (const x of xs) { for (const y of ys) { continue outer; } }";
        assert!(run_on(src).is_empty());
    }
}
