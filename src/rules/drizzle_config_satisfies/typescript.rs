//! In files whose path contains `drizzle.config`, flag variable declarations
//! annotated `: Config` or default exports annotated `: Config` — they should
//! use `satisfies Config` instead.

use crate::diagnostic::{Diagnostic, Severity};

fn path_is_drizzle_config(path: &std::path::Path) -> bool {
    path.to_string_lossy().contains("drizzle.config")
}

crate::ast_check! { prefilter = ["drizzle.config"] => |node, source, ctx, diagnostics|
    if !path_is_drizzle_config(ctx.path) {
        return;
    }
    // Case A: `const config: Config = ...` — a variable_declarator with a
    // type_annotation whose type is `Config`.
    if node.kind() == "variable_declarator" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "type_annotation" {
                let text = child.utf8_text(source).unwrap_or("");
                // `: Config` or `: Config` with spaces.
                let t = text.trim_start_matches(':').trim();
                if t == "Config" {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &node,
                        super::META.id,
                        "Use `satisfies Config` instead of `: Config` — prefer `export default { ... } satisfies Config`.".into(),
                        Severity::Warning,
                    ));
                    return;
                }
            }
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
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "drizzle.config.ts")
    }

    fn run_other(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "other.ts")
    }

    #[test]
    fn flags_const_config_type_annotation() {
        let src = "const config: Config = { out: './drizzle' }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_satisfies_config() {
        let src = "export default { out: './drizzle' } satisfies Config";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_drizzle_config_files() {
        let src = "const config: Config = { out: './drizzle' }";
        assert!(run_other(src).is_empty());
    }
}
