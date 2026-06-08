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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.css")
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
