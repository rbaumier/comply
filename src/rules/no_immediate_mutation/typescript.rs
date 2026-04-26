//! no-immediate-mutation backend — flag patterns where a variable is declared
//! with an array/object/Set/Map literal and immediately mutated on the next
//! statement.
//!
//! Examples flagged:
//!   const arr = [3, 1, 2]; arr.sort();
//!   const arr = []; arr.push(1);
//!   const obj = {}; obj.foo = 'bar';
//!   const set = new Set(); set.add(1);
//!   const map = new Map(); map.set('a', 1);

use crate::diagnostic::{Diagnostic, Severity};

/// Extract text from a tree-sitter node.
fn text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

/// Mutating methods on arrays that indicate immediate mutation.
const ARRAY_MUTATORS: &[&str] = &[
    "push",
    "unshift",
    "pop",
    "shift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
];

/// Check whether a value node is an array literal (`[]` or `[...]`).
fn is_array_literal(node: tree_sitter::Node) -> bool {
    node.kind() == "array"
}

/// Check whether a value node is an object literal (`{}` or `{...}`).
fn is_object_literal(node: tree_sitter::Node) -> bool {
    node.kind() == "object"
}

/// Check whether a value node is `new Set(...)`, `new Map(...)`, etc.
fn is_new_collection(node: tree_sitter::Node, source: &[u8]) -> Option<&'static str> {
    if node.kind() != "new_expression" {
        return None;
    }
    let callee = node.child_by_field_name("constructor")?;
    let name = text(callee, source);
    match name {
        "Set" | "WeakSet" => Some("Set"),
        "Map" | "WeakMap" => Some("Map"),
        _ => None,
    }
}

/// Get the variable name from a `variable_declarator` node.
fn get_declared_name<'a>(declarator: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let name_node = declarator.child_by_field_name("name")?;
    if name_node.kind() != "identifier" {
        return None;
    }
    Some(text(name_node, source))
}

/// Find the next sibling statement of a given node's parent declaration.
fn next_statement(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    // The declarator's parent is the variable_declaration.
    let decl = node.parent()?;
    if decl.kind() != "variable_declaration" && decl.kind() != "lexical_declaration" {
        return None;
    }
    decl.next_named_sibling()
}

/// Check if the next statement is `varName.method(...)` where method is a
/// mutating method, and return the method name.
fn is_method_call_on<'a>(
    stmt: tree_sitter::Node<'a>,
    var_name: &str,
    methods: &[&str],
    source: &'a [u8],
) -> bool {
    if stmt.kind() != "expression_statement" {
        return false;
    }
    let expr = stmt.named_child(0);
    let expr = match expr {
        Some(e) if e.kind() == "call_expression" => e,
        _ => return false,
    };
    let callee = match expr.child_by_field_name("function") {
        Some(c) if c.kind() == "member_expression" => c,
        _ => return false,
    };
    let obj = callee.child_by_field_name("object");
    let prop = callee.child_by_field_name("property");
    match (obj, prop) {
        (Some(o), Some(p)) => text(o, source) == var_name && methods.contains(&text(p, source)),
        _ => false,
    }
}

/// Check if the next statement is `varName.prop = value` (property assignment).
fn is_property_assignment(stmt: tree_sitter::Node, var_name: &str, source: &[u8]) -> bool {
    if stmt.kind() != "expression_statement" {
        return false;
    }
    let expr = match stmt.named_child(0) {
        Some(e) if e.kind() == "assignment_expression" => e,
        _ => return false,
    };
    let left = match expr.child_by_field_name("left") {
        Some(l) if l.kind() == "member_expression" => l,
        Some(l) if l.kind() == "subscript_expression" => l,
        _ => return false,
    };
    let obj = left.child_by_field_name("object");
    match obj {
        Some(o) => text(o, source) == var_name,
        None => false,
    }
}

crate::ast_check! { on ["variable_declarator"] => |node, source, ctx, diagnostics|
    let Some(var_name) = get_declared_name(node, source) else { return };
    let Some(value) = node.child_by_field_name("value") else { return };

    let Some(next) = next_statement(node) else { return };

    // Determine what kind of literal was assigned and what mutation to look for.
    let flagged = if is_array_literal(value) {
        // Array: check for mutating method calls or property assignment.
        is_method_call_on(next, var_name, ARRAY_MUTATORS, source)
            || is_property_assignment(next, var_name, source)
    } else if is_object_literal(value) {
        // Object: check for property assignment or Object.assign().
        is_property_assignment(next, var_name, source)
    } else if let Some(collection_type) = is_new_collection(value, source) {
        // Set/Map: check for .add()/.set()
        match collection_type {
            "Set" => is_method_call_on(next, var_name, &["add"], source),
            "Map" => is_method_call_on(next, var_name, &["set"], source),
            _ => false,
        }
    } else {
        false
    };

    if !flagged {
        return;
    }

    let pos = next.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-immediate-mutation".into(),
        message: "Immediate mutation after variable assignment \u{2014} chain onto the initialiser instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_array_sort() {
        let d = run_on("const arr = [3, 1, 2];\narr.sort();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-immediate-mutation");
    }

    #[test]
    fn flags_array_push() {
        let d = run_on("const arr = [];\narr.push(1);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_array_reverse() {
        let d = run_on("const arr = [1, 2, 3];\narr.reverse();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_object_property_assignment() {
        let d = run_on("const obj = {};\nobj.foo = 'bar';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_set_add() {
        let d = run_on("const s = new Set();\ns.add(1);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_map_set() {
        let d = run_on("const m = new Map();\nm.set('a', 1);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_non_immediate_mutation() {
        // If there's a statement between declaration and mutation, it's fine.
        assert!(run_on("const arr = [3, 1, 2];\nconsole.log('hi');\narr.sort();").is_empty());
    }

    #[test]
    fn allows_chained_init() {
        // Already chained — no issue.
        assert!(run_on("const arr = [3, 1, 2].sort();").is_empty());
    }

    #[test]
    fn allows_non_literal_init() {
        // If init is not a literal, don't flag.
        assert!(run_on("const arr = getItems();\narr.sort();").is_empty());
    }
}
