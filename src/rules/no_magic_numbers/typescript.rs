//! no-magic-numbers backend — flag numeric literals that are neither
//! in the common-constants allowlist (-1, 0, 1, 2) nor bound to a named
//! `const` / class field / enum value.
//!
//! Keeps the heuristic small: we skip literals used inside type
//! annotations, object keys, enum member values, class-field or
//! variable declarators (the "extracted to a constant" shape), and
//! array indices `arr[0]` (ignore by default matches eslint's option).

use crate::diagnostic::{Diagnostic, Severity};

const ALLOWED: &[&str] = &["0", "1", "2", "-1"];

fn is_allowed_literal(text: &str) -> bool {
    let t = text.trim();
    ALLOWED.contains(&t)
}

fn is_in_skip_context(node: tree_sitter::Node) -> bool {
    // Only skip when the literal is the DIRECT child (the value) of a
    // naming construct. Walking up through `binary_expression`,
    // `call_expression`, etc. still counts as "magic".
    let Some(parent) = node.parent() else { return false };
    match parent.kind() {
        // Type position: `type X = 42` — literal IS the alias.
        "literal_type" => return true,
        // Enum: `enum E { A = 42 }`.
        "enum_assignment" => return true,
        // `const PORT = 8080;` — literal is being named directly.
        "variable_declarator" | "public_field_definition" => return true,
        // `{ timeout: 30 }` — keyed value is effectively named.
        "pair" => return true,
        // Unary minus: `-1` — recurse so `-1` is allowed via the literal check.
        "unary_expression" => {
            if let Some(gp) = parent.parent() {
                return matches!(
                    gp.kind(),
                    "literal_type"
                        | "enum_assignment"
                        | "variable_declarator"
                        | "public_field_definition"
                        | "pair"
                );
            }
        }
        _ => {}
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "number" {
        return;
    }
    let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
    if is_allowed_literal(text) {
        return;
    }
    if is_in_skip_context(node) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-magic-numbers".into(),
        message: format!(
            "Magic number `{text}` — extract it into a `const` with a descriptive name."
        ),
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
    fn flags_magic_number_in_expression() {
        assert_eq!(run_on("const ms = elapsed * 3600;").len(), 1);
    }

    #[test]
    fn flags_magic_in_return() {
        assert_eq!(run_on("function f() { return 8080; }").len(), 1);
    }

    #[test]
    fn flags_magic_in_arg() {
        assert_eq!(run_on("setTimeout(cb, 5000);").len(), 1);
    }

    #[test]
    fn allows_zero_one_minus_one() {
        assert!(run_on("const a = 0; const b = 1; const c = -1; const d = 2;").is_empty());
    }

    #[test]
    fn allows_named_const() {
        // The literal IS being named; that's the whole point.
        assert!(run_on("const PORT = 8080;").is_empty());
    }

    #[test]
    fn allows_literal_type() {
        assert!(run_on("type Port = 8080;").is_empty());
    }
}
