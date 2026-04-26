//! zod-consistent-import-source backend — flag imports from `zod/<subpath>`.
//!
//! Why: importing from `zod/v4`, `zod/mini`, or other subpaths alongside the
//! main `zod` package produces mixed schema types and inconsistent behavior.
//! Stick to a single import source (`zod`) across the codebase.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if `spec` (quoted) starts with `zod/` (a subpath import).
/// Scoped packages like `@zod/foo` are ignored.
fn is_zod_subpath(spec: &str) -> bool {
    let inner = spec.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    inner.starts_with("zod/")
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(src) = node.child_by_field_name("source") else { return };
    let text = src.utf8_text(source).unwrap_or("");
    if !is_zod_subpath(text) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_zod_v4_import() {
        let d = run_on("import { z } from 'zod/v4';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_zod_mini_import() {
        let d = run_on("import { z } from 'zod/mini';");
        assert_eq!(d.len(), 1);
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
