//! no-mutation backend — flag property assignments on `const` bindings.
//!
//! Pattern: `obj.prop = value` or `obj[key] = value` (or any compound
//! assignment like `+=`) where `obj` is a binding declared with `const`
//! somewhere up the scope chain.
//!
//! Scope resolution here is deliberately lightweight: we walk up from
//! the assignment looking for any ancestor that textually contains
//! `const <name>` as a variable declarator. We stop at the first match
//! — no shadowing logic. This keeps the rule cheap and catches the
//! overwhelming majority of real-world cases (top-level or block-local
//! `const`); a pathological shadowed-`let`-inside-`const` case would
//! still be reported, which is fine since the outer `const` is also
//! being indirectly mutated.

use crate::diagnostic::{Diagnostic, Severity};

/// Walk down the LHS of an assignment to find the root identifier of
/// a member/subscript chain. Returns `None` if the LHS is a plain
/// identifier (that's a reassignment, not a property mutation) or an
/// unsupported shape (destructuring pattern, etc).
fn root_identifier_of_member_chain<'a>(
    mut node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<&'a str> {
    // The LHS must be a member/subscript access — a plain identifier
    // means reassignment, which this rule doesn't handle.
    if node.kind() != "member_expression" && node.kind() != "subscript_expression" {
        return None;
    }
    // Walk left-most object until we hit an identifier.
    loop {
        match node.kind() {
            "member_expression" | "subscript_expression" => {
                node = node.child_by_field_name("object")?;
            }
            "identifier" => {
                return node.utf8_text(source).ok();
            }
            _ => return None,
        }
    }
}

/// Return true when any ancestor of `node` declares `name` via `const`.
///
/// We look for a `lexical_declaration` whose first child token is
/// `const` and that contains a `variable_declarator` whose `name` is
/// the target identifier. That matches simple cases and `const { a } =
/// ...` destructuring where `a` appears as an identifier inside the
/// declarator's pattern.
fn declared_as_const(start: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut ancestor = start.parent();
    while let Some(scope) = ancestor {
        let mut cursor = scope.walk();
        for child in scope.named_children(&mut cursor) {
            if is_const_decl_of(child, source, name) {
                return true;
            }
            // `export const x = ...` wraps the lexical_declaration.
            if child.kind() == "export_statement"
                && let Some(decl) = child.child_by_field_name("declaration")
                && is_const_decl_of(decl, source, name)
            {
                return true;
            }
        }
        ancestor = scope.parent();
    }
    false
}

fn is_const_decl_of(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    if node.kind() != "lexical_declaration" {
        return false;
    }
    let Some(kw) = node.child(0) else { return false };
    if kw.utf8_text(source).unwrap_or("") != "const" {
        return false;
    }
    // Scan each variable_declarator for the name — covers both
    // `const x = ...` and `const { x } = ...` / `const [x] = ...`.
    let mut cursor = node.walk();
    for decl in node.named_children(&mut cursor) {
        if decl.kind() != "variable_declarator" {
            continue;
        }
        let Some(pat) = decl.child_by_field_name("name") else { continue };
        if pattern_binds(pat, source, name) {
            return true;
        }
    }
    false
}

/// True when the destructuring (or identifier) pattern introduces a
/// binding named `name`. Recurses into object/array patterns.
fn pattern_binds(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    match node.kind() {
        "identifier" => node.utf8_text(source).unwrap_or("") == name,
        _ => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .any(|c| pattern_binds(c, source, name))
        }
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "assignment_expression" && node.kind() != "augmented_assignment_expression" {
        return;
    }
    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(root) = root_identifier_of_member_chain(left, source) else { return };
    if !declared_as_const(node, source, root) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-mutation",
        format!(
            "Mutating a property of `{root}` (declared with `const`) — build a new value and rebind instead of mutating."
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_property_mutation_on_const() {
        let d = run_on("const obj = {}; obj.prop = 1;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_subscript_mutation_on_const() {
        assert_eq!(run_on("const obj = {}; obj['prop'] = 1;").len(), 1);
    }

    #[test]
    fn flags_compound_assignment_on_const() {
        assert_eq!(run_on("const c = { n: 0 }; c.n += 1;").len(), 1);
    }

    #[test]
    fn flags_nested_member_on_const() {
        assert_eq!(run_on("const a = { b: { c: 0 } }; a.b.c = 1;").len(), 1);
    }

    #[test]
    fn flags_mutation_on_exported_const() {
        assert_eq!(run_on("export const obj = {}; obj.x = 1;").len(), 1);
    }

    #[test]
    fn allows_mutation_on_let_binding() {
        // `let` isn't this rule's target — no-let covers reassignment concerns.
        assert!(run_on("let obj = {}; obj.prop = 1;").is_empty());
    }

    #[test]
    fn allows_plain_reassignment() {
        // Reassigning a `let` identifier (no member access) is out of scope.
        assert!(run_on("let x = 1; x = 2;").is_empty());
    }

    #[test]
    fn allows_mutation_on_unknown_binding() {
        // When we can't see the declaration (e.g. function parameter), we don't flag.
        assert!(run_on("function f(obj: { x: number }) { obj.x = 1; }").is_empty());
    }

    #[test]
    fn allows_new_object_via_spread() {
        assert!(run_on("const obj = { a: 1 }; const next = { ...obj, b: 2 };").is_empty());
    }
}
