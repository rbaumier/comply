//! ui-symmetric-initial-exit OXC backend — compare `initial` and `exit`
//! property keys on `motion.*` JSX components.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName,
    JSXExpression, JSXMemberExpressionObject, ObjectPropertyKind,
};
use std::collections::BTreeSet;
use std::sync::Arc;

pub struct Check;

fn jsx_tag_name(name: &JSXElementName) -> Option<String> {
    match name {
        JSXElementName::Identifier(id) => Some(id.name.to_string()),
        JSXElementName::IdentifierReference(id) => Some(id.name.to_string()),
        JSXElementName::MemberExpression(member) => {
            let obj = match &member.object {
                JSXMemberExpressionObject::IdentifierReference(id) => id.name.to_string(),
                JSXMemberExpressionObject::MemberExpression(_) => return None,
                JSXMemberExpressionObject::ThisExpression(_) => return None,
            };
            Some(format!("{}.{}", obj, member.property.name))
        }
        _ => None,
    }
}

fn extract_object_keys_from_obj(obj: &oxc_ast::ast::ObjectExpression) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
        let key_name = match &p.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        keys.insert(key_name.to_string());
    }
    keys
}

fn get_attr_object_keys(attrs: &oxc_allocator::Vec<'_, JSXAttributeItem<'_>>, name: &str) -> Option<BTreeSet<String>> {
    for item in attrs {
        let JSXAttributeItem::Attribute(attr) = item else { continue };
        let JSXAttributeName::Identifier(id) = &attr.name else { continue };
        if id.name.as_str() != name {
            continue;
        }
        // Value should be JSXExpressionContainer with an object expression
        let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else { continue };
        let JSXExpression::ObjectExpression(obj) = &container.expression else { continue };
        let keys = extract_object_keys_from_obj(obj);
        if !keys.is_empty() {
            return Some(keys);
        }
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["motion."])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        let Some(tag) = jsx_tag_name(&opening.name) else { return };
        if !tag.starts_with("motion.") {
            return;
        }

        let initial_keys = get_attr_object_keys(&opening.attributes, "initial");
        let exit_keys = get_attr_object_keys(&opening.attributes, "exit");

        let (Some(init), Some(ex)) = (initial_keys, exit_keys) else { return };
        if init == ex {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "<{tag}> `initial` keys {init:?} don't match `exit` keys {ex:?} \u{2014} enter and exit won't mirror."
            ),
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
    fn flags_mismatched_keys() {
        let src = r#"
            const x = <motion.div
                initial={{ opacity: 0, y: 10 }}
                exit={{ opacity: 0 }}
            />;
        "#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_matching_keys() {
        let src = r#"
            const x = <motion.div
                initial={{ opacity: 0, y: 10 }}
                exit={{ opacity: 0, y: 10 }}
            />;
        "#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_without_exit() {
        let src = r#"
            const x = <motion.div initial={{ opacity: 0 }} />;
        "#;
        assert!(run(src).is_empty());
    }
}
