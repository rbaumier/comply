//! zod-no-optional-and-default-together backend.
//!
//! Flags a call chain that combines `.optional()` and `.default()` on
//! the same schema, in either order. `.default(x)` already makes the
//! field effectively optional on input, so `.optional().default(x)` and
//! `.default(x).optional()` are either redundant or — worse — a subtle
//! semantic bug (the schema now accepts explicit `undefined` without
//! applying the default).

use crate::diagnostic::{Diagnostic, Severity};

/// Return the method name if `node` is a `call_expression` whose
/// function is a `member_expression` ending in `.<name>()`.
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
    let Some((method, object)) = method_call_name(node, source) else {
        return;
    };
    // We only care about seeing `.optional()` chained with `.default()`.
    if method != "optional" && method != "default" {
        return;
    }
    let other = if method == "optional" { "default" } else { "optional" };
    // The inner expression must be a call to the complementary method.
    let Some((inner_method, _)) = method_call_name(object, source) else {
        return;
    };
    if inner_method != other {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "zod-no-optional-and-default-together".into(),
        message: "`.optional()` and `.default()` on the same schema is redundant — \
                  `.default(x)` already handles missing input. Keep one: prefer \
                  `.default(x)` alone unless you specifically want `undefined` to \
                  bypass the default."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_optional_then_default() {
        assert_eq!(
            run_on("const s = z.string().optional().default('x');").len(),
            1
        );
    }

    #[test]
    fn flags_default_then_optional() {
        assert_eq!(
            run_on("const s = z.string().default('x').optional();").len(),
            1
        );
    }

    #[test]
    fn allows_default_alone() {
        assert!(run_on("const s = z.string().default('x');").is_empty());
    }

    #[test]
    fn allows_optional_alone() {
        assert!(run_on("const s = z.string().optional();").is_empty());
    }

    #[test]
    fn allows_optional_with_other_method_between() {
        // `.optional().nullable().default(x)` — not the pattern we flag,
        // only direct adjacency of the two methods.
        assert!(run_on("const s = z.string().optional().nullable();").is_empty());
    }
}
