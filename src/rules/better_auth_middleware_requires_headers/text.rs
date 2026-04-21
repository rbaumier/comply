use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(file_name) = ctx.path.file_name().and_then(|n| n.to_str()) else {
            return Vec::new();
        };
        if file_name != "middleware.ts" && file_name != "middleware.tsx" {
            return Vec::new();
        }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            if !line.contains("getSession(") {
                continue;
            }
            let end = (idx + 5).min(lines.len());
            let window = &lines[idx..end];
            let has_headers = window.iter().any(|l| l.contains("headers:"));
            if has_headers {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "better-auth-middleware-requires-headers".into(),
                message: "`getSession()` in middleware must forward request headers — pass `{ headers: await headers() }` or session lookup will fail.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run_middleware(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("middleware.ts"), s))
    }
    fn run_other(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("api.ts"), s))
    }
    #[test]
    fn flags_get_session_no_args() {
        assert_eq!(run_middleware("const session = getSession()").len(), 1);
    }
    #[test]
    fn flags_get_session_without_headers() {
        assert_eq!(
            run_middleware("const session = await getSession({ foo: 1 })").len(),
            1
        );
    }
    #[test]
    fn allows_get_session_with_headers() {
        assert!(
            run_middleware("const session = await getSession({ headers: await headers() })")
                .is_empty()
        );
    }
    #[test]
    fn ignores_non_middleware_files() {
        assert!(run_other("const session = getSession()").is_empty());
    }
    #[test]
    fn allows_multiline_get_session_with_headers() {
        let src = "const session = await getSession({\n  headers: h,\n})";
        assert!(run_middleware(src).is_empty());
    }
}
