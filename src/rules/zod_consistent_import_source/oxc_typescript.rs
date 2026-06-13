use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Official versioned/variant subpath entry points declared in zod's
/// `package.json` `exports`. These are stable public API, not internal paths,
/// so importing from them is not an inconsistency.
const OFFICIAL_ZOD_SUBPATHS: &[&str] = &["v3", "v4", "v4-mini", "mini", "locales"];

/// Returns true if `spec` is a non-standard zod subpath import, i.e. it starts
/// with `zod/` but does not target an official published subpath entry point.
fn is_non_standard_zod_subpath(spec: &str) -> bool {
    let Some(subpath) = spec.strip_prefix("zod/") else {
        return false;
    };
    // The first path segment determines the entry point (e.g. `v4` in
    // `zod/v4/locales/en`). Official entry points and their nested public
    // modules (`zod/v4/core`, `zod/v4/locales/*`) are all acceptable.
    let first_segment = subpath.split('/').next().unwrap_or(subpath);
    !OFFICIAL_ZOD_SUBPATHS.contains(&first_segment)
}

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
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let src_value = import.source.value.as_str();
        if !is_non_standard_zod_subpath(src_value) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Import from '{src_value}' uses a non-standard zod subpath. Use consistent import source for zod."
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
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_official_versioned_subpaths() {
        assert!(run_on("import * as z from 'zod/v4';").is_empty());
        assert!(run_on("import * as z from 'zod/v3';").is_empty());
        assert!(run_on("import * as z from 'zod/v4-mini';").is_empty());
        assert!(run_on("import * as z from 'zod/v4/mini';").is_empty());
        assert!(run_on("import { en } from 'zod/v4/locales/en';").is_empty());
        assert!(run_on("import { z } from 'zod/mini';").is_empty());
    }

    #[test]
    fn flags_internal_zod_path() {
        assert_eq!(run_on("import { z } from 'zod/src/internal/foo';").len(), 1);
        assert_eq!(run_on("import { z } from 'zod/dist/cjs/index.js';").len(), 1);
    }

    #[test]
    fn allows_main_zod_import() {
        assert!(run_on("import { z } from 'zod';").is_empty());
    }

    #[test]
    fn allows_scoped_zod_package() {
        assert!(run_on("import { foo } from '@zod/utils';").is_empty());
    }
}
