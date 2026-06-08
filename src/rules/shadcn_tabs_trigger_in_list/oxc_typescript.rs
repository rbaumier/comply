use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXElementName;
use std::sync::Arc;

fn jsx_tag_name<'a>(opening: &'a oxc_ast::ast::JSXOpeningElement<'a>) -> Option<String> {
    match &opening.name {
        JSXElementName::Identifier(id) => Some(id.name.to_string()),
        JSXElementName::IdentifierReference(id) => Some(id.name.to_string()),
        JSXElementName::MemberExpression(member) => {
            let obj = match &member.object {
                oxc_ast::ast::JSXMemberExpressionObject::IdentifierReference(id) => {
                    id.name.to_string()
                }
                oxc_ast::ast::JSXMemberExpressionObject::MemberExpression(m) => {
                    m.property.name.to_string()
                }
                _ => return None,
            };
            Some(format!("{}.{}", obj, member.property.name))
        }
        _ => None,
    }
}

fn is_trigger_tag(tag: &str) -> bool {
    tag == "TabsTrigger" || tag == "Tabs.Trigger"
}

fn is_list_tag(tag: &str) -> bool {
    tag == "TabsList" || tag == "Tabs.List"
}

fn is_tabs_root_tag(tag: &str) -> bool {
    tag == "Tabs" || tag == "Tabs.Root"
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["TabsTrigger", "Tabs.Trigger"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };
        let Some(tag) = jsx_tag_name(opening) else { return };
        if !is_trigger_tag(&tag) {
            return;
        }

        // Walk ancestors looking for TabsList or Tabs root.
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if let AstKind::JSXOpeningElement(parent_opening) = ancestor.kind()
                && let Some(parent_tag) = jsx_tag_name(parent_opening) {
                    if is_list_tag(&parent_tag) {
                        return; // Correctly wrapped.
                    }
                    if is_tabs_root_tag(&parent_tag) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "`<TabsTrigger>` must be a descendant of `<TabsList>`, not a direct child of `<Tabs>`.".into(),
                            severity: Severity::Error,
                            span: None,
                        });
                        return;
                    }
                }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
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
