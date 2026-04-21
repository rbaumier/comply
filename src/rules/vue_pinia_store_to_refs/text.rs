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
            if t.starts_with("const {")
                && t.contains("= use")
                && t.contains("Store()")
                && !t.contains("storeToRefs(")
            {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Wrap the store in `storeToRefs()` when destructuring to preserve reactivity.".into(),
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
    fn flags_destructure_without_store_to_refs() {
        assert_eq!(run("const { count, name } = useCounterStore()").len(), 1);
    }
    #[test]
    fn allows_store_to_refs() {
        assert!(run("const { count } = storeToRefs(useCounterStore())").is_empty());
    }
    #[test]
    fn allows_no_destructure() {
        assert!(run("const store = useCounterStore()").is_empty());
    }
}
