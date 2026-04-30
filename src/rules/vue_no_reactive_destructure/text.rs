// TextCheck is appropriate here: while this checks a JS/TS pattern, the engine
// returns None for Vue SFCs (see engine.rs) — TreeSitter backends are skipped
// entirely for .vue files. Migrating to AST would silently disable this rule.

//! vue-no-reactive-destructure text backend.
//!
//! Detects `const { ... } = reactive(...)` which silently breaks
//! reactivity. The destructured values are plain copies — mutations
//! to them don't trigger Vue's reactivity system.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            // Pattern: `const { ... } = reactive(` or `let { ... } = reactive(`
            if (trimmed.starts_with("const {") || trimmed.starts_with("let {"))
                && trimmed.contains("= reactive(")
            {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "vue-no-reactive-destructure".into(),
                    message: "Destructuring `reactive()` breaks reactivity — the \
                              variables become plain copies. Use `toRefs(state)` to \
                              keep them connected, or use `ref()` directly."
                        .into(),
                    severity: Severity::Error,
                    span: None,
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
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source))
    }

    #[test]
    fn flags_const_destructure_reactive() {
        assert_eq!(run("const { count } = reactive({ count: 0 })").len(), 1);
    }

    #[test]
    fn flags_let_destructure_reactive() {
        assert_eq!(
            run("let { name, age } = reactive({ name: '', age: 0 })").len(),
            1
        );
    }

    #[test]
    fn allows_torefs() {
        assert!(run("const { count } = toRefs(state)").is_empty());
    }

    #[test]
    fn allows_ref() {
        assert!(run("const count = ref(0)").is_empty());
    }

    #[test]
    fn allows_non_destructured_reactive() {
        assert!(run("const state = reactive({ count: 0 })").is_empty());
    }
}
