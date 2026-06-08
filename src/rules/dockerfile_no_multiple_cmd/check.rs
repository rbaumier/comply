use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["cmd_instruction"] => |node, source, ctx, diagnostics|
    let _ = source;
    // Walk previous siblings up to the most recent FROM. If we find another
    // cmd_instruction in that range, flag the current one as a duplicate.
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        match p.kind() {
            "from_instruction" => return,
            "cmd_instruction" => {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: super::META.id.into(),
                    message: "Multiple CMD instructions in the same stage; only the last is honored.".into(),
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
    fn flags_duplicate_cmd() {
        assert_eq!(
            run("FROM node:20\nCMD [\"node\"]\nCMD [\"npm\", \"start\"]\n").len(),
            1
        );
    }

    #[test]
    fn allows_single_cmd() {
        assert!(run("FROM node:20\nCMD [\"node\", \"server.js\"]\n").is_empty());
    }
}
