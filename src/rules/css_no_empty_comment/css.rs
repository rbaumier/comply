use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or_default();
    let inner = text.strip_prefix("/*").and_then(|s| s.strip_suffix("*/")).unwrap_or(text);
    if !inner.trim().is_empty() { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Empty comment; remove or add content.".into(),
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
    fn flags_empty_with_space() {
        assert_eq!(run("/* */").len(), 1);
    }

    #[test]
    fn flags_empty_no_space() {
        assert_eq!(run("/**/").len(), 1);
    }

    #[test]
    fn allows_comment_with_text() {
        assert!(run("/* some text */").is_empty());
    }
}
