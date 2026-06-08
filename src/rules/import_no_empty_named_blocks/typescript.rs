//! import-no-empty-named-blocks backend — forbid `import { } from '...'`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");

    // Detect `{ }` or `{}` pattern indicating empty named imports.
    // Must have braces but no identifiers between them.
    if let Some(open) = text.find('{')
        && let Some(close) = text[open..].find('}') {
            let between = &text[open + 1..open + close];
            let trimmed = between.trim();
            if trimmed.is_empty() {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "import-no-empty-named-blocks".into(),
                    message: "Unexpected empty named import block.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_empty_braces() {
        let d = run_on("import { } from 'foo';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("empty named import"));
    }

    #[test]
    fn flags_empty_braces_no_space() {
        let d = run_on("import {} from 'foo';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_named_imports() {
        assert!(run_on("import { foo } from 'bar';").is_empty());
    }
}
