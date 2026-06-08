//! zod-refine-requires-path backend — flag `<obj>.refine(fn, opts)` calls
//! whose `opts` object carries a `message` but no `path` key. Without
//! `path: ['field']` the form error attaches to the whole object rather
//! than a specific field, which is the original Zod footgun.
//!
//! Only fires when the receiver chain contains `z.object(`. We look at
//! the receiver's source text rather than walking the chain, because the
//! object schema can be deeply nested and re-using a precise walker is
//! not worth the extra code.

use crate::diagnostic::{Diagnostic, Severity};

/// Return true if the options-object argument carries a `message:` pair
/// without a `path:` pair.
fn has_message_no_path(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "object" {
        return false;
    }
    let mut has_message = false;
    let mut has_path = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let Some(key) = child.child_by_field_name("key") else {
            continue;
        };
        let Ok(key_text) = key.utf8_text(source) else {
            continue;
        };
        let normalized = key_text.trim_matches(|c: char| c == '"' || c == '\'');
        match normalized {
            "message" => has_message = true,
            "path" => has_path = true,
            _ => {}
        }
    }
    has_message && !has_path
}

/// Return true if any descendant of `node` is a call to `z.object(...)`.
fn receiver_uses_z_object(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let text = node.utf8_text(source).unwrap_or("");
    text.contains("z.object(")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(function) = node.child_by_field_name("function") else { return };
    if function.kind() != "member_expression" { return; }

    let Some(property) = function.child_by_field_name("property") else { return };
    if property.utf8_text(source).ok() != Some("refine") { return; }

    // Only flag when the receiver chain involves `z.object(...)`.
    let Some(object) = function.child_by_field_name("object") else { return };
    if !receiver_uses_z_object(object, source) { return; }

    // Find the second positional argument (named children of `arguments`).
    let Some(arguments) = node.child_by_field_name("arguments") else { return };
    let mut named_args = Vec::new();
    let mut cursor = arguments.walk();
    for child in arguments.children(&mut cursor) {
        if child.is_named() {
            named_args.push(child);
        }
    }
    let Some(opts) = named_args.get(1).copied() else { return };

    if !has_message_no_path(opts, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Add `path: ['fieldName']` to `.refine()` options so form errors attach to the correct field.".into(),
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
    fn flags_refine_no_path() {
        assert_eq!(
            run("z.object({ a: z.string(), b: z.string() }).refine(d => d.a !== d.b, { message: 'Must differ' })").len(),
            1
        );
    }

    #[test]
    fn allows_refine_with_path() {
        assert!(run(
            "z.object({ a: z.string() }).refine(d => d.a.length > 0, { message: 'Required', path: ['a'] })"
        )
        .is_empty());
    }
}
