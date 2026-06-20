// TextCheck is appropriate here: Vue template directives (v-for, :key) are
// HTML-like syntax, not parseable by tree-sitter-typescript. The engine returns
// None for Vue SFCs (see engine.rs), so TreeSitter backends are skipped entirely
// for .vue files.

//! vue-v-for-needs-stable-key text backend.
//!
//! Flags a `v-for` whose `:key` binds to the numeric loop index. The index is
//! identified by its position in the destructure, not just its name:
//! - array form `(item, index) in arr` — the 2nd alias is the index;
//! - object form `(value, key) in obj` — the 2nd alias is the property name
//!   (a stable id, the Vue-recommended key), and only a 3rd alias
//!   `(value, key, index) in obj` is the numeric index.
//!
//! So a 2nd alias is flagged only when its name is an index-style name
//! (`index`/`idx`/`i`/`j`), never when it is an object-key name like `key`.
//! Any 3rd alias is always the numeric index and is flagged regardless of name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Names that denote a numeric loop index when in the 2nd destructure
/// position (array iteration). `key` is excluded: as a 2nd alias it is an
/// object property name (stable), not an index.
const INDEX_NAMES: &[&str] = &["index", "idx", "i", "j"];

/// Returns the bare identifier inside `:key="..."` on the line, if any.
fn key_binding(line: &str) -> Option<&str> {
    let after = line.split(":key=\"").nth(1)?;
    let ident = after.split('"').next()?.trim();
    let is_bare = !ident.is_empty()
        && ident
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    is_bare.then_some(ident)
}

/// Parses the destructure aliases of a `v-for` expression on the line.
/// `v-for="(value, key, index) in obj"` -> `["value", "key", "index"]`;
/// `v-for="item in items"` -> `["item"]`.
fn v_for_aliases(line: &str) -> Vec<&str> {
    let Some(after) = line.split("v-for=").nth(1) else {
        return Vec::new();
    };
    let quote = match after.chars().next() {
        Some(c @ ('"' | '\'')) => c,
        _ => return Vec::new(),
    };
    let Some(expr) = after[1..].split(quote).next() else {
        return Vec::new();
    };
    // The left-hand side is everything before ` in ` / ` of `.
    let lhs = expr
        .split_once(" in ")
        .or_else(|| expr.split_once(" of "))
        .map_or(expr, |(lhs, _)| lhs);
    let lhs = lhs.trim().trim_start_matches('(').trim_end_matches(')');
    lhs.split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .collect()
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if !line.contains("v-for") {
                continue;
            }
            let Some(key) = key_binding(line) else {
                continue;
            };
            let aliases = v_for_aliases(line);
            let position = aliases.iter().position(|&a| a == key);
            let is_index = match position {
                // 2nd alias is an array index only when index-named.
                Some(1) => INDEX_NAMES.contains(&key),
                // 3rd alias is always the numeric index (object iteration).
                Some(2) => true,
                // 1st alias is the item/value; absent or any other position.
                _ => false,
            };
            if is_index {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "vue-v-for-needs-stable-key".into(),
                    message: format!(
                        "`:key=\"{key}\"` in `v-for` uses the loop index, not a \
                         stable id. When items reorder or get filtered, Vue reuses \
                         the wrong DOM. Use `:key=\"item.id\"` instead."
                    ),
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
    fn flags_index_key() {
        assert_eq!(
            run("<li v-for=\"(item, index) in items\" :key=\"index\">").len(),
            1
        );
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

    #[test]
    fn allows_object_key_alias() {
        // `key` is the 2nd alias of object iteration: a property name, stable.
        assert!(
            run("<div v-for=\"(value, key) in someObject\" :key=\"key\">").is_empty()
        );
    }

    #[test]
    fn allows_object_key_alias_with_index_present() {
        // `(value, key, index)`: keying off `key` (property name) is stable.
        assert!(
            run("<div v-for=\"(value, key, index) in obj\" :key=\"key\">").is_empty()
        );
    }

    #[test]
    fn flags_third_position_index() {
        // `(value, key, index)`: the 3rd alias is the numeric index.
        assert_eq!(
            run("<div v-for=\"(value, key, index) in obj\" :key=\"index\">").len(),
            1
        );
    }
}
