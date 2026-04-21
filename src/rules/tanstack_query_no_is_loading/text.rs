use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("useQuery") && !src.contains("useInfiniteQuery") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("isLoading")
                && !line.trim().starts_with("//")
                && let Some(col) = line.find("isLoading")
            {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "`isLoading` was removed in TanStack Query v5 — use `isPending` instead.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }

    #[test]
    fn flags_is_loading() {
        assert_eq!(
            run("const { isLoading } = useQuery({ queryKey: ['x'], queryFn: f })").len(),
            1
        );
    }

    #[test]
    fn allows_is_pending() {
        assert!(run("const { isPending } = useQuery({ queryKey: ['x'], queryFn: f })").is_empty());
    }

    #[test]
    fn ignores_file_without_usequery() {
        assert!(run("const isLoading = true").is_empty());
    }
}
