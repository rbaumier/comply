//! security-bcrypt-min-rounds backend — flag bcrypt hashing with cost < 12.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
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
    let value: i64 = if cost_node.kind() == "number" {
        let Ok(text) = cost_node.utf8_text(source) else { return };
        let Ok(v) = text.parse::<i64>() else { return };
        v
    } else if cost_node.kind() == "identifier" {
        let Some(v) = resolve_const_number(*cost_node, source) else { return };
        v
    } else {
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

/// Walk up to the program root, then search for `const <ident> = <number>`.
/// Returns `Some(value)` only when the identifier resolves to a numeric literal
/// in a const declaration. Returns `None` for anything else (let, dynamic, etc.).
fn resolve_const_number(ident: tree_sitter::Node, source: &[u8]) -> Option<i64> {
    let ident_text = ident.utf8_text(source).ok()?;
    // Walk up to root.
    let mut root = ident;
    while let Some(p) = root.parent() {
        root = p;
    }
    find_const_number(root, source, ident_text)
}

fn find_const_number(node: tree_sitter::Node, source: &[u8], name: &str) -> Option<i64> {
    if node.kind() == "lexical_declaration" {
        let text = node.utf8_text(source).unwrap_or("");
        if !text.starts_with("const") {
            return None;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                let Some(name_node) = child.child_by_field_name("name") else {
                    continue;
                };
                let Some(value_node) = child.child_by_field_name("value") else {
                    continue;
                };
                if name_node.utf8_text(source).ok()? == name && value_node.kind() == "number" {
                    return value_node.utf8_text(source).ok()?.parse::<i64>().ok();
                }
            }
        }
        return None;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(v) = find_const_number(child, source, name) {
            return Some(v);
        }
    }
    None
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    #[test]
    fn flags_low_rounds_via_const() {
        assert_eq!(
            run("const SALT_ROUNDS = 8;\nbcrypt.hash(pw, SALT_ROUNDS);").len(),
            1
        );
    }

    #[test]
    fn allows_sufficient_rounds_via_const() {
        assert!(run("const SALT_ROUNDS = 12;\nbcrypt.hash(pw, SALT_ROUNDS);").is_empty());
    }

    #[test]
    fn ignores_dynamic_identifier() {
        assert!(run("bcrypt.hash(pw, rounds);").is_empty());
    }
}
