use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn has_disabled_cert_verification(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();

    // rejectUnauthorized: false  or  rejectUnauthorized = false
    if lower.contains("rejectunauthorized") && lower.contains("false") {
        return true;
    }

    // NODE_TLS_REJECT_UNAUTHORIZED set to '0' or "0"
    if lower.contains("node_tls_reject_unauthorized") {
        return true;
    }

    // verify: false in HTTP client contexts (e.g., axios, got, request)
    if lower.contains("verify") {
        // Look for `verify: false` or `verify = false` pattern
        let patterns = ["verify: false", "verify:false", "verify = false", "verify =false"];
        for pat in patterns {
            if lower.contains(pat) {
                return true;
            }
        }
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_disabled_cert_verification(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-unverified-certificate".into(),
                    message:
                        "Disabled SSL certificate verification — enables MITM attacks.".into(),
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
    fn flags_reject_unauthorized_false() {
        assert_eq!(run("rejectUnauthorized: false").len(), 1);
    }

    #[test]
    fn flags_node_tls_env() {
        assert_eq!(
            run("process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0'").len(),
            1
        );
    }

    #[test]
    fn flags_verify_false() {
        assert_eq!(run("verify: false,").len(), 1);
    }

    #[test]
    fn allows_reject_unauthorized_true() {
        assert!(run("rejectUnauthorized: true").is_empty());
    }

    #[test]
    fn allows_verify_true() {
        assert!(run("verify: true").is_empty());
    }
}
