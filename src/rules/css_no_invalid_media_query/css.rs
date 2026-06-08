use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["media_statement"] => |node, source, ctx, diagnostics|
    let _ = source;
    if !node.has_error() { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Malformed `@media` query.".into(),
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
    fn flags_malformed_media() {
        let css = "@media screen and { .a { color: red; } }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_well_formed_media() {
        let css = "@media screen and (min-width: 768px) { .a { color: red; } }";
        assert!(run(css).is_empty());
    }
}
