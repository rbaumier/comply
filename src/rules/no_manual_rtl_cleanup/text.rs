//! no-manual-rtl-cleanup backend — detect manual `cleanup` imports from
//! `@testing-library` in test files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns `true` if the file path looks like a test file.
fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("__tests__")
        || s.contains("_test.")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if !line.contains("@testing-library") {
                continue;
            }
            if !has_cleanup_import(line) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-manual-rtl-cleanup".into(),
                message: "Manual `cleanup` import from `@testing-library` — \
                          Vitest runs cleanup automatically after each test."
                    .into(),
                severity: Severity::Warning,
            });
        }
        diagnostics
    }
}

/// Check whether `line` contains an import of `cleanup` from `@testing-library`.
fn has_cleanup_import(line: &str) -> bool {
    // Must mention `cleanup` as a word boundary — not inside another identifier.
    // We look for `cleanup` preceded by `{`, `,`, or whitespace and followed by
    // `}`, `,`, whitespace, or end-of-string (covers named imports).
    let Some(pos) = line.find("cleanup") else {
        return false;
    };

    let before = if pos > 0 {
        line.as_bytes()[pos - 1]
    } else {
        b' '
    };
    let after_pos = pos + "cleanup".len();
    let after = if after_pos < line.len() {
        line.as_bytes()[after_pos]
    } else {
        b' '
    };

    let valid_before = matches!(before, b'{' | b',' | b' ' | b'\t');
    let valid_after = matches!(after, b'}' | b',' | b' ' | b'\t');

    valid_before && valid_after
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_cleanup_import() {
        let diags = run(
            "src/App.test.tsx",
            "import { cleanup } from '@testing-library/react';",
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-manual-rtl-cleanup");
    }

    #[test]
    fn flags_cleanup_among_other_imports() {
        let diags = run(
            "src/App.spec.ts",
            "import { render, cleanup } from '@testing-library/react';",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_render_only() {
        let diags = run(
            "src/App.test.tsx",
            "import { render } from '@testing-library/react';",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        let diags = run(
            "src/utils.ts",
            "import { cleanup } from '@testing-library/react';",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_in_dunder_tests_dir() {
        let diags = run(
            "src/__tests__/App.tsx",
            "import { cleanup } from '@testing-library/react';",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_in_underscore_test_file() {
        let diags = run(
            "src/App_test.ts",
            "import { cleanup } from '@testing-library/react';",
        );
        assert_eq!(diags.len(), 1);
    }
}
