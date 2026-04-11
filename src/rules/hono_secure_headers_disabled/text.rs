use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const SECURITY_HEADERS: &[&str] = &[
    "strictTransportSecurity",
    "xFrameOptions",
    "xContentTypeOptions",
    "removePoweredBy",
    "referrerPolicy",
];

fn has_secure_headers_import(source: &str) -> bool {
    source.contains("hono/secure-headers")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !has_secure_headers_import(ctx.source) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let norm: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            for &header in SECURITY_HEADERS {
                let pattern = format!("{}:false", header);
                if norm.contains(&pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "hono-secure-headers-disabled".into(),
                        message: format!("`{}` is explicitly disabled — this removes a security protection.", header),
                        severity: Severity::Error,
                    });
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
    fn flags_disabled_hsts() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\napp.use(secureHeaders({\n  strictTransportSecurity: false\n}));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_disabled_x_frame_options() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\napp.use(secureHeaders({ xFrameOptions: false }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiple_disabled() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\nsecureHeaders({\n  xFrameOptions: false,\n  removePoweredBy: false\n});";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_default_secure_headers() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\napp.use(secureHeaders());";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "secureHeaders({ xFrameOptions: false });";
        assert!(run(src).is_empty());
    }
}
