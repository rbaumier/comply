//! zod-require-error-messages backend — flag `.refine(fn)` calls that
//! omit the second argument carrying the error message.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(function) = node.child_by_field_name("function") else { return };
    if function.kind() != "member_expression" { return; }

    let Some(property) = function.child_by_field_name("property") else { return };
    if property.utf8_text(source).ok() != Some("refine") { return; }

    let Some(arguments) = node.child_by_field_name("arguments") else { return };

    // Count top-level argument children (skip punctuation).
    let mut arg_count = 0usize;
    let mut cursor = arguments.walk();
    for child in arguments.children(&mut cursor) {
        if child.is_named() {
            arg_count += 1;
        }
    }
    if arg_count >= 2 { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Add `{ error: '...' }` to `.refine()` — bare refine produces no helpful error message.".into(),
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
    fn flags_single_arg_refine() {
        assert_eq!(run("z.string().refine(val => val.includes('@'))").len(), 1);
    }

    #[test]
    fn allows_refine_with_message() {
        assert!(
            run("z.string().refine(val => val.includes('@'), { message: 'Must be email' })")
                .is_empty()
        );
    }
}
