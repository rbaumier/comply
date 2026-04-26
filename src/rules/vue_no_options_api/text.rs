// TextCheck is appropriate here: while these are JS/TS patterns, the engine
// returns None for Vue SFCs (see engine.rs) — TreeSitter backends are skipped
// entirely for .vue files. Migrating to AST would silently disable this rule.
// The text-based pre-filter (<script setup>, export default {) works correctly.

//! vue-no-options-api text backend.
//!
//! Detects `export default {` inside a `<script>` block (NOT `<script setup>`)
//! followed by Options API markers: `data()`, `methods`, `computed`,
//! `watch`, `mounted`, `created`. A file with `<script setup>` is exempt.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const OPTIONS_MARKERS: &[&str] = &[
    "data()", "data ()", "methods:", "methods :", "computed:", "computed :",
    "watch:", "watch :", "mounted()", "mounted ()", "created()", "created ()",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // If the file uses `<script setup`, it's Composition API — pass.
        if ctx.source.contains("<script setup") {
            return Vec::new();
        }
        // Must have `export default {` inside a `<script>` block.
        if !ctx.source.contains("export default {") && !ctx.source.contains("export default{") {
            return Vec::new();
        }
        // Look for any Options API marker.
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            for &marker in OPTIONS_MARKERS {
                if trimmed.contains(marker) {
                    return vec![Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: 1,
                        rule_id: "vue-no-options-api".into(),
                        message: format!(
                            "Options API detected (`{marker}`) — use `<script setup lang=\"ts\">` \
                             with Composition API instead. Options API is legacy in Vue 3."
                        ),
                        severity: Severity::Error,
                        span: None,
                    }];
                }
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source))
    }

    #[test]
    fn flags_options_api() {
        let source = "<script>\nexport default {\n  data() { return { x: 1 } }\n}\n</script>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_script_setup() {
        let source = "<script setup lang=\"ts\">\nconst x = ref(1)\n</script>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_non_vue_export() {
        let source = "const x = 1;";
        assert!(run(source).is_empty());
    }
}
