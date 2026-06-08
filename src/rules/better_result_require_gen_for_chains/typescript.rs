use crate::diagnostic::{Diagnostic, Severity};

fn is_andthen_call(node: &tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    prop.utf8_text(source).unwrap_or("") == "andThen"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_andthen_call(&node, source) {
        return;
    }
    // Check that the object of the member_expression is another .andThen call
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let Some(obj) = callee.child_by_field_name("object") else { return; };
    if !is_andthen_call(&obj, source) {
        return;
    }
    // We only want to report once per chain — report at the outermost call.
    // If this node's parent is itself an andThen member_expression, skip.
    if let Some(parent) = node.parent()
        && parent.kind() == "member_expression"
        && let Some(grand) = parent.parent()
        && is_andthen_call(&grand, source)
    {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Chaining 2+ .andThen() calls — rewrite using Result.gen + yield*.".into(),
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }
    #[test]
    fn flags_two_andthen_chain() {
        let src = "const r = getUser().andThen(u => getOrders(u)).andThen(o => getItems(o));";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_single_andthen() {
        let src = "const r = getUser().andThen(u => getOrders(u));";
        assert!(run(src).is_empty());
    }
}
