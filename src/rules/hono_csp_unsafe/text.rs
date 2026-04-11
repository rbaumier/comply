use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn has_secure_headers_import(source: &str) -> bool {
    source.contains("secureHeaders") || source.contains("NONCE")
}

fn has_hono_import(source: &str) -> bool {
    source.contains("hono/secure-headers")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !has_hono_import(ctx.source) || !has_secure_headers_import(ctx.source) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("'unsafe-inline'") || line.contains("\"unsafe-inline\"") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "hono-csp-unsafe".into(),
                    message: "`'unsafe-inline'` in CSP defeats its purpose — use nonces instead.".into(),
                    severity: Severity::Error,
                });
            }
            if line.contains("'unsafe-eval'") || line.contains("\"unsafe-eval\"") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "hono-csp-unsafe".into(),
                    message: "`'unsafe-eval'` in CSP enables code injection.".into(),
                    severity: Severity::Error,
                });
            }
            let norm: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            if norm.contains("defaultSrc:['*']") || norm.contains("defaultSrc:[\"*\"]") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "hono-csp-unsafe".into(),
                    message: "`defaultSrc: ['*']` allows loading resources from any origin.".into(),
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
    fn flags_unsafe_inline() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\nsecureHeaders({ contentSecurityPolicy: { scriptSrc: ['unsafe-inline'] } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_unsafe_eval() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\nsecureHeaders({ contentSecurityPolicy: { scriptSrc: ['unsafe-eval'] } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_default_src_wildcard() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\nsecureHeaders({ contentSecurityPolicy: { defaultSrc: ['*'] } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_safe_csp() {
        let src = "import { secureHeaders, NONCE } from 'hono/secure-headers';\nsecureHeaders({ contentSecurityPolicy: { scriptSrc: ['self', NONCE] } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "const policy = { scriptSrc: ['unsafe-inline'] };";
        assert!(run(src).is_empty());
    }
}
