//! Flag `<TabsTrigger>` whose closest enclosing shadcn Tabs component
//! is `<Tabs>` itself rather than `<TabsList>`.
//!
//! We walk up the parent chain: the first ancestor whose tag is
//! `TabsList` / `Tabs.List` / `Tabs` / `Tabs.Root` decides the verdict.

use crate::diagnostic::{Diagnostic, Severity};

fn is_trigger_tag(tag: &str) -> bool {
    tag == "TabsTrigger" || tag == "Tabs.Trigger"
}

fn is_list_tag(tag: &str) -> bool {
    tag == "TabsList" || tag == "Tabs.List"
}

fn is_tabs_root_tag(tag: &str) -> bool {
    tag == "Tabs" || tag == "Tabs.Root"
}

fn jsx_element_tag<'a>(elem: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if elem.kind() != "jsx_element" {
        return None;
    }
    let open = elem.child_by_field_name("open_tag")?;
    crate::rules::jsx::jsx_element_tag_name(open, source)
}

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] prefilter = ["TabsTrigger", "Tabs.Trigger"] => |node, source, ctx, diagnostics|    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if !is_trigger_tag(tag) {
        return;
    }

    // Walk up: the `jsx_opening_element` sits inside a `jsx_element`;
    // its parent JSX element is the immediate wrapper, and so on.
    let mut current = node.parent();
    while let Some(parent) = current {
        if let Some(parent_tag) = jsx_element_tag(parent, source) {
            if is_list_tag(parent_tag) {
                return; // Correctly wrapped.
            }
            if is_tabs_root_tag(parent_tag) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    "`<TabsTrigger>` must be a descendant of `<TabsList>`, not a direct child of `<Tabs>`.".into(),
                    Severity::Error,
                ));
                return;
            }
        }
        current = parent.parent();
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_trigger_directly_in_tabs() {
        let src = r#"const x = <Tabs><TabsTrigger value="a">A</TabsTrigger></Tabs>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_trigger_in_list() {
        let src = r#"const x = <Tabs><TabsList><TabsTrigger value="a">A</TabsTrigger></TabsList></Tabs>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_trigger_in_dotted_list() {
        let src = r#"const x = <Tabs.Root><Tabs.List><Tabs.Trigger value="a">A</Tabs.Trigger></Tabs.List></Tabs.Root>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_trigger_outside_tabs() {
        // Standalone `<TabsTrigger>` — no Tabs ancestor, nothing to complain about here.
        let src = r#"const x = <TabsTrigger value="a">A</TabsTrigger>;"#;
        assert!(run(src).is_empty());
    }
}
