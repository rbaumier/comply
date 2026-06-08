//! zod-no-optional-nullable-chain backend — detect the redundant pairing
//! of `.optional()` and `.nullable()` in either order. Both orderings
//! collapse to the built-in `.nullish()`.
//!
//! Walks `call_expression` nodes. For each call we check whether the
//! callee is `.optional` or `.nullable` and whether the receiver is
//! itself a call to the complementary method.

use crate::diagnostic::{Diagnostic, Severity};

/// Return `(method_name, receiver_node)` if `node` is a `call_expression`
/// whose function is a `member_expression`.
fn method_call_name<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<(&'a str, tree_sitter::Node<'a>)> {
    if node.kind() != "call_expression" {
        return None;
    }
    let function = node.child_by_field_name("function")?;
    if function.kind() != "member_expression" {
        return None;
    }
    let property = function.child_by_field_name("property")?;
    let object = function.child_by_field_name("object")?;
    let name = property.utf8_text(source).ok()?;
    Some((name, object))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((method, object)) = method_call_name(node, source) else { return };
    if method != "optional" && method != "nullable" {
        return;
    }
    let other = if method == "optional" { "nullable" } else { "optional" };
    let Some((inner_method, _)) = method_call_name(object, source) else { return };
    if inner_method != other {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Replace `.optional().nullable()` with `.nullish()` for clearer intent.".into(),
        Severity::Warning,
    ));
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
    fn flags_optional_nullable() {
        assert_eq!(run("z.string().optional().nullable()").len(), 1);
    }

    #[test]
    fn flags_nullable_optional() {
        assert_eq!(run("z.string().nullable().optional()").len(), 1);
    }

    #[test]
    fn allows_nullish() {
        assert!(run("z.string().nullish()").is_empty());
    }
}
