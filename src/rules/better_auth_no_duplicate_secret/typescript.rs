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

crate::ast_check! { on ["call_expression"] prefilter = ["BETTER_AUTH_SECRET"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.utf8_text(source).unwrap_or("") != "betterAuth" {
        return;
    }

    // Only flag when the file actually references BETTER_AUTH_SECRET — otherwise
    // there's no "duplication" to warn about.
    if !ctx.source.contains("BETTER_AUTH_SECRET") {
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
        let src = "const s = process.env.BETTER_AUTH_SECRET;\nbetterAuth({ secret: \"abc\" })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_config_without_secret() {
        assert!(run("betterAuth({ database: db })").is_empty());
    }

    #[test]
    fn ignores_secret_outside_betterauth() {
        let src = "const s = process.env.BETTER_AUTH_SECRET;\notherFactory({ secret: \"abc\" })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_secret_when_no_env_var_referenced() {
        assert!(run("betterAuth({ secret: \"abc\" })").is_empty());
    }
}
