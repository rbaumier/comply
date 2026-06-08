//! rn-memo-list-items oxc backend — flag `renderItem={Component}` without memo.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXExpression;
use std::sync::Arc;

fn source_wraps_in_memo(source: &str, ident: &str) -> bool {
    let patterns = [
        format!("memo({ident})"),
        format!("React.memo({ident})"),
        format!("const {ident} = memo("),
        format!("const {ident} = React.memo("),
        format!("let {ident} = memo("),
        format!("var {ident} = memo("),
    ];
    patterns.iter().any(|p| crate::oxc_helpers::source_contains(source, p.as_str()))
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
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else { return };

        let oxc_ast::ast::JSXAttributeName::Identifier(name) = &attr.name else { return };
        if name.name.as_str() != "renderItem" {
            return;
        }

        // Value must be a JSX expression container: `renderItem={Ident}`.
        let Some(oxc_ast::ast::JSXAttributeValue::ExpressionContainer(container)) = &attr.value
        else {
            return;
        };

        let JSXExpression::Identifier(ident) = &container.expression else { return };
        let ident_name = ident.name.as_str();

        // Only flag PascalCase identifiers (component convention).
        if !ident_name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
        {
            return;
        }

        if source_wraps_in_memo(ctx.source, ident_name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, ident.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "List item component `{ident_name}` should be wrapped in `React.memo(...)`."
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
    fn flags_unmemoized_component() {
        let src = "function Row() { return null; }\nconst x = <FlatList renderItem={Row} />;";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_memo_wrapped() {
        let src = "const Row = memo(function Row() { return null; });\nconst x = <FlatList renderItem={Row} />;";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_react_memo_wrapped() {
        let src = "const Row = React.memo(RowImpl);\nconst x = <FlatList renderItem={Row} />;";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_non_pascal_reference() {
        // A camelCase identifier likely points to a stable callback, not a component.
        let src = "const x = <FlatList renderItem={renderRow} />;";
        assert!(run(src).is_empty());
    }
}
