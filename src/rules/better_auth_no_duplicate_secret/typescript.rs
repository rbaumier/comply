//! better-auth-no-duplicate-secret — flag `secret` inside `betterAuth({ ... })`.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

fn find_pair_with_key<'a>(obj: Node<'a>, source: &[u8], key: &str) -> Option<Node<'a>> {
    let mut cursor = obj.walk();
    for child in obj.children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let Some(k) = child.child_by_field_name("key") else {
            continue;
        };
        let k_text = k
            .utf8_text(source)
            .unwrap_or("")
            .trim_matches(|c: char| c == '\'' || c == '"');
        if k_text == key {
            return Some(child);
        }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(func) = node.child_by_field_name("function") else { return };
    if func.utf8_text(source).unwrap_or("") != "betterAuth" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(obj) = args.children(&mut cursor).find(|c| c.kind() == "object") else { return };

    let Some(pair) = find_pair_with_key(obj, source, "secret") else { return };

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &pair,
        super::META.id,
        "`secret` duplicates `BETTER_AUTH_SECRET` — remove it and use the env var.".into(),
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
    fn flags_secret_in_config() {
        assert_eq!(run("betterAuth({ secret: \"abc\" })").len(), 1);
    }

    #[test]
    fn allows_config_without_secret() {
        assert!(run("betterAuth({ database: db })").is_empty());
    }

    #[test]
    fn ignores_secret_outside_betterauth() {
        assert!(run("otherFactory({ secret: \"abc\" })").is_empty());
    }
}
