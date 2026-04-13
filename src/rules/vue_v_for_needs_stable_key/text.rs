// TextCheck is appropriate here: Vue template directives (v-for, :key) are
// HTML-like syntax, not parseable by tree-sitter-typescript. The engine returns
// None for Vue SFCs (see engine.rs), so TreeSitter backends are skipped entirely
// for .vue files.

//! vue-v-for-needs-stable-key text backend.
//!
//! Scans for lines that have both `v-for` and `:key="index"` or
//! `:key="i"` (common loop variable names). The heuristic is simple:
//! if the key expression is a bare identifier matching the second
//! parameter of the `v-for` destructure `(item, index)`, it's an
//! index-based key.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const INDEX_NAMES: &[&str] = &["index", "idx", "i", "j", "key"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if !line.contains("v-for") {
                continue;
            }
            for &name in INDEX_NAMES {
                let pattern = format!(":key=\"{name}\"");
                if line.contains(&pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "vue-v-for-needs-stable-key".into(),
                        message: format!(
                            "`:key=\"{name}\"` in `v-for` uses the loop index, not a \
                             stable id. When items reorder or get filtered, Vue reuses \
                             the wrong DOM. Use `:key=\"item.id\"` instead."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                    break;
                }
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
    fn flags_index_key() {
        assert_eq!(run("<li v-for=\"(item, index) in items\" :key=\"index\">").len(), 1);
    }

    #[test]
    fn flags_i_key() {
        assert_eq!(run("<li v-for=\"(item, i) in items\" :key=\"i\">").len(), 1);
    }

    #[test]
    fn allows_stable_key() {
        assert!(run("<li v-for=\"item in items\" :key=\"item.id\">").is_empty());
    }

    #[test]
    fn ignores_non_vfor_lines() {
        assert!(run(":key=\"index\"").is_empty());
    }
}
