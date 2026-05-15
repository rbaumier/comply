//! i18n-enforce-message-id oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXElementName};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["FormattedMessage"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };
        let tag = match &opening.name {
            JSXElementName::Identifier(id) => id.name.as_str(),
            JSXElementName::IdentifierReference(id) => id.name.as_str(),
            _ => return,
        };
        if tag != "FormattedMessage" {
            return;
        }
        let has_id = opening.attributes.iter().any(|attr| {
            let JSXAttributeItem::Attribute(attr) = attr else { return false };
            let JSXAttributeName::Identifier(name) = &attr.name else { return false };
            name.name.as_str() == "id"
        });
        if has_id {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`<FormattedMessage>` requires an explicit `id` prop. Without \
                      it the build hashes on `defaultMessage`, which breaks on \
                      whitespace edits."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(src, &Check)
    }

    #[test]
    fn flags_formatted_message_without_id() {
        let src = r#"const x = <FormattedMessage defaultMessage="Hi" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_formatted_message_with_id() {
        let src = r#"const x = <FormattedMessage id="greet" defaultMessage="Hi" />;"#;
        assert!(run(src).is_empty());
    }
}
