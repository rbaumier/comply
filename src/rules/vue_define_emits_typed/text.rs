use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// `lang="ts"` SFCs are owned by `vue-typed-define-props-emits`, which flags the
/// runtime form for both `defineProps` and `defineEmits`. This rule defers there
/// and keeps only the non-TS SFCs, where the typed generic form is still wanted.
fn has_ts_script(source: &str) -> bool {
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("<script")
            && (trimmed.contains("lang=\"ts\"") || trimmed.contains("lang='ts'"))
        {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["defineEmits"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if has_ts_script(ctx.source) {
            return Vec::new();
        }
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
        assert_eq!(
            run("const emit = defineEmits(['change', 'update'])").len(),
            1
        );
    }
    #[test]
    fn flags_array_form_in_non_ts_sfc() {
        let sfc = "<script setup>\nconst emit = defineEmits(['change'])\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }
    #[test]
    fn allows_typed_form() {
        assert!(run("const emit = defineEmits<{ change: [value: string] }>()").is_empty());
    }
    #[test]
    fn defers_to_typed_rule_in_ts_sfc() {
        // `vue-typed-define-props-emits` owns the `lang="ts"` case; avoid
        // emitting a duplicate diagnostic for the same `defineEmits([...])`.
        let sfc = "<script lang=\"ts\" setup>\nconst emit = defineEmits(['pane'])\n</script>";
        assert!(run(sfc).is_empty());
    }
}
