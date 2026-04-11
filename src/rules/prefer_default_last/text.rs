use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut default_line: Option<usize> = None;

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            if trimmed.starts_with("default:") || trimmed == "default :" {
                default_line = Some(idx);
            } else if trimmed.starts_with("case ") && default_line.is_some() {
                // A `case` appeared after `default:` — flag the default line.
                let dl = default_line.take().unwrap();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: dl + 1,
                    column: 1,
                    rule_id: "prefer-default-last".into(),
                    message: "`default` clause should be the last clause in the switch statement."
                        .into(),
                    severity: Severity::Warning,
                });
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
    fn flags_default_before_case() {
        let src = "switch (x) {\n  default:\n    break;\n  case 1:\n    break;\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 2);
    }

    #[test]
    fn allows_default_last() {
        let src = "switch (x) {\n  case 1:\n    break;\n  default:\n    break;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_default_in_middle() {
        let src =
            "switch (x) {\n  case 1:\n    break;\n  default:\n    break;\n  case 2:\n    break;\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 4);
    }
}
