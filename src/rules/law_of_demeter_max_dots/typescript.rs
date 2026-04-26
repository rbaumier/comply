//! Flag `a.b.c.d` (and `a.b().c().d()` variants) — member chains with 3+
//! property accesses reach too deep into another object's internals.

use crate::diagnostic::{Diagnostic, Severity};

const MAX_DOTS: usize = 2;

fn depth<'a>(node: tree_sitter::Node<'a>) -> usize {
    let mut cur = node;
    let mut count = 0usize;
    loop {
        match cur.kind() {
            "member_expression" => {
                count += 1;
                let Some(obj) = cur.child_by_field_name("object") else { break };
                cur = obj;
            }
            "call_expression" => {
                let Some(func) = cur.child_by_field_name("function") else { break };
                cur = func;
            }
            _ => break,
        }
    }
    count
}

crate::ast_check! { on ["member_expression"] => |node, source, ctx, diagnostics|
    let _ = source;
    // Only evaluate the outermost member_expression in a chain to avoid
    // reporting every intermediate node.
    if let Some(parent) = node.parent() {
        match parent.kind() {
            "member_expression" => return,
            "call_expression" => {
                if parent.child_by_field_name("function").map(|f| f.id()) == Some(node.id()) {
                    // node is the callee; let the parent chain reach the outer check.
                    if let Some(gp) = parent.parent()
                        && matches!(gp.kind(), "member_expression" | "call_expression") {
                            return;
                        }
                }
            }
            _ => {}
        }
    }
    if depth(node) < MAX_DOTS { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Chain reaches {MAX_DOTS} or more levels deep. Ask the collaborator for a higher-level method."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(s, &Check) }

    #[test]
    fn flags_three_level_chain() {
        assert_eq!(run("a.b.c.d").len(), 1);
    }

    #[test]
    fn flags_mixed_call_chain() {
        assert_eq!(run("a.b().c().d").len(), 1);
    }

    #[test]
    fn flags_two_level_chain() {
        // Documented threshold (2 levels) — `a.b().c()` is the canonical
        // example and must be flagged.
        assert_eq!(run("a.b().c()").len(), 1);
    }

    #[test]
    fn allows_one_level_chain() {
        assert!(run("a.b").is_empty());
    }
}
