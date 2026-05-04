use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ExportDefaultDeclarationKind;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportDefaultDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["export default"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ExportDefaultDeclaration(export) = node.kind() else {
            return;
        };
        let (is_anon, label) = match &export.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                let has_name = func.id.as_ref().is_some_and(|id| !id.name.is_empty());
                (!has_name, "function")
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                let has_name = class.id.as_ref().is_some_and(|id| !id.name.is_empty());
                (!has_name, "class")
            }
            _ => return,
        };
        if !is_anon {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, export.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Anonymous default export {label} — give it a name for \
                 better stack traces and refactoring support."
            ),
            severity: super::META.severity,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_anonymous_function() {
        let d = run_on("export default function() {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("function"));
    }

    #[test]
    fn flags_anonymous_class() {
        let d = run_on("export default class {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("class"));
    }

    #[test]
    fn allows_named_function() {
        assert!(run_on("export default function myFn() {}").is_empty());
    }

    #[test]
    fn allows_named_class() {
        assert!(run_on("export default class MyClass {}").is_empty());
    }

    #[test]
    fn allows_identifier_export() {
        assert!(run_on("export default myVariable;").is_empty());
    }
}
