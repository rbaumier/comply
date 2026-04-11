use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Detect `.charCodeAt(`
            let mut start = 0;
            while let Some(pos) = line[start..].find(".charCodeAt(") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: start + pos + 1,
                    rule_id: "prefer-code-point".into(),
                    message: "Prefer `String#codePointAt()` over `String#charCodeAt()`.".into(),
                    severity: Severity::Warning,
                });
                start += pos + 12;
            }

            // Detect `String.fromCharCode(`
            start = 0;
            while let Some(pos) = line[start..].find("String.fromCharCode(") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: start + pos + 1,
                    rule_id: "prefer-code-point".into(),
                    message: "Prefer `String.fromCodePoint()` over `String.fromCharCode()`."
                        .into(),
                    severity: Severity::Warning,
                });
                start += pos + 20;
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
    fn flags_char_code_at() {
        let d = run("const c = str.charCodeAt(0);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("codePointAt"));
    }

    #[test]
    fn flags_from_char_code() {
        let d = run("const s = String.fromCharCode(65);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("fromCodePoint"));
    }

    #[test]
    fn allows_code_point_at() {
        assert!(run("const c = str.codePointAt(0);").is_empty());
    }

    #[test]
    fn allows_from_code_point() {
        assert!(run("const s = String.fromCodePoint(65);").is_empty());
    }

    #[test]
    fn flags_multiple_on_same_line() {
        let d = run("const a = s.charCodeAt(0), b = s.charCodeAt(1);");
        assert_eq!(d.len(), 2);
    }
}
