//! no-nested-template-literal backend.
//!
//! Flags a template literal that contains ANOTHER template literal
//! inside one of its interpolations — i.e. `` `a ${`b`}` ``. Those are
//! hard to read and usually mean the inner expression deserves its
//! own named variable.
//!
//! Having multiple interpolations inside a single template is NOT
//! nesting — ``\`${foo}/api/${id}\``` stays clean. Earlier versions
//! counted `${` occurrences, which flagged that case incorrectly.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["template_string"] => |node, source, ctx, diagnostics|
    // Only inspect outer template literals — but run on every one so
    // we catch nesting however deep it goes. `_source` stays unused
    // because the AST check is purely structural.
    let _ = source;
    if !has_template_descendant(node) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-nested-template-literal".into(),
        message: "Nested template literal — extract the inner template to a named variable."
            .into(),
        severity: Severity::Error,
        span: None,
    });
}

/// True when any descendant of `node` is itself a `template_string`.
/// A direct child of `node` lives inside one of its interpolation
/// expressions (`template_substitution` wrapper) — a template literal
/// at that level is the "nested" case we want to flag.
fn has_template_descendant(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "template_string" {
            return true;
        }
        if has_template_descendant(child) {
            return true;
        }
    }
    false
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
    fn flags_nested() {
        assert_eq!(
            run(r#"const msg = `Hello ${user.name}, you have ${`${count} items`}`;"#).len(),
            1
        );
    }

    #[test]
    fn allows_single_interpolation() {
        assert!(run(r#"const msg = `Hello ${name}`;"#).is_empty());
    }

    #[test]
    fn allows_no_interpolation() {
        assert!(run(r#"const msg = `plain string`;"#).is_empty());
    }

    #[test]
    fn allows_multiple_interpolations_in_same_template() {
        // Regression: previously flagged because the check counted
        // `${` occurrences. Two interpolations in the same template
        // literal are not nesting — the tree-sitter AST has a single
        // `template_string` node with two `template_substitution`
        // children.
        assert!(
            run(r#"const url = baseUrl + `${baseUrl}/api/v1/subscriptions/${subscriptionId}`;"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_function_call_in_interpolation() {
        // A function call inside `${}` is not a nested template —
        // just an expression that happens to produce a string.
        assert!(run(r#"const msg = `id: ${String(id)}`;"#).is_empty());
    }
}
