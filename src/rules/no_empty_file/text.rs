use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns true if the source has no meaningful content — only whitespace,
/// comments, and `"use strict"` / `'use strict'` directives.
fn is_empty(source: &str) -> bool {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Single-line comments
        if trimmed.starts_with("//") {
            continue;
        }
        // Block comment fragments
        if trimmed.starts_with("/*") || trimmed.starts_with('*') || trimmed.ends_with("*/") {
            continue;
        }
        // "use strict" directive
        if trimmed == r#""use strict";"#
            || trimmed == r#"'use strict';"#
            || trimmed == r#""use strict""#
            || trimmed == r#"'use strict'"#
        {
            continue;
        }
        // Triple-slash TS directives (e.g. `/// <reference ... />`)
        if trimmed.starts_with("///") {
            continue;
        }
        return false;
    }
    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_empty(ctx.source) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: 1,
            column: 1,
            rule_id: "no-empty-file".into(),
            message: "File has no meaningful content — remove it or add code.".into(),
            severity: Severity::Warning,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_empty_file() {
        assert_eq!(run("").len(), 1);
    }

    #[test]
    fn flags_whitespace_only() {
        assert_eq!(run("  \n\n  \n").len(), 1);
    }

    #[test]
    fn flags_comments_only() {
        assert_eq!(run("// this file is empty\n/* nothing */").len(), 1);
    }

    #[test]
    fn flags_use_strict_only() {
        assert_eq!(run("\"use strict\";\n").len(), 1);
    }

    #[test]
    fn allows_file_with_code() {
        assert!(run("export const x = 1;").is_empty());
    }

    #[test]
    fn allows_file_with_import() {
        assert!(run("import { foo } from './foo';").is_empty());
    }

    #[test]
    fn flags_triple_slash_only() {
        assert_eq!(run("/// <reference types=\"vite/client\" />").len(), 1);
    }
}
