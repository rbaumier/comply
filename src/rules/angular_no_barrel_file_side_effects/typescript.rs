//! In an `index.ts` file, only allow re-export statements.

use crate::diagnostic::{Diagnostic, Severity};

fn is_barrel_path(path: &std::path::Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("index.ts") | Some("public-api.ts") | Some("public_api.ts")
    )
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    if !is_barrel_path(ctx.path) { return; }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "export_statement" | "import_statement" | "comment" | "empty_statement" => continue,
            _ => {
                let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
                let snippet: String = text.chars().take(60).collect();
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &child,
                    super::META.id,
                    format!("Barrel file should only re-export — found side-effecting statement: `{snippet}`."),
                    Severity::Warning,
                ));
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

    fn run_idx(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "index.ts")
    }
    fn run_other(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "thing.ts")
    }

    #[test]
    fn flags_side_effect_in_barrel() {
        let src = "export * from './a';\nconsole.log('side');";
        assert_eq!(run_idx(src).len(), 1);
    }

    #[test]
    fn allows_pure_reexports() {
        let src = "export * from './a';\nexport { B } from './b';";
        assert!(run_idx(src).is_empty());
    }

    #[test]
    fn ignores_non_barrel_files() {
        let src = "console.log('side');";
        assert!(run_other(src).is_empty());
    }
}
