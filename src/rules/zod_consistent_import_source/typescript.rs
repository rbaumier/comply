//! zod-consistent-import-source backend — flag imports from non-standard
//! `zod/<subpath>` paths.
//!
//! Why: importing from internal zod paths (e.g. `zod/src/...`, `zod/dist/...`)
//! circumvents the public API surface and produces inconsistent schemas. The
//! official versioned/variant entry points (`zod/v3`, `zod/v4`, `zod/v4-mini`,
//! `zod/v4/locales/*`) are stable public API and are not flagged.

use crate::diagnostic::{Diagnostic, Severity};

/// Official versioned/variant subpath entry points declared in zod's
/// `package.json` `exports`. These are stable public API, not internal paths,
/// so importing from them is not an inconsistency.
const OFFICIAL_ZOD_SUBPATHS: &[&str] = &["v3", "v4", "v4-mini", "mini", "locales"];

/// Returns true if `spec` (quoted) is a non-standard zod subpath import, i.e. it
/// starts with `zod/` but does not target an official published subpath entry
/// point. Scoped packages like `@zod/foo` are ignored.
fn is_non_standard_zod_subpath(spec: &str) -> bool {
    let inner = spec.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    let Some(subpath) = inner.strip_prefix("zod/") else {
        return false;
    };
    let first_segment = subpath.split('/').next().unwrap_or(subpath);
    !OFFICIAL_ZOD_SUBPATHS.contains(&first_segment)
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(src) = node.child_by_field_name("source") else { return };
    let text = src.utf8_text(source).unwrap_or("");
    if !is_non_standard_zod_subpath(text) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "zod-consistent-import-source".into(),
        message: format!(
            "Import from {text} uses a non-standard zod subpath. Use consistent import source for zod."
        ),
        severity: Severity::Warning,
        span: None,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_internal_zod_path() {
        let d = run_on("import { z } from 'zod/src/internal/foo';");
        assert_eq!(d.len(), 1);
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
    fn allows_main_zod_import() {
        assert!(run_on("import { z } from 'zod';").is_empty());
    }

    #[test]
    fn allows_scoped_zod_package() {
        assert!(run_on("import { foo } from '@zod/utils';").is_empty());
    }
}
