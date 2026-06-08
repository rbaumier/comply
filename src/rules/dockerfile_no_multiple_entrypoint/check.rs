use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["entrypoint_instruction"] => |node, source, ctx, diagnostics|
    let _ = source;
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        match p.kind() {
            "from_instruction" => return,
            "entrypoint_instruction" => {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: super::META.id.into(),
                    message: "Multiple ENTRYPOINT instructions in the same stage; only the last is honored.".into(),
                    severity: Severity::Warning,
                    span: Some((node.byte_range().start, node.byte_range().len())),
                });
                return;
            }
            _ => {}
        }
        prev = p.prev_sibling();
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
    fn flags_duplicate_entrypoint() {
        assert_eq!(
            run("FROM node:20\nENTRYPOINT [\"node\"]\nENTRYPOINT [\"npm\"]\n").len(),
            1
        );
    }

    #[test]
    fn allows_single_entrypoint() {
        assert!(run("FROM node:20\nENTRYPOINT [\"node\", \"server.js\"]\n").is_empty());
    }
}
