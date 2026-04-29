use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["defineEmits"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") {
                continue;
            }
            if t.contains("defineEmits([") {
                diags.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: i + 1,
                    column: line.find("defineEmits").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message:
                        "Use `defineEmits<{ eventName: [arg: Type] }>()` instead of the untyped array form."
                            .into(),
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
    fn flags_array_form() {
        assert_eq!(run("const emit = defineEmits(['change', 'update'])").len(), 1);
    }
    #[test]
    fn allows_typed_form() {
        assert!(run("const emit = defineEmits<{ change: [value: string] }>()").is_empty());
    }
}
