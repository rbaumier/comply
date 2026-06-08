use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["onbuild_instruction"] => |node, source, ctx, diagnostics|
    let _ = source;
    let mut cursor = node.walk();
    for c in node.children(&mut cursor) {
        match c.kind() {
            "from_instruction" | "onbuild_instruction" | "maintainer_instruction" => {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: super::META.id.into(),
                    message: "ONBUILD cannot wrap FROM, ONBUILD, or MAINTAINER.".into(),
                    severity: Severity::Warning,
                    span: Some((node.byte_range().start, node.byte_range().len())),
                });
                return;
            }
            _ => {}
        }
    }
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
    fn flags_onbuild_from() {
        assert_eq!(run("FROM node:20\nONBUILD FROM node:20\n").len(), 1);
    }

    #[test]
    fn flags_onbuild_maintainer() {
        assert_eq!(
            run("FROM node:20\nONBUILD MAINTAINER user@example.com\n").len(),
            1
        );
    }

    #[test]
    fn allows_onbuild_run() {
        assert!(run("FROM node:20\nONBUILD RUN echo hello\n").is_empty());
    }
}
