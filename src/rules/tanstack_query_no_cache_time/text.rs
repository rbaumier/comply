use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("QueryClient") && !src.contains("useQuery") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if t.contains("cacheTime")
                && !t.starts_with("//")
                && let Some(col) = line.find("cacheTime")
            {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "`cacheTime` was renamed to `gcTime` in TanStack Query v5.".into(),
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
    fn flags_cache_time() {
        assert_eq!(
            run("new QueryClient({ defaultOptions: { queries: { cacheTime: 5000 } } })").len(),
            1
        );
    }

    #[test]
    fn allows_gc_time() {
        assert!(
            run("new QueryClient({ defaultOptions: { queries: { gcTime: 5000 } } })").is_empty()
        );
    }
}
