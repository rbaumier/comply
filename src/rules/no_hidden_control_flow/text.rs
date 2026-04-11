use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim();
            if trimmed.starts_with('@') && !trimmed.starts_with("@ts-") {
                let decorator_start = i;
                let mut count = 0;
                // Count consecutive decorator lines.
                while i < lines.len() {
                    let t = lines[i].trim();
                    if t.starts_with('@') && !t.starts_with("@ts-") {
                        count += 1;
                        i += 1;
                    } else {
                        break;
                    }
                }
                if count >= 3 {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: decorator_start + 1,
                        column: 1,
                        rule_id: "no-hidden-control-flow".into(),
                        message: format!(
                            "{count} stacked decorators hide control flow — compose into fewer decorators or use explicit middleware."
                        ),
                        severity: Severity::Warning,
                    });
                }
            } else {
                i += 1;
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_three_decorators() {
        let src = "@Auth()\n@Log()\n@Cache()\nclass MyService {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_two_decorators() {
        let src = "@Auth()\n@Log()\nclass MyService {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_four_decorators() {
        let src = "@A\n@B\n@C\n@D\nfunction handler() {}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains('4'));
    }

    #[test]
    fn ignores_ts_directives() {
        let src = "@ts-ignore\n@ts-expect-error\n@Auth()\nclass X {}";
        assert!(run(src).is_empty());
    }
}
