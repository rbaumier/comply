use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") {
                continue;
            }
            if !(t.contains("t('") || t.contains("t(\"")) {
                continue;
            }
            if !t.contains(" + ") {
                continue;
            }
            if let Some(col) = line.find(" + ") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "Don't concatenate `t()` results — use interpolation variables in the translation string instead.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }
    #[test]
    fn flags_concat() {
        assert_eq!(run("const msg = t('hello') + ' ' + name").len(), 1);
    }
    #[test]
    fn allows_interpolation() {
        assert!(run("const msg = t('greeting', { name })").is_empty());
    }
}
