use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn imports_hono(source: &str) -> bool {
    source.contains("from 'hono'") || source.contains("from \"hono\"")
}

fn has_routes(source: &str) -> bool {
    source.contains(".get(")
        || source.contains(".post(")
        || source.contains(".put(")
        || source.contains(".delete(")
        || source.contains(".patch(")
}

fn has_secure_headers(source: &str) -> bool {
    source.contains("hono/secure-headers")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !imports_hono(ctx.source) || !has_routes(ctx.source) || has_secure_headers(ctx.source) {
            return Vec::new();
        }

        // Find the line with `new Hono()` or the hono import to report the diagnostic.
        let mut report_line = 1;
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("new Hono(") || line.contains("from 'hono'") || line.contains("from \"hono\"") {
                report_line = idx + 1;
                break;
            }
        }

        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: report_line,
            column: 1,
            rule_id: "hono-missing-secure-headers".into(),
            message: "Hono app defines routes without `secureHeaders()` middleware — security headers are missing.".into(),
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
    fn flags_hono_app_without_secure_headers() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.get('/', handler);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_hono_app_with_secure_headers() {
        let src = "import { Hono } from 'hono';\nimport { secureHeaders } from 'hono/secure-headers';\nconst app = new Hono();\napp.use(secureHeaders());\napp.get('/', handler);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "app.get('/', handler);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_hono_import_without_routes() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();";
        assert!(run(src).is_empty());
    }
}
