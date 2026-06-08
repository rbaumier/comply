//! react-jsx-no-jsx-as-prop AST backend.
//!
//! Flags JSX attributes whose value is an expression wrapping a JSX element or
//! fragment, e.g. `<Comp icon={<Icon />} />` or `<Comp header={<><h1 /></>} />`.
//! A fresh element object is built each render, breaking memoization of `Comp`.

use crate::diagnostic::{Diagnostic, Severity};

const ALLOWED_PROPS: &[&str] = &[
    "trigger",
    "content",
    "icon",
    "overlay",
    "asChild",
    "fallback",
    "label",
    "description",
    "title",
    "action",
    "prefix",
    "suffix",
    "left",
    "right",
    "header",
    "footer",
];

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let Some(attr_name_early) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    if ALLOWED_PROPS.contains(&attr_name_early) { return; }
    let Some(value_node) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    if value_node.kind() != "jsx_expression" {
        return;
    }

    // Walk inside the `{...}` wrapper to find the actual expression node.
    let mut inner: Option<tree_sitter::Node> = None;
    let mut cursor = value_node.walk();
    for child in value_node.children(&mut cursor) {
        match child.kind() {
            "{" | "}" => continue,
            _ => { inner = Some(child); break; }
        }
    }
    let Some(expr) = inner else { return };

    let kind_label = match expr.kind() {
        "jsx_element" | "jsx_self_closing_element" => "JSX element",
        "jsx_fragment" => "JSX fragment",
        _ => return,
    };

    let attr_name = attr_name_early;
    let pos = expr.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "{kind_label} as value of JSX prop `{attr_name}` creates a new element every render — extract to a variable or `useMemo`."
        ),
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

    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_jsx_element_as_prop() {
        let src = "const x = <Comp sidebar={<Icon />} />;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_jsx_element_with_children_as_prop() {
        let src = "const x = <Comp banner={<h1>Title</h1>} />;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_jsx_fragment_as_prop() {
        let src = "const x = <Comp banner={<><h1>Title</h1></>} />;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_whitelisted_prop_names() {
        for prop in ["trigger", "content", "icon", "overlay", "header", "footer"] {
            let src = format!("const x = <Comp {prop}={{<Icon />}} />;");
            assert!(run_on(&src).is_empty(), "prop '{prop}' should be allowed");
        }
    }

    #[test]
    fn allows_identifier_prop() {
        let src = "const x = <Comp icon={icon} />;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_children_slot() {
        let src = "const x = <Comp>{children}</Comp>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_string_attribute() {
        let src = r#"const x = <div className="foo" />;"#;
        assert!(run_on(src).is_empty());
    }
}
