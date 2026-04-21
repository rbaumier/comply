use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("useQuery") {
            return vec![];
        }
        if src.contains("queryOptions(") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("useQuery({")
                || (line.contains("useQuery(") && line.contains("queryKey:"))
            {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Extract inline `useQuery` options to a `queryOptions()` factory for reuse and type-safety.".into(),
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
    fn flags_inline_options() {
        assert_eq!(
            run("useQuery({ queryKey: ['users'], queryFn: fetchUsers })").len(),
            1
        );
    }

    #[test]
    fn allows_query_options_factory() {
        assert!(run(
            "const opts = queryOptions({ queryKey: ['users'], queryFn: fetchUsers })\nuseQuery(opts)"
        )
        .is_empty());
    }
}
