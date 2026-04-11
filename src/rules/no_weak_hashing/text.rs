use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const PATTERNS: &[&str] = &[
    "createHash('md5')",
    "createHash(\"md5\")",
    "createHash('sha1')",
    "createHash(\"sha1\")",
    "MD5(",
    "SHA1(",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for pattern in PATTERNS {
                if line.contains(pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-weak-hashing".into(),
                        message: format!(
                            "Weak hashing algorithm `{}` — use SHA-256 or stronger.",
                            pattern.trim_end_matches('('),
                        ),
                        severity: Severity::Error,
                    });
                    break;
                }
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
    fn flags_md5_single_quotes() {
        assert_eq!(run("const h = crypto.createHash('md5');").len(), 1);
    }

    #[test]
    fn flags_sha1_double_quotes() {
        assert_eq!(run("const h = crypto.createHash(\"sha1\");").len(), 1);
    }

    #[test]
    fn flags_md5_function() {
        assert_eq!(run("const hash = MD5(data);").len(), 1);
    }

    #[test]
    fn allows_sha256() {
        assert!(run("const h = crypto.createHash('sha256');").is_empty());
    }
}
