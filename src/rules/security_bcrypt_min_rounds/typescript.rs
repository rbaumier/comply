//! security-bcrypt-min-rounds backend — flag bcrypt hashing with cost < 12.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    // Match `bcrypt.hash`, `bcrypt.hashSync`, and common aliases like `bcryptjs.hash`.
    let is_bcrypt_hash = matches!(
        name,
        "bcrypt.hash"
            | "bcrypt.hashSync"
            | "bcryptjs.hash"
            | "bcryptjs.hashSync"
    );
    if !is_bcrypt_hash {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    // Pull the positional arguments (skip "(", ",", ")").
    let mut cursor = args.walk();
    let positional: Vec<_> = args
        .children(&mut cursor)
        .filter(|c| !matches!(c.kind(), "(" | ")" | ","))
        .collect();
    let Some(cost_node) = positional.get(1) else {
        return;
    };
    if cost_node.kind() != "number" {
        return;
    }
    let Ok(text) = cost_node.utf8_text(source) else {
        return;
    };
    let Ok(value) = text.parse::<i64>() else {
        return;
    };
    if value >= 12 {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{name}` cost factor {value} is below 12 — use at least 12 to resist brute-force attacks."
        ),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_low_rounds_hash() {
        assert_eq!(run("bcrypt.hash(pw, 8);").len(), 1);
    }

    #[test]
    fn flags_low_rounds_hash_sync() {
        assert_eq!(run("bcrypt.hashSync(pw, 10);").len(), 1);
    }

    #[test]
    fn allows_sufficient_rounds() {
        assert!(run("bcrypt.hash(pw, 12);").is_empty());
    }

    #[test]
    fn allows_high_rounds() {
        assert!(run("bcrypt.hashSync(pw, 14);").is_empty());
    }

    #[test]
    fn ignores_unrelated_calls() {
        assert!(run("crypto.hash(pw, 8);").is_empty());
    }
}
