use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXElementName, JSXExpression};
use oxc_span::GetSpan;
use std::sync::Arc;

const RN_LIST_COMPONENTS: &[&str] = &[
    "FlatList",
    "SectionList",
    "FlashList",
    "VirtualizedList",
    "SwipeListView",
];

fn jsx_tag_name_str<'a>(opening: &'a oxc_ast::ast::JSXOpeningElement<'a>) -> Option<String> {
    match &opening.name {
        JSXElementName::Identifier(id) => Some(id.name.to_string()),
        JSXElementName::IdentifierReference(id) => Some(id.name.to_string()),
        JSXElementName::MemberExpression(member) => {
            Some(member.property.name.to_string())
        }
        _ => None,
    }
}

fn is_rn_list_tag(tag: &str) -> bool {
    RN_LIST_COMPONENTS.iter().any(|c| tag.ends_with(c))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["renderItem"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else { return };
        let oxc_ast::ast::JSXAttributeName::Identifier(name_ident) = &attr.name else { return };
        if name_ident.name.as_str() != "renderItem" {
            return;
        }

        // Check parent opening element is a known RN list component.
        let mut is_list = false;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if let AstKind::JSXOpeningElement(opening) = ancestor.kind() {
                if let Some(tag) = jsx_tag_name_str(opening) {
                    is_list = is_rn_list_tag(&tag);
                }
                break;
            }
        }
        if !is_list {
            return;
        }

        // Value must be a JSX expression container with an inline function.
        let Some(oxc_ast::ast::JSXAttributeValue::ExpressionContainer(container)) = &attr.value
        else {
            return;
        };

        let span = match &container.expression {
            JSXExpression::ArrowFunctionExpression(f) => f.span(),
            JSXExpression::FunctionExpression(f) => f.span(),
            _ => return,
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Inline function in `renderItem` creates a new reference every render — extract to a stable component or `useCallback`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_inline_arrow() {
        let src = "const x = <FlatList renderItem={({ item }) => <Row item={item} />} />;";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_inline_function_expression() {
        let src = "const x = <FlatList renderItem={function ({ item }) { return null; }} />;";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_extracted_handler() {
        let src = "const x = <FlatList renderItem={renderRow} />;";
        assert!(run(src).is_empty());
    }


    #[test]
    fn flags_inline_arrow_flashlist() {
        let src = "const x = <FlashList renderItem={({ item }) => <Row />} />;";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_custom_component() {
        let src = "const x = <CustomRenderer renderItem={() => <View />} />;";
        assert!(run(src).is_empty());
    }
}
