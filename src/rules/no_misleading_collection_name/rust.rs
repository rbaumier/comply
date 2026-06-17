//! no-misleading-collection-name Rust backend.
//!
//! Flag variable names whose suffix disagrees with the initializer type.
//! E.g., `user_list` initialized with `HashSet::new()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug, PartialEq, Clone, Copy)]
enum Shape {
    Vec,
    Set,
    Map,
}

impl Shape {
    fn label(self) -> &'static str {
        match self {
            Shape::Vec => "Vec/array",
            Shape::Set => "Set",
            Shape::Map => "Map",
        }
    }
}

/// Map a binding name's trailing type word to its claimed shape.
///
/// `list` is a general collection term (`allow_list`, `deny_list`) that does
/// not promise a `Vec`, so it claims no shape. Only a trailing word naming a
/// specific backing type makes a contract: `array`/`vec`, `set`, `map`/`dict`.
/// The match is on the trailing token (exact equality), so `list_offset` claims
/// no shape (last token `offset`, not `set`).
fn name_suffix_shape(name: &str) -> Option<Shape> {
    match super::last_token(name).as_str() {
        "array" | "vec" => Some(Shape::Vec),
        "set" => Some(Shape::Set),
        "map" | "dict" => Some(Shape::Map),
        _ => None,
    }
}

fn initializer_shape(node: tree_sitter::Node, source: &[u8]) -> Option<Shape> {
    let text = node.utf8_text(source).unwrap_or("");
    if text.starts_with("Vec::") || text.starts_with("vec!") {
        Some(Shape::Vec)
    } else if text.starts_with("HashSet::") || text.starts_with("BTreeSet::") {
        Some(Shape::Set)
    } else if text.starts_with("HashMap::") || text.starts_with("BTreeMap::") {
        Some(Shape::Map)
    } else {
        None
    }
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["let_declaration"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        let Some(pat) = node.child_by_field_name("pattern") else {
            return;
        };
        let Ok(name) = pat.utf8_text(source) else {
            return;
        };
        // Strip `mut ` prefix.
        let name = name.strip_prefix("mut ").unwrap_or(name).trim();
        let Some(claimed) = name_suffix_shape(name) else {
            return;
        };
        let Some(value) = node.child_by_field_name("value") else {
            return;
        };
        let Some(actual) = initializer_shape(value, source) else {
            return;
        };
        if claimed == actual {
            return;
        }
        let pos = pat.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-misleading-collection-name".into(),
            message: format!(
                "`{name}` is named like a {} but holds a {}.",
                claimed.label(),
                actual.label()
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_vec_holding_set() {
        let src = "fn f() { let user_vec = HashSet::new(); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_matching_name() {
        let src = "fn f() { let user_set = HashSet::new(); }";
        assert!(run_on(src).is_empty());
    }

    // `list` is a general collection term, not a Vec contract.
    #[test]
    fn allows_list_holding_set() {
        let src = "fn f() { let allow_list = HashSet::new(); }";
        assert!(run_on(src).is_empty());
    }

    // Mid-word fragments must not read as a type token (issue #3953):
    // `offset` ends in `set`, `bitmap` ends in `map`, but neither names a Set/Map.
    #[test]
    fn allows_offset_holding_vec() {
        let src = "fn f(n: usize) { let mut list_offset = Vec::with_capacity(n); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bitmap_holding_vec() {
        let src = "fn f() { let bitmap = vec![]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_dataset_holding_map() {
        let src = "fn f() { let dataset = HashMap::new(); }";
        assert!(run_on(src).is_empty());
    }

    // A genuine trailing `set` token still flags a Vec mismatch.
    #[test]
    fn flags_set_token_holding_vec() {
        let src = "fn f() { let user_set = Vec::new(); }";
        assert_eq!(run_on(src).len(), 1);
    }
}
