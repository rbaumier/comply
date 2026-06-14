//! OxcCheck backend for react-display-name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Display names are a React DevTools / Fast Refresh concern. Files for
        // non-React JSX frameworks (SolidJS, Vue, Preact, Qwik, Stencil) — whose
        // file-based routing uses anonymous `export default function` route
        // components — must not be flagged.
        if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
            return diagnostics;
        }

        for node in semantic.nodes().iter() {
            let AstKind::ExportDefaultDeclaration(export) = node.kind() else { continue };

            let anonymous_span = match &export.declaration {
                ExportDefaultDeclarationKind::ArrowFunctionExpression(arrow) => {
                    if contains_jsx(arrow.span, semantic) {
                        Some(arrow.span)
                    } else {
                        None
                    }
                }
                ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                    if func.id.is_some() {
                        None
                    } else if contains_jsx(func.span, semantic) {
                        Some(func.span)
                    } else {
                        None
                    }
                }
                ExportDefaultDeclarationKind::CallExpression(call) => {
                    if is_react_wrapper_call(call) {
                        call.arguments.first().and_then(|arg| {
                            let expr = arg.as_expression()?;
                            match expr {
                                Expression::ArrowFunctionExpression(arrow) => {
                                    if contains_jsx(arrow.span, semantic) {
                                        Some(arrow.span)
                                    } else {
                                        None
                                    }
                                }
                                Expression::FunctionExpression(func) => {
                                    if func.id.is_some() {
                                        None
                                    } else if contains_jsx(func.span, semantic) {
                                        Some(func.span)
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            }
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            };

            let Some(span) = anonymous_span else { continue };
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Anonymous React component missing a display name.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

fn is_react_wrapper_call(call: &CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => id.name == "memo" || id.name == "forwardRef",
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name == "React"
                    && (member.property.name == "memo" || member.property.name == "forwardRef")
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Check if any JSX node exists within the given span by scanning semantic nodes.
fn contains_jsx(outer: oxc_span::Span, semantic: &oxc_semantic::Semantic) -> bool {
    for node in semantic.nodes().iter() {
        if let AstKind::JSXOpeningElement(el) = node.kind() {
            if el.span.start >= outer.start && el.span.end <= outer.end {
                return true;
            }
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_anonymous_arrow_default_export_in_react_file() {
        let src = "import { useState } from 'react';\nexport default () => <div />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_solidjs_anonymous_default_export() {
        // SolidStart file-based routing: an anonymous `export default function`
        // route component in a file importing from `@solidjs/router`. Display
        // names are a React-only concern. (Closes #2218)
        let src = "import { RouteSectionProps } from \"@solidjs/router\";\n\
                   export default function (props: RouteSectionProps) {\n\
                       return <h1>Layout</h1>;\n\
                   }";
        assert!(run(src).is_empty(), "got unexpected diagnostics: {:?}", run(src));
    }

    #[test]
    fn skips_solid_js_anonymous_default_export() {
        // A route file importing from `solid-js` itself.
        let src = "import { createSignal } from \"solid-js\";\n\
                   export default function () { return <section>x</section>; }";
        assert!(run(src).is_empty(), "got unexpected diagnostics: {:?}", run(src));
    }

    #[test]
    fn still_flags_anonymous_default_export_in_react_file() {
        // Negative-space guard: a real React file (imports `react`, no Solid)
        // with an anonymous default-export component is still flagged.
        let src = "import { useState } from \"react\";\n\
                   export default function () { return <div />; }";
        assert_eq!(run(src).len(), 1);
    }
}
