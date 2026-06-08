//! no-interpolation-in-snapshots backend — flag `toMatchSnapshot` /
//! `toMatchInlineSnapshot` calls receiving a template literal with
//! interpolation. Interpolated values defeat snapshot stability: a
//! snapshot is meant to be a verbatim, byte-stable expectation, so
//! embedding runtime values turns the snapshot into a tautology.

use crate::diagnostic::{Diagnostic, Severity};

const SNAPSHOT_MATCHERS: &[&str] = &["toMatchSnapshot", "toMatchInlineSnapshot"];

/// True if a `template_string` node has at least one `template_substitution`
/// child — i.e. actual interpolation, not just a plain `` `literal` ``.
fn template_has_interpolation(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|c| c.kind() == "template_substitution")
}

crate::ast_check! { on ["call_expression"] prefilter = ["toMatchSnapshot", "toMatchInlineSnapshot"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(property) = callee.child_by_field_name("property") else { return };
    let Ok(name) = property.utf8_text(source) else { return };
    if !SNAPSHOT_MATCHERS.contains(&name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    for arg in args.named_children(&mut cursor) {
        if arg.kind() == "template_string" && template_has_interpolation(arg) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &arg,
                "no-interpolation-in-snapshots",
                "Do not use template literal interpolation in snapshot matchers.".into(),
                Severity::Warning,
            ));
        }
    }
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
    fn flags_interpolation_in_to_match_snapshot() {
        assert_eq!(
            run_on("expect(x).toMatchSnapshot(`hello ${name}`)").len(),
            1
        );
    }

    #[test]
    fn flags_interpolation_in_to_match_inline_snapshot() {
        assert_eq!(
            run_on("expect(x).toMatchInlineSnapshot(`value is ${v}`)").len(),
            1
        );
    }

    #[test]
    fn allows_plain_template_literal() {
        assert!(run_on("expect(x).toMatchSnapshot(`hello world`)").is_empty());
    }

    #[test]
    fn allows_plain_string_argument() {
        assert!(run_on("expect(x).toMatchSnapshot('hello')").is_empty());
    }

    #[test]
    fn allows_no_arguments() {
        assert!(run_on("expect(x).toMatchSnapshot()").is_empty());
    }

    #[test]
    fn ignores_unrelated_matcher() {
        assert!(run_on("expect(x).toEqual(`hello ${name}`)").is_empty());
    }
}
