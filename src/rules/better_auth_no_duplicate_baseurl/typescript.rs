//! better-auth-no-duplicate-baseurl — flag `baseURL` inside `betterAuth({ ... })`.

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

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.utf8_text(source).unwrap_or("") != "betterAuth" {
        return;
    }

    // Only flag when the file actually references BETTER_AUTH_URL — otherwise
    // there's no "duplication" to warn about.
    if !ctx.source.contains("BETTER_AUTH_URL") {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(obj) = args.children(&mut cursor).find(|c| c.kind() == "object") else { return };

    let Some(pair) = find_pair_with_key(obj, source, "baseURL") else { return };

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &pair,
        super::META.id,
        "`baseURL` duplicates `BETTER_AUTH_URL` — remove it and use the env var.".into(),
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
    fn flags_baseurl_in_config() {
        let src = "const url = process.env.BETTER_AUTH_URL;\nbetterAuth({ baseURL: \"https://app.example.com\" })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_config_without_baseurl() {
        assert!(run("betterAuth({ database: db })").is_empty());
    }

    #[test]
    fn ignores_baseurl_outside_betterauth() {
        let src = "const url = process.env.BETTER_AUTH_URL;\nmakeClient({ baseURL: \"https://app.example.com\" })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_baseurl_when_no_env_var_referenced() {
        assert!(run("betterAuth({ baseURL: \"https://app.example.com\" })").is_empty());
    }
}
