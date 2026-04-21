use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const REMOVED_CALLBACKS: &[&str] = &["onSuccess:", "onError:", "onSettled:"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("useQuery") && !src.contains("useSuspenseQuery") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") {
                continue;
            }
            for cb in REMOVED_CALLBACKS {
                if t.contains(cb) {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: line.find(cb).unwrap_or(0) + 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{cb}` on `useQuery` was removed in TanStack Query v5 — move side-effects to `useEffect`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
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
    fn flags_on_success() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], queryFn: f, onSuccess: () => {} })").len(),
            1
        );
    }

    #[test]
    fn allows_no_callbacks() {
        assert!(run("useQuery({ queryKey: ['x'], queryFn: f })").is_empty());
    }

    #[test]
    fn ignores_no_usequery() {
        assert!(run("useMutation({ onSuccess: () => {} })").is_empty());
    }
}
