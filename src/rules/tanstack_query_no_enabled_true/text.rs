use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("useQuery") && !ctx.source.contains("useSuspenseQuery") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !t.starts_with("//") && (t.contains("enabled: true") || t.contains("enabled:true"))
            {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: line.find("enabled").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message: "`enabled: true` is redundant — queries are enabled by default."
                        .into(),
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
    fn flags_enabled_true() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], queryFn: f, enabled: true })").len(),
            1
        );
    }

    #[test]
    fn allows_enabled_condition() {
        assert!(
            run("useQuery({ queryKey: ['x'], queryFn: f, enabled: !!userId })").is_empty()
        );
    }

    #[test]
    fn allows_no_enabled() {
        assert!(run("useQuery({ queryKey: ['x'], queryFn: f })").is_empty());
    }
}
