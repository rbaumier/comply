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

fn name_suffix_shape(name: &str) -> Option<Shape> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with("list") || lower.ends_with("array") || lower.ends_with("vec") {
        Some(Shape::Vec)
    } else if lower.ends_with("set") {
        Some(Shape::Set)
    } else if lower.ends_with("map") || lower.ends_with("dict") {
        Some(Shape::Map)
    } else {
        None
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
        let Some(pat) = node.child_by_field_name("pattern") else { return };
        let Ok(name) = pat.utf8_text(source) else { return };
        // Strip `mut ` prefix.
        let name = name.strip_prefix("mut ").unwrap_or(name).trim();
        let Some(claimed) = name_suffix_shape(name) else { return };
        let Some(value) = node.child_by_field_name("value") else { return };
        let Some(actual) = initializer_shape(value, source) else { return };
        if claimed == actual {
            return;
        }
        let pos = pat.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_list_holding_set() {
        let src = "fn f() { let user_list = HashSet::new(); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_matching_name() {
        let src = "fn f() { let user_set = HashSet::new(); }";
        assert!(run_on(src).is_empty());
    }
}
