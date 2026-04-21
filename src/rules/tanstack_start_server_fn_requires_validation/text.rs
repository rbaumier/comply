use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("createServerFn") {
            return vec![];
        }
        if src.contains(".input(") || src.contains(".safeParse(") || src.contains(".parse(") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("createServerFn") && line.contains("(") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`createServerFn` without `.input()` validation accepts unvalidated data at the RPC boundary.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("api.functions.ts"), src))
    }

    #[test]
    fn flags_no_input_validation() {
        assert_eq!(
            run("const fn = createServerFn().handler(async () => { await db.delete(x) })").len(),
            1
        );
    }

    #[test]
    fn allows_with_input() {
        assert!(run(
            "const fn = createServerFn().input(z.object({ id: z.string() })).handler(async (ctx) => {})"
        )
        .is_empty());
    }

    #[test]
    fn ignores_non_server_fn_files() {
        assert!(run("const x = 1;").is_empty());
    }
}
