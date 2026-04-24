//! Flag `useSession({ password: '<short>' })` where `password` is a string
//! literal shorter than 32 characters. Env lookups (`process.env.X`,
//! `env.X`, identifiers) are fine.

use crate::diagnostic::{Diagnostic, Severity};

const MIN_LEN: usize = 32;

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let Ok(callee_text) = callee.utf8_text(source) else { return; };
    if !callee_text.ends_with("useSession") { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(options) = first_object_argument(args) else { return; };
    let Some(password_value) = find_pair_value(options, source, "password") else { return; };

    if !matches!(password_value.kind(), "string" | "template_string") {
        return;
    }
    let Ok(text) = password_value.utf8_text(source) else { return; };
    let inner_len = text.trim_matches(|c| c == '"' || c == '\'' || c == '`').chars().count();
    if inner_len >= MIN_LEN { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &password_value,
        super::META.id,
        format!(
            "`useSession` password literal is only {inner_len} chars; must be \
             at least {MIN_LEN}. Prefer reading from an env var."
        ),
        Severity::Warning,
    ));
}

fn first_object_argument<'a>(args: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = args.walk();
    args.children(&mut cursor).find(|c| c.kind() == "object")
}

fn find_pair_value<'a>(
    object: tree_sitter::Node<'a>,
    source: &[u8],
    key: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = object.walk();
    for child in object.children(&mut cursor) {
        if child.kind() != "pair" { continue; }
        let Some(k) = child.child_by_field_name("key") else { continue; };
        let Ok(raw) = k.utf8_text(source) else { continue; };
        let name = raw.trim_matches(|c| c == '"' || c == '\'');
        if name == key {
            return child.child_by_field_name("value");
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_short_literal() {
        assert_eq!(run("useSession({ password: 'too-short' });").len(), 1);
    }

    #[test]
    fn allows_long_literal() {
        assert!(
            run("useSession({ password: 'abcdefghijklmnopqrstuvwxyz0123456789' });").is_empty()
        );
    }

    #[test]
    fn allows_env_var() {
        assert!(run("useSession({ password: process.env.SECRET });").is_empty());
    }
}
