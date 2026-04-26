//! no-misleading-collection-name backend — name suffix vs. type mismatch.
//!
//! For each `variable_declarator`, look at:
//! - the binding name's suffix (`*List`, `*Set`, `*Map`, `*Array`)
//! - the initializer's type (`new Set(...)`, `new Map(...)`, array literal,
//!   `[]`, etc.)
//!
//! Flag when the suffix and the actual type disagree. We deliberately
//! restrict to constructor calls and array literals — type annotations
//! alone are too easy to misread, and we want zero false positives.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["variable_declarator"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(name) = name_node.utf8_text(source) else {
            return;
        };
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
        let pos = name_node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-misleading-collection-name".into(),
            message: format!(
                "`{name}` is named like {claimed_article} {claimed_label} but holds \
                 {actual_article} {actual_label}. Rename to match the actual type — \
                 the suffix is part of the contract.",
                claimed_article = article(claimed.label()),
                claimed_label = claimed.label(),
                actual_article = article(actual.label()),
                actual_label = actual.label()
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Claimed vs. actual collection shape inferred from a binding name's
/// suffix and from its initializer expression. Kept as a closed enum so
/// we can exhaustively match on it in `name_suffix_shape` /
/// `initializer_shape`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Array,
    Set,
    Map,
}

impl Shape {
    fn label(self) -> &'static str {
        match self {
            Shape::Array => "Array",
            Shape::Set => "Set",
            Shape::Map => "Map",
        }
    }
}

/// English article ("a" / "an") for a label starting with a vowel sound.
fn article(label: &str) -> &'static str {
    match label.chars().next() {
        Some('A') | Some('E') | Some('I') | Some('O') | Some('U') => "an",
        _ => "a",
    }
}

/// Map a binding name's suffix to its claimed shape.
fn name_suffix_shape(name: &str) -> Option<Shape> {
    if name.ends_with("List") || name.ends_with("Array") {
        Some(Shape::Array)
    } else if name.ends_with("Set") {
        Some(Shape::Set)
    } else if name.ends_with("Map") {
        Some(Shape::Map)
    } else {
        None
    }
}

/// Inspect an initializer expression and return the actual shape, if it
/// can be determined statically. We only handle the unambiguous cases:
/// `new Set(...)`, `new Map(...)`, `new Array(...)`, and array literals.
fn initializer_shape(value: tree_sitter::Node, source: &[u8]) -> Option<Shape> {
    match value.kind() {
        "array" => Some(Shape::Array),
        "new_expression" => {
            let ctor = value.child_by_field_name("constructor")?;
            let ctor_name = ctor.utf8_text(source).ok()?;
            match ctor_name {
                "Set" => Some(Shape::Set),
                "Map" => Some(Shape::Map),
                "Array" => Some(Shape::Array),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source), &tree)
    }

    #[test]
    fn flags_list_holding_set() {
        let diags = run("const userList = new Set();");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("an Array"));
        assert!(diags[0].message.contains("a Set"));
    }

    #[test]
    fn flags_set_holding_array() {
        let diags = run("const userSet = [];");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_map_holding_set() {
        let diags = run("const cacheMap = new Set();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_matching_list_array() {
        assert!(run("const userList = [];").is_empty());
    }

    #[test]
    fn allows_matching_set_set() {
        assert!(run("const userSet = new Set();").is_empty());
    }

    #[test]
    fn allows_matching_map_map() {
        assert!(run("const cacheMap = new Map();").is_empty());
    }

    #[test]
    fn ignores_unsuffixed_name() {
        // No suffix, no claim, no diagnostic.
        assert!(run("const cache = new Set();").is_empty());
    }

    #[test]
    fn ignores_unknown_initializer() {
        // We can't tell what `getUsers()` returns, so we don't false-positive.
        assert!(run("const userList = getUsers();").is_empty());
    }
}
