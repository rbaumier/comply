use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("onMounted") || !src.contains("addEventListener(") {
            return vec![];
        }
        if src.contains("removeEventListener(") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("addEventListener(") && !line.trim().starts_with("//") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`addEventListener` in `onMounted` without `removeEventListener` in `onUnmounted` leaks listeners.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }
    #[test]
    fn flags_no_remove() {
        assert_eq!(
            run("onMounted(() => { window.addEventListener('resize', handler) })").len(),
            1
        );
    }
    #[test]
    fn allows_with_remove() {
        assert!(
            run("onMounted(() => { window.addEventListener('resize', h) })\nonUnmounted(() => { window.removeEventListener('resize', h) })").is_empty()
        );
    }
}
