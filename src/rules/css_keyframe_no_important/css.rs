use crate::diagnostic::{Diagnostic, Severity};

fn inside_keyframes(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "keyframes_statement" {
            return true;
        }
        cur = n.parent();
    }
    false
}

crate::ast_check! { on ["important"] => |node, source, ctx, diagnostics|
    let _ = source;
    if !inside_keyframes(node) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`!important` is ignored inside `@keyframes`; remove it.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_css(s, &Check)
    }

    #[test]
    fn flags_important_in_keyframes() {
        let css = "@keyframes fade { from { opacity: 0 !important; } }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_no_important_in_keyframes() {
        let css = "@keyframes fade { from { opacity: 0; } }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn allows_important_outside_keyframes() {
        let css = ".a { color: red !important; }";
        assert!(run(css).is_empty());
    }
}
