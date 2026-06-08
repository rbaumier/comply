//! prefer-dom-node-remove backend — flag `.removeChild()` calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["removeChild"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "removeChild" {
        return;
    }

    let pos = prop.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-dom-node-remove".into(),
        message: "Prefer `childNode.remove()` over `parentNode.removeChild(childNode)`.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_remove_child() {
        let d = run_on("parent.removeChild(child);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("remove"));
    }

    #[test]
    fn flags_parent_node_remove_child() {
        let d = run_on("el.parentNode.removeChild(el);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_remove() {
        assert!(run_on("child.remove();").is_empty());
    }
}
