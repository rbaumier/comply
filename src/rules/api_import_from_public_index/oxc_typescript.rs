//! api-import-from-public-index oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.path.to_string_lossy().contains(".test.")
            || ctx.path.to_string_lossy().contains(".spec.")
        {
            return;
        }

        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let import_path = import.source.value.as_str();

        // Only cross-feature imports (2+ parent segments).
        let parent_count = import_path.split('/').filter(|s| *s == "..").count();
        if parent_count < 2 {
            return;
        }

        // If the import traverses out of the source tree entirely (e.g. into
        // `scripts/` from `src/api/features/auth/`), it is not a cross-feature
        // boundary violation — the destination has no public index to import from.
        let parent_dir_depth = ctx.path
            .parent()
            .map(|p| p.components().count())
            .unwrap_or(0);
        if parent_count >= parent_dir_depth {
            return;
        }

        // A bare feature-root import (`../../users`) has exactly one
        // non-`..` segment — the feature name — and that *is* the public
        // index. Anything deeper (`../../users/db/queries`) has 2+ and is
        // reaching into internals.
        let non_parent_segments: Vec<&str> = import_path
            .split('/')
            .filter(|s| *s != ".." && !s.is_empty())
            .collect();
        if non_parent_segments.len() <= 1 {
            return;
        }

        // Flag if the import doesn't end at an index file.
        let last_segment = *non_parent_segments.last().unwrap_or(&"");
        if last_segment == "index" {
            return;
        }
        // Skip obvious shared-leaf imports.
        if last_segment == "types" || last_segment == "utils" {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.source.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Import from `{import_path}` crosses a feature boundary — import from the public index instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
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
mod tests {
    
    use super::Check;

    #[test]
    fn flags_deep_cross_feature_import() {
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "import { query } from '../../users/db/queries'", "src/api/features/auth/handler.ts")
            .len(),
            1
        );
    }

    #[test]
    fn allows_index_import() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "import { User } from '../../users'", "src/api/features/auth/handler.ts")
        .is_empty());
    }

    // Regression: import from scripts/ (outside src/) must not fire — issue #492
    #[test]
    fn allows_import_outside_src_tree() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "import { seedAdminCdr } from '../../../../scripts/seed-admin-cdr'", "src/api/features/auth/seed-admin-cdr.integration.test.ts")
        .is_empty());
    }

    // Regression: test files must be allowed to import internal modules — issue #798
    #[test]
    fn allows_test_file_importing_internal_module() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "import { renderToString } from '../../src/adapter/deno/ssg.ts'", "runtime-tests/deno/ssg.test.tsx")
        .is_empty());
    }

    #[test]
    fn allows_spec_file_importing_internal_module() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "import { queryUsers } from '../../api/features/users/db/queries'", "src/features/auth/handlers.spec.ts")
        .is_empty());
    }
}
