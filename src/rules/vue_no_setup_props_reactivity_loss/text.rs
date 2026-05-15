//! vue-no-setup-props-reactivity-loss text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("defineProps") {
            return Vec::new();
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }
            // Look for `const { … } = defineProps` (any whitespace, no `:` to
            // skip the optional type-annotation case).
            let Some(eq_idx) = trimmed.find('=') else { continue };
            let lhs = trimmed[..eq_idx].trim_end();
            let rhs = trimmed[eq_idx + 1..].trim_start();
            if !rhs.starts_with("defineProps") {
                continue;
            }
            let starts_with_kw = lhs.starts_with("const ") || lhs.starts_with("let ");
            if !starts_with_kw {
                continue;
            }
            // The binding pattern starts after the keyword.
            let after_kw = lhs.split_once(' ').map(|(_, r)| r.trim_start()).unwrap_or("");
            if !after_kw.starts_with('{') {
                continue;
            }
            diags.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: i + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Destructuring `defineProps()` strips reactivity — keep the \
                          object and read `props.foo`."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("App.vue"), src))
    }

    #[test]
    fn flags_destructured_defineprops() {
        let src = "const { foo } = defineProps<{ foo: string }>();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_object_defineprops() {
        let src = "const props = defineProps<{ foo: string }>();";
        assert!(run(src).is_empty());
    }
}
