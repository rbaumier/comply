//! prefer-type-over-interface backend — default to `type`, use `interface`
//! only when you need extension, declaration merging, or `implements`.
//!
//! Why: the skill rule is "types by default, interface only for extension/perf".
//! `type` supports unions, intersections, mapped types, and conditional
//! types — `interface` doesn't. Using `type` everywhere keeps the toolkit
//! uniform. `interface` is still fine when you need `extends` for structural
//! inheritance, `declare module` augmentation, or when a class `implements` it.
//!
//! Detection: walk `interface_declaration` nodes and flag those WITHOUT an
//! `extends_type_clause` child AND not used in an `implements` clause.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use std::collections::HashSet;

#[derive(Debug)]
pub struct Check;

/// Visit-time state. `implemented` may not be complete when an
/// `interface_declaration` is visited (a class declaring `implements I` may
/// appear after the interface), so candidates are buffered and resolved in
/// `finish` once the entire AST has been walked.
#[derive(Default)]
struct State {
    implemented: HashSet<String>,
    candidates: Vec<Candidate>,
}

struct Candidate {
    name: String,
    line: usize,
    column: usize,
}

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["interface"])
    }

    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["implements_clause", "interface_declaration"])
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(State::default()))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };
        if node.kind() == "implements_clause" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Ok(name) = child.utf8_text(source_bytes) {
                    // Handle both simple identifiers and generic types
                    let base_name = name.split('<').next().unwrap_or(name).trim();
                    if !base_name.is_empty() && base_name != "implements" {
                        state.implemented.insert(base_name.to_string());
                    }
                }
            }
            return;
        }
        // interface_declaration
        if has_extends_clause(node) {
            return;
        }
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source_bytes).ok())
            .unwrap_or("<interface>");

        let pos = node.start_position();
        state.candidates.push(Candidate {
            name: name.to_string(),
            line: pos.row + 1,
            column: pos.column + 1,
        });
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(state) = state.and_then(|s| s.downcast::<State>().ok()) else {
            return;
        };
        for c in &state.candidates {
            if state.implemented.contains(&c.name) {
                continue;
            }
            let name = &c.name;
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: c.line,
                column: c.column,
                rule_id: "prefer-type-over-interface".into(),
                message: format!(
                    "Interface '{name}' has no extends clause and is not implemented — use \
                     `type {name} = {{ ... }}` instead. Types support \
                     unions, intersections, mapped types, and conditional \
                     types. Keep `interface` for extension, declaration \
                     merging, and `implements` only."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn has_extends_clause(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|c| c.kind() == "extends_type_clause")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_plain_interface() {
        assert_eq!(run_on("interface User { name: string; }").len(), 1);
    }

    #[test]
    fn allows_interface_with_extends() {
        assert!(run_on("interface Admin extends User { role: string; }").is_empty());
    }

    #[test]
    fn allows_type_alias() {
        assert!(run_on("type User = { name: string };").is_empty());
    }

    #[test]
    fn allows_interface_with_implements() {
        let code = r#"
            interface Serializable { serialize(): string; }
            class User implements Serializable { serialize() { return ""; } }
        "#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_interface_with_generic_implements() {
        let code = r#"
            interface Repository<T> { find(id: string): T; }
            class UserRepo implements Repository<User> { find(id: string) { return null; } }
        "#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_interface_not_implemented() {
        let code = r#"
            interface Unused { foo: string; }
            class User implements OtherInterface {}
        "#;
        assert_eq!(run_on(code).len(), 1);
    }
}
