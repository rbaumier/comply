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

    // Narrowing targets: literal types, template literal types, PascalCase
    // user-defined types, and generic utility types like `NonNullable<T>`.
    if !target_is_narrowing(target, source) {
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

/// Generic utility types from the standard library that produce narrower
/// types than their input. Casts like `value as NonNullable<T>` are typically
/// narrowing and would be better expressed via a runtime check.
const NARROWING_UTILITY_TYPES: &[&str] = &[
    "NonNullable",
    "Exclude",
    "Extract",
    "Required",
    "Readonly",
    "Pick",
    "Capitalize",
    "Uncapitalize",
    "Uppercase",
    "Lowercase",
];

fn target_is_narrowing(target: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    match target.kind() {
        "literal_type" | "template_literal_type" => true,
        // `as TypeName` — flag PascalCase identifiers (likely user-defined
        // narrowing types), allow lowercase aliases (e.g. type aliases for
        // primitives) which are widening or neutral.
        "type_identifier" => {
            let Ok(name) = target.utf8_text(source) else { return false; };
            name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        }
        // `as NonNullable<T>` / `as Exclude<T, U>` / `as Pick<T, K>`.
        "generic_type" => {
            let Some(name_node) = target.child_by_field_name("name") else { return false; };
            let Ok(name) = name_node.utf8_text(source) else { return false; };
            NARROWING_UTILITY_TYPES.contains(&name)
        }
        _ => false,
    }
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

    #[test]
    fn flags_cast_to_pascal_case_type() {
        assert_eq!(run("const x = value as AdminUser;").len(), 1);
    }

    #[test]
    fn flags_cast_to_non_nullable() {
        assert_eq!(run("const x = value as NonNullable<T>;").len(), 1);
    }

    #[test]
    fn flags_cast_to_exclude() {
        assert_eq!(run("const x = value as Exclude<T, null>;").len(), 1);
    }

    #[test]
    fn allows_cast_to_any() {
        assert!(run("const x = value as any;").is_empty());
    }

    #[test]
    fn allows_cast_to_unknown() {
        assert!(run("const x = value as unknown;").is_empty());
    }

    #[test]
    fn allows_cast_to_lowercase_alias() {
        assert!(run("const x = value as myAlias;").is_empty());
    }
}
