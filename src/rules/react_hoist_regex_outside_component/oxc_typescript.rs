//! react-hoist-regex-outside-component oxc backend for TSX.
//!
//! Flags regex literals inside React component bodies (PascalCase functions).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(regex) = node.kind() else {
            return;
        };
        if !inside_component_body(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, regex.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "react-hoist-regex-outside-component".into(),
            message: "Regex literal inside a component body is \
                      recompiled every render. Hoist to a module-level \
                      `const` so it compiles once."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Walk ancestors to find a PascalCase function (component convention).
fn inside_component_body<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(func) => {
                if let Some(ref id) = func.id
                    && starts_with_uppercase(id.name.as_str()) {
                        return true;
                    }
            }
            AstKind::VariableDeclarator(decl) => {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &decl.id
                    && starts_with_uppercase(ident.name.as_str()) {
                        // Only count if init is a function/arrow.
                        if let Some(init) = &decl.init
                            && matches!(
                                init.without_parentheses(),
                                oxc_ast::ast::Expression::FunctionExpression(_)
                                    | oxc_ast::ast::Expression::ArrowFunctionExpression(_)
                            ) {
                                return true;
                            }
                    }
            }
            _ => {}
        }
    }
    false
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_regex_in_component() {
        let source = "function Foo() { const r = /test/g; return null; }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_module_level_regex() {
        let source = "const r = /test/g; function Foo() { return null; }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_regex_in_non_component_function() {
        let source = "function helper() { const r = /test/g; return r; }";
        assert!(run_on(source).is_empty());
    }
}
