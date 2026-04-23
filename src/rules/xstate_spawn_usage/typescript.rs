//! xstate-spawn-usage — flag `spawn(...)` calls that are not nested inside
//! an `assign(...)` call. In XState v5, `spawn` must only be invoked from
//! within an `assign` action so the spawned actor is tracked by the machine.

use crate::diagnostic::{Diagnostic, Severity};

/// Return true if `node` is a `call_expression` whose callee is the plain
/// identifier `assign`.
fn is_assign_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "identifier" {
        return false;
    }
    callee.utf8_text(source).unwrap_or("") == "assign"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }
    if callee.utf8_text(source).unwrap_or("") != "spawn" {
        return;
    }

    // Walk ancestors; if any is an `assign(...)` call, we're fine.
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if is_assign_call(ancestor, source) {
            return;
        }
        current = ancestor.parent();
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`spawn()` must be called inside an `assign()` action.".into(),
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
    fn flags_spawn_outside_assign() {
        let diags = run_on("const actor = spawn(childMachine);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_spawn_inside_unrelated_call() {
        let diags = run_on("doStuff(spawn(childMachine));");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_spawn_inside_assign() {
        assert!(run_on(
            "const action = assign({ ref: () => spawn(childMachine) });"
        )
        .is_empty());
    }

    #[test]
    fn allows_spawn_inside_assign_with_context_arg() {
        assert!(run_on(
            "const action = assign((ctx) => ({ ref: spawn(childMachine) }));"
        )
        .is_empty());
    }

    #[test]
    fn allows_no_spawn_call() {
        assert!(run_on("const x = foo(childMachine);").is_empty());
    }
}
