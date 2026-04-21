use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("Route.useSearch(") && !src.contains("useSearch()") {
            return vec![];
        }
        if src.contains("validateSearch:") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("Route.useSearch(")
                || (line.contains("useSearch(") && line.contains("Route"))
            {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`Route.useSearch()` without `validateSearch:` in the route config accepts untyped search params.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                break;
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
        Check.check(&CheckCtx::for_test(Path::new("route.tsx"), src))
    }

    #[test]
    fn flags_use_search_without_validate() {
        assert_eq!(run("const { page } = Route.useSearch()").len(), 1);
    }

    #[test]
    fn allows_with_validate_search() {
        assert!(run(
            "const { page } = Route.useSearch()\nconst route = createFileRoute('/posts')({ validateSearch: z.object({ page: z.number() }) })"
        )
        .is_empty());
    }

    #[test]
    fn ignores_no_use_search() {
        assert!(run("const route = createFileRoute('/posts')({})").is_empty());
    }
}
