//! Flag `throw new Error('...not found...')` anywhere — conservative: we do
//! not try to prove the surrounding function is a server fn, because the
//! signal (an Error message containing "not found") is specific enough.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "throw_statement" { return; }
    let Some(expr) = node.named_child(0) else { return; };
    if expr.kind() != "new_expression" { return; }
    let Some(ctor) = expr.child_by_field_name("constructor") else { return; };
    let Ok(ctor_name) = ctor.utf8_text(source) else { return; };
    if ctor_name != "Error" { return; }

    let Some(args) = expr.child_by_field_name("arguments") else { return; };
    let mut cursor = args.walk();
    let has_notfound_msg = args.children(&mut cursor).any(|c| {
        if !matches!(c.kind(), "string" | "template_string") { return false; }
        c.utf8_text(source)
            .ok()
            .map(|s| s.to_ascii_lowercase().contains("not found"))
            .unwrap_or(false)
    });
    if !has_notfound_msg { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Throw `notFound()` instead of `new Error('...not found...')` so the \
         router can render the 404 boundary."
            .into(),
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
    fn flags_not_found_error() {
        assert_eq!(
            run("if (!user) { throw new Error('user not found'); }").len(),
            1
        );
    }

    #[test]
    fn flags_case_insensitive() {
        assert_eq!(run("throw new Error('Not Found');").len(), 1);
    }

    #[test]
    fn allows_notfound_helper() {
        assert!(run("if (!user) { throw notFound(); }").is_empty());
    }

    #[test]
    fn allows_unrelated_error() {
        assert!(run("throw new Error('permission denied');").is_empty());
    }
}
