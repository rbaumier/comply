use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut template_line: Option<usize> = None;
        let mut script_line: Option<usize> = None;
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("<template") && template_line.is_none() {
                template_line = Some(i);
            }
            if t.starts_with("<script") && script_line.is_none() {
                script_line = Some(i);
            }
        }
        match (template_line, script_line) {
            (Some(tl), Some(sl)) if tl < sl => {
                vec![Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: tl + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`<template>` appears before `<script>` — the canonical SFC order is: script → template → style.".into(),
                    severity: Severity::Warning,
                    span: None,
                }]
            }
            _ => vec![],
        }
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
    fn flags_template_before_script() {
        assert_eq!(
            run("<template><div /></template>\n<script setup lang=\"ts\">\n</script>").len(),
            1
        );
    }
    #[test]
    fn allows_script_before_template() {
        assert!(
            run("<script setup lang=\"ts\">\n</script>\n<template><div /></template>").is_empty()
        );
    }
}
