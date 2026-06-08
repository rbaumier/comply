//! Flags `xs.flatMap(...).filter(...)` chains — two passes that can be
//! collapsed into a single `flatMap` callback returning `[]` for excluded
//! items.

use crate::diagnostic::{Diagnostic, Severity};

fn member_property_name<'a>(callee: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if callee.kind() != "member_expression" {
        return None;
    }
    let prop = callee.child_by_field_name("property")?;
    prop.utf8_text(source).ok()
}

crate::ast_check! { on ["call_expression"] prefilter = ["flatMap"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if member_property_name(callee, source) != Some("filter") {
        return;
    }

    let Some(receiver) = callee.child_by_field_name("object") else { return };
    if receiver.kind() != "call_expression" {
        return;
    }
    let Some(inner_callee) = receiver.child_by_field_name("function") else { return };
    if member_property_name(inner_callee, source) != Some("flatMap") {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "`.flatMap().filter()` iterates twice — return `[]` from the `flatMap` \
                  callback to filter and transform in a single pass.".into(),
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
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_flatmap_filter_chain() {
        let diags = run(r#"const r = xs.flatMap(x => x.children).filter(c => c.active);"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_flatmap_filter_boolean() {
        let diags = run(r#"const r = xs.flatMap(x => x.tags).filter(Boolean);"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_flatmap_filter_multiline() {
        let diags = run(r#"
const result = items
    .flatMap(item => item.children)
    .filter(child => child.visible);
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_flatmap_alone() {
        assert!(run(r#"const r = xs.flatMap(x => x.children);"#).is_empty());
    }

    #[test]
    fn allows_filter_alone() {
        assert!(run(r#"const r = xs.filter(x => x.active);"#).is_empty());
    }

    #[test]
    fn allows_map_filter_chain() {
        assert!(run(r#"const r = xs.map(x => x.id).filter(Boolean);"#).is_empty());
    }
}
