use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["maintainer_instruction"] => |node, source, ctx, diagnostics|
    let _ = source;
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "MAINTAINER is deprecated; use `LABEL maintainer=...` instead.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "Dockerfile")
    }

    #[test]
    fn flags_maintainer() {
        assert_eq!(run("FROM node:20\nMAINTAINER user@example.com\n").len(), 1);
    }

    #[test]
    fn allows_label_maintainer() {
        assert!(run("FROM node:20\nLABEL maintainer=\"user@example.com\"\n").is_empty());
    }
}
