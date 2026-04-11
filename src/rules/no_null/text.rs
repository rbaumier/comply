use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();

            // Skip single-line comments.
            if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                continue;
            }

            // Walk through the line looking for `null` as a standalone token.
            let bytes = line.as_bytes();
            let mut pos = 0;
            while pos + 4 <= bytes.len() {
                let Some(found) = line[pos..].find("null") else {
                    break;
                };
                let abs = pos + found;

                // Check that `null` is a standalone token (not part of an identifier).
                let before_ok =
                    abs == 0 || !bytes[abs - 1].is_ascii_alphanumeric() && bytes[abs - 1] != b'_';
                let after_ok = abs + 4 >= bytes.len()
                    || !bytes[abs + 4].is_ascii_alphanumeric() && bytes[abs + 4] != b'_';

                if before_ok && after_ok {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: abs + 1,
                        rule_id: "no-null".into(),
                        message: "Use `undefined` instead of `null`.".into(),
                        severity: Severity::Warning,
                    });
                    break; // one diagnostic per line
                }

                pos = abs + 4;
            }
        }
        diagnostics
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
    fn flags_null_literal() {
        assert_eq!(run("const x = null;").len(), 1);
    }

    #[test]
    fn flags_null_comparison() {
        assert_eq!(run("if (x === null) {}").len(), 1);
    }

    #[test]
    fn flags_return_null() {
        assert_eq!(run("return null;").len(), 1);
    }

    #[test]
    fn allows_nullable_type_annotation() {
        // `nullable` contains "null" but is an identifier — should not flag.
        assert!(run("const nullable: string | undefined = undefined;").is_empty());
    }

    #[test]
    fn allows_null_in_comments() {
        assert!(run("// returns null if not found").is_empty());
    }

    #[test]
    fn allows_undefined() {
        assert!(run("const x = undefined;").is_empty());
    }
}
