//! Flag `useSession({ password: '<short>' })` where `password` is a string
//! literal shorter than 32 characters. Env lookups (`process.env.X`,
//! `env.X`, identifiers) are fine.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["useSession"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let Ok(callee_text) = callee.utf8_text(source) else { return; };
    if !callee_text.ends_with("useSession") { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(options) = first_object_argument(args) else { return; };
    let Some(password_value) = find_pair_value(options, source, "password") else { return; };

    if !matches!(password_value.kind(), "string" | "template_string") {
        return;
    }
    let min_len = ctx.config.threshold("tanstack-start-session-secret-min-length", "min_length", ctx.lang);
    let Ok(text) = password_value.utf8_text(source) else { return; };
    let inner_len = text.trim_matches(|c| c == '"' || c == '\'' || c == '`').chars().count();
    if inner_len >= min_len { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &password_value,
        super::META.id,
        format!(
            "`useSession` password literal is only {inner_len} chars; must be \
             at least {min_len}. Prefer reading from an env var."
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
        if child.kind() != "pair" {
            continue;
        }
        let Some(k) = child.child_by_field_name("key") else {
            continue;
        };
        let Ok(raw) = k.utf8_text(source) else {
            continue;
        };
        let name = raw.trim_matches(|c| c == '"' || c == '\'');
        if name == key {
            return child.child_by_field_name("value");
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
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
