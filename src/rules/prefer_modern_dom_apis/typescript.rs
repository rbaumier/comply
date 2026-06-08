//! prefer-modern-dom-apis — flag legacy DOM mutation methods that have
//! modern replacements (`insertBefore` → `before`, `replaceChild` →
//! `replaceWith`).
//!
//! Detection: walk `call_expression` nodes whose callee is a
//! `member_expression` and whose property name is one of the legacy DOM
//! method names.

use crate::diagnostic::{Diagnostic, Severity};

const PATTERNS: &[(&str, &str)] = &[
    (
        "insertBefore",
        "Prefer `ref.before(newNode)` over `parent.insertBefore(newNode, ref)`.",
    ),
    (
        "replaceChild",
        "Prefer `old.replaceWith(newNode)` over `parent.replaceChild(newNode, old)`.",
    ),
];

crate::ast_check! { on ["call_expression"] prefilter = ["insertBefore", "replaceChild"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let Some(name) = prop.utf8_text(source).ok() else { return };
    let Some((_, message)) = PATTERNS.iter().find(|(p, _)| *p == name) else {
        return;
    };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "prefer-modern-dom-apis",
        (*message).into(),
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

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_insert_before() {
        let d = crate::rules::test_helpers::run_rule(&Check, "parent.insertBefore(newNode, refNode);", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("before"));
    }

    #[test]
    fn flags_replace_child() {
        let d = crate::rules::test_helpers::run_rule(&Check, "parent.replaceChild(newEl, oldEl);", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("replaceWith"));
    }

    #[test]
    fn allows_modern_before() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "refNode.before(newNode);", "t.ts").is_empty());
    }

    #[test]
    fn allows_modern_replace_with() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "oldEl.replaceWith(newEl);", "t.ts").is_empty());
    }

    #[test]
    fn ignores_comment() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "// parent.insertBefore(a, b)", "t.ts").is_empty());
    }
}
