//! no-duplicate-imports OXC backend — flag multiple imports from the same module.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // (module source, is_type_import) -> first line number
        let mut seen: HashMap<(&str, bool), usize> = HashMap::new();

        for node in semantic.nodes().iter() {
            let AstKind::ImportDeclaration(import) = node.kind() else {
                continue;
            };
            let module = import.source.value.as_str();
            if module.is_empty() {
                continue;
            }
            let is_type = import.import_kind.is_type();
            let key = (module, is_type);
            let (line, column) =
                byte_offset_to_line_col(ctx.source, import.span.start as usize);
            if let Some(&first_line) = seen.get(&key) {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Duplicate import from `{}` \u{2014} already imported on line {}. Merge into a single statement.",
                        module, first_line
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            } else {
                seen.insert(key, line);
            }
        }
        diagnostics
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod oxc_tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_two_imports_from_same_module_issue_1081() {
        // Regression for rbaumier/comply#1081 — two value imports from the
        // same module are flagged once, on the second statement. The
        // duplicate `import-no-duplicates` rule that fired on the same line
        // has been deleted.
        let src = "\
import { foo } from './mod';
import { bar } from './mod';
";
        let diags = run(src);
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn allows_single_import_per_module() {
        let src = "\
import { foo } from './a';
import { bar } from './b';
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_type_only_import_alongside_value_import() {
        // A `import type` and a value `import` from the same module are
        // distinct statements (different `is_type` key) and not duplicates.
        let src = "\
import { foo } from './mod';
import type { Bar } from './mod';
";
        assert!(run(src).is_empty());
    }
}
