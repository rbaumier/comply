use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["rule_set"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let Some(block) = node.children(&mut c).find(|n| n.kind() == "block") else { return; };
    let mut bc = block.walk();
    let has_decl = block.children(&mut bc).any(|n| n.kind() == "declaration");
    if has_decl { return; }
    let _ = source;
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &block,
        super::META.id,
        "Empty declaration block; remove or populate it.".into(),
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
    fn flags_empty_block() {
        assert_eq!(run(".a { }").len(), 1);
    }

    #[test]
    fn allows_block_with_declarations() {
        assert!(run(".a { color: red; }").is_empty());
    }
}
