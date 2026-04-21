use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("queryFn") || !src.contains("fetch(") {
            return vec![];
        }
        if src.contains("res.ok") || src.contains("response.ok") || src.contains(".ok)") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("queryFn") && line.contains("fetch(") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`queryFn` with `fetch()` must check `res.ok` and throw on error so TanStack Query can retry.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_fetch_no_ok_check() {
        assert_eq!(
            run("queryFn: async () => { const res = await fetch('/api'); return res.json() }")
                .len(),
            1
        );
    }

    #[test]
    fn allows_with_ok_check() {
        assert!(run(
            "queryFn: async () => { const res = await fetch('/api'); if (!res.ok) throw new Error('err'); return res.json() }"
        )
        .is_empty());
    }
}
