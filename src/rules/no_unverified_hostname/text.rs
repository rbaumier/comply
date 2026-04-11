use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn disables_hostname_check(line: &str) -> bool {
    let trimmed = line.trim();
    // checkServerIdentity: null
    if trimmed.contains("checkServerIdentity") && trimmed.contains("null") {
        return true;
    }
    // checkServerIdentity: () => or checkServerIdentity: function()
    // that returns nothing (empty arrow / empty function body)
    if let Some(pos) = trimmed.find("checkServerIdentity") {
        let after = &trimmed[pos + "checkServerIdentity".len()..];
        let after = after.trim_start().trim_start_matches(':').trim_start();
        // () => {} or () => undefined
        if after.starts_with("()")
            || after.starts_with("function(")
            || after.starts_with("function (")
        {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if disables_hostname_check(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-unverified-hostname".into(),
                    message: "`checkServerIdentity` override disables TLS hostname verification."
                        .into(),
                    severity: Severity::Error,
                });
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
    fn flags_arrow_noop() {
        assert_eq!(run("  checkServerIdentity: () => {},").len(), 1);
    }

    #[test]
    fn flags_function_noop() {
        assert_eq!(run("  checkServerIdentity: function() {},").len(), 1);
    }

    #[test]
    fn flags_null() {
        assert_eq!(run("  checkServerIdentity: null,").len(), 1);
    }

    #[test]
    fn allows_proper_check() {
        assert!(run("  checkServerIdentity: verifyHost,").is_empty());
    }

    #[test]
    fn allows_unrelated() {
        assert!(run("const x = tls.connect({ host: 'example.com' });").is_empty());
    }
}
