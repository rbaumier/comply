//! no-sync-scripts oxc backend — flag `<script src>` without `async` or `defer`.

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
        Some(&["script"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        let tag = match &opening.name {
            JSXElementName::Identifier(ident) => ident.name.as_str(),
            _ => return,
        };
        if tag != "script" {
            return;
        }

        let mut has_src = false;
        let mut has_async = false;
        let mut has_defer = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else { continue };
            match name_ident.name.as_str() {
                "src" => has_src = true,
                "async" => has_async = true,
                "defer" => has_defer = true,
                _ => {}
            }
        }

        // Inline scripts (no src) are out of scope.
        if !has_src || has_async || has_defer {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`<script src>` blocks parsing — add `async` or `defer`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }


    #[test]
    fn flags_sync_external_script() {
        assert_eq!(run(r#"const x = <script src="a.js" />;"#).len(), 1);
    }


    #[test]
    fn allows_async_script() {
        assert!(run(r#"const x = <script src="a.js" async />;"#).is_empty());
    }


    #[test]
    fn allows_defer_script() {
        assert!(run(r#"const x = <script src="a.js" defer />;"#).is_empty());
    }


    #[test]
    fn allows_inline_script() {
        assert!(run(r#"const x = <script>{code}</script>;"#).is_empty());
    }


    #[test]
    fn ignores_non_script() {
        assert!(run(r#"const x = <div />;"#).is_empty());
    }
}
