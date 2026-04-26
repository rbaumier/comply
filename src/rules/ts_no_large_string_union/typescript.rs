//! Flags `union_type` nodes with more than 50 `literal_type` children.

use crate::diagnostic::{Diagnostic, Severity};

const THRESHOLD: usize = 50;

crate::ast_check! { on ["union_type"] => |node, source, ctx, diagnostics|
    let _ = source;
    // Avoid double-flagging nested unions — only emit on the outermost.
    if let Some(parent) = node.parent()
        && parent.kind() == "union_type"
    {
        return;
    }

    fn count_literals(n: tree_sitter::Node) -> usize {
        let mut total = 0;
        let mut cursor = n.walk();
        for c in n.named_children(&mut cursor) {
            match c.kind() {
                "literal_type" => total += 1,
                "union_type" => total += count_literals(c),
                _ => {}
            }
        }
        total
    }

    let count = count_literals(node);

    if count > THRESHOLD {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("String-literal union has {count} members (>{THRESHOLD}); consider a branded string or enum."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_union_over_threshold() {
        let members: Vec<String> = (0..60).map(|i| format!("'m{i}'")).collect();
        let src = format!("type T = {};", members.join(" | "));
        let diags = run(&src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_small_union() {
        let src = "type T = 'a' | 'b' | 'c';";
        assert!(run(src).is_empty());
    }
}
