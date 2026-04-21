use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("createServerFn") {
            return vec![];
        }
        let file_name = ctx
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if file_name.ends_with(".functions.ts") || file_name.ends_with(".functions.tsx") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if line.contains("createServerFn") && line.contains("(") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`createServerFn` must be in a `*.functions.ts` file, not `{file_name}`."
                    ),
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

    fn run(path: &str, src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), src))
    }

    #[test]
    fn flags_wrong_file_name() {
        assert_eq!(
            run("src/users/actions.ts", "const fn = createServerFn()").len(),
            1
        );
    }

    #[test]
    fn allows_functions_ts() {
        assert!(run(
            "src/users/users.functions.ts",
            "const fn = createServerFn()"
        )
        .is_empty());
    }

    #[test]
    fn ignores_no_server_fn() {
        assert!(run("src/users/actions.ts", "const x = 1").is_empty());
    }
}
