//! Flags `as_expression` nodes where the asserted type looks narrower than
//! the source. Heuristic: assertions to a literal type (`as 'foo'`,
//! `as 123`, `as true`), or to a concrete nominal type when the expression
//! is a plain identifier typed as a union (detected via presence of
//! `unknown`/`any` textual markers is NOT used — we focus on literal targets
//! which are the canonical narrowing-via-cast smell).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "as_expression" {
        return;
    }

    // Skip `as const` — that's a widening-to-literal assertion pattern.
    let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
    if text.trim_end().ends_with("as const") {
        return;
    }

    // The second named child is the target type (first is the expression).
    let Some(target) = node.named_child(1) else { return };
    let target_kind = target.kind();

    // Narrowing targets: literal types, predefined narrow types.
    let is_narrowing = matches!(
        target_kind,
        "literal_type" | "template_literal_type"
    );

    if !is_narrowing {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Avoid using `as` to narrow types; use a type predicate or `in`/`typeof` check.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_cast_to_string_literal() {
        let diags = run("const x = val as 'foo';");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_cast_to_number_literal() {
        let diags = run("const x = val as 42;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_as_const() {
        assert!(run("const x = [1, 2] as const;").is_empty());
    }

    #[test]
    fn allows_cast_to_regular_type() {
        assert!(run("const x = val as string;").is_empty());
    }
}
