//! zod-transform-requires-pipe backend.
//!
//! Walks every `call_expression`. If the callee is a `member_expression`
//! whose `property` is `transform`, inspect the parent: when the
//! `.transform(...)` call is itself the object of a `member_expression`
//! whose `property` is `pipe`, we allow it. Anything else is flagged —
//! standalone `.transform(fn)` without a follow-up `.pipe(...)` yields
//! an un-validated value, which defeats Zod's purpose at the boundary.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.kind() != "member_expression" { return; }
    let Some(prop) = func.child_by_field_name("property") else { return; };
    if prop.utf8_text(source).unwrap_or("") != "transform" { return; }

    // Allowed iff node.parent is `member_expression` whose property is `pipe`
    // AND whose object is our call_expression (i.e. the next link in the chain
    // is `.pipe`). That parent member_expression will itself be wrapped in a
    // `call_expression` for `.pipe(schema)` — we don't need to check that far.
    if let Some(parent) = node.parent()
        && parent.kind() == "member_expression"
        && let Some(parent_prop) = parent.child_by_field_name("property")
        && parent_prop.utf8_text(source).unwrap_or("") == "pipe"
    {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`.transform()` output is not re-validated — chain `.pipe(z.*)` to assert the output schema.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_transform_without_pipe() {
        assert_eq!(run("const s = z.string().transform(s => s.trim());").len(), 1);
    }

    #[test]
    fn flags_standalone_transform_call() {
        assert_eq!(run("const s = schema.transform(fn);").len(), 1);
    }

    #[test]
    fn allows_transform_pipe_chain() {
        assert!(
            run("const s = z.string().transform(s => s.trim()).pipe(z.string().min(1));")
                .is_empty()
        );
    }

    #[test]
    fn allows_transform_pipe_simple() {
        assert!(run("const s = schema.transform(fn).pipe(other);").is_empty());
    }

    #[test]
    fn flags_transform_as_argument() {
        // `.transform(fn)` used as an argument (not followed by .pipe) still
        // produces an un-validated value — flag it.
        assert_eq!(run("doStuff(z.string().transform(s => s.trim()));").len(), 1);
    }
}
