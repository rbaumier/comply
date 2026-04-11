use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// True if the line contains `eval(` as a standalone call, not as
/// part of a longer word like `evaluate`.
fn has_eval_call(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("eval(") {
        let abs = start + pos;
        if abs == 0 {
            return true;
        }
        let prev = line.as_bytes()[abs - 1];
        if !prev.is_ascii_alphanumeric() && prev != b'_' {
            return true;
        }
        start = abs + 5;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_eval_call(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-eval".into(),
                    message: "`eval()` enables arbitrary code injection — remove it.".into(),
                    severity: Severity::Error,
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
    fn flags_eval_call() {
        assert_eq!(run(r#"const result = eval("1 + 2");"#).len(), 1);
    }

    #[test]
    fn flags_eval_at_start() {
        assert_eq!(run(r#"eval(userInput);"#).len(), 1);
    }

    #[test]
    fn allows_evaluate() {
        assert!(run("const v = evaluate(expr);").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// eval(x) is dangerous").is_empty());
    }
}
