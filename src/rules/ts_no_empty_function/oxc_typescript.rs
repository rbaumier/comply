//! ts-no-empty-function OxcCheck backend.
//!
//! Flag functions/methods with empty bodies. A body that contains a comment is
//! treated as non-empty (the comment is the "intentionally empty" signal).
//! Dependency-injection constructors — whose parameters carry an accessibility
//! modifier, `readonly`, or a decorator — are exempt: the parameters are the work.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::FunctionBody;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// Returns true when the function expression sits in a JSX expression container
/// or as an argument to a call/new expression (including parenthesized).
fn is_placeholder_callback_position(
    nodes: &oxc_semantic::AstNodes,
    node_id: oxc_semantic::NodeId,
) -> bool {
    let parent_id = nodes.parent_id(node_id);
    if parent_id == node_id {
        return false;
    }
    match nodes.kind(parent_id) {
        AstKind::JSXExpressionContainer(_) => true,
        AstKind::CallExpression(call) => {
            let node_span = nodes.kind(node_id).span();
            call.arguments.iter().any(|arg| arg.span() == node_span)
        }
        AstKind::NewExpression(new_expr) => {
            let node_span = nodes.kind(node_id).span();
            new_expr.arguments.iter().any(|arg| arg.span() == node_span)
        }
        AstKind::ParenthesizedExpression(_) => {
            let grandparent_id = nodes.parent_id(parent_id);
            if grandparent_id == parent_id {
                return false;
            }
            matches!(
                nodes.kind(grandparent_id),
                AstKind::CallExpression(_)
                    | AstKind::NewExpression(_)
                    | AstKind::JSXExpressionContainer(_)
            )
        }
        _ => false,
    }
}

/// Returns true when the function body is empty: no statements, no directives,
/// and no comment between the braces. A comment is the explicit "intentionally
/// empty" signal, so a comment-bearing body is treated as non-empty.
fn is_empty_body(body: &FunctionBody, semantic: &oxc_semantic::Semantic) -> bool {
    if !body.statements.is_empty() || !body.directives.is_empty() {
        return false;
    }
    let start = body.span.start;
    let end = body.span.end;
    !semantic
        .comments()
        .iter()
        .any(|comment| comment.span.start >= start && comment.span.end <= end)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (body_opt, span, is_method) = match node.kind() {
            AstKind::Function(func) => {
                // Check if this is a constructor with parameter properties
                // by looking at parent for MethodDefinition context.
                let parent = semantic.nodes().parent_node(node.id());
                let is_method = matches!(parent.kind(), AstKind::MethodDefinition(_));
                (func.body.as_ref(), func.span, is_method)
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                (Some(&arrow.body), arrow.span, false)
            }
            _ => return,
        };

        let Some(body) = body_opt else { return };

        // Arrow functions with expression bodies (no block) are never empty.
        if matches!(node.kind(), AstKind::ArrowFunctionExpression(arrow) if arrow.expression) {
            return;
        }

        if !is_empty_body(body, semantic) {
            return;
        }

        // Dual-read: the unit-test harness injects an empty default FileCtx, so
        // `in_test_dir` is false in tests — fall back to the local check, which
        // also covers the `_test.` infix that `in_test_dir` does not.
        if (ctx.file.path_segments.in_test_dir || is_test_file(ctx.path))
            && is_placeholder_callback_position(semantic.nodes(), node.id())
        {
            return;
        }

        // Skip dependency-injection constructors: a constructor whose parameters
        // carry an accessibility modifier (`private`/`public`/`protected`),
        // `readonly`, or a decorator (e.g. `@Inject(...)`) is a parameter-property
        // constructor — the parameters ARE the work, not an empty body.
        if is_method
            && let AstKind::MethodDefinition(method) = semantic.nodes().parent_node(node.id()).kind()
            && method.key.is_specific_id("constructor")
            && let AstKind::Function(func) = node.kind()
            && func.params.items.iter().any(|param| {
                param.accessibility.is_some() || param.readonly || !param.decorators.is_empty()
            })
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected empty function.".into(),
            severity: Severity::Warning,
            span: None,
        });
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn allows_empty_arrow_in_jsx_prop_in_test_file() {
        let src = r#"
            const x = <Foo onClose={() => {}} />;
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn allows_empty_function_expression_in_jsx_prop_in_test_file() {
        let src = r#"
            const x = <Foo onClose={function () {}} />;
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn allows_empty_arrow_as_call_argument_in_test_file() {
        let src = r#"
            useEffect(() => {}, []);
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn allows_parenthesized_empty_arrow_as_call_argument_in_test_file() {
        // Regression: useEffect((() => {}), []) — ParenthesizedExpression parent
        // must not fall through to the `_ => false` arm.
        let src = r#"
            useEffect((() => {}), []);
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn flags_empty_arrow_in_variable_assignment_in_test_file() {
        // Negative control: direct assignment is not a placeholder callback position.
        let src = r#"
            const handler = () => {};
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_named_function_declaration_in_test_file() {
        let src = r#"
            function doNothing() {}
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_arrow_in_jsx_prop_in_non_test_file() {
        let src = r#"
            const x = <Foo onClose={() => {}} />;
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_di_constructor_with_decorated_param() {
        // NestJS: a decorated DI parameter is the constructor's purpose.
        let src = r#"
            export class HelperService {
                constructor(@Inject(REQUEST) request) {}
            }
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "helper.service.ts").is_empty());
    }

    #[test]
    fn allows_di_constructor_with_decorated_param_property() {
        let src = r#"
            export class HelperService {
                constructor(@Inject(REQUEST) public readonly request) {}
            }
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "helper.service.ts").is_empty());
    }

    #[test]
    fn allows_constructor_with_readonly_param_property() {
        let src = r#"
            export class HelperService {
                constructor(readonly request: Request) {}
            }
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "helper.service.ts").is_empty());
    }

    #[test]
    fn allows_method_with_comment_only_body() {
        let src = r#"
            export class HelperService {
                public noop() {
                    // intentionally empty
                }
            }
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "helper.service.ts").is_empty());
    }

    #[test]
    fn allows_function_with_block_comment_only_body() {
        let src = r#"
            function noop() {
                /* intentionally empty */
            }
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn flags_empty_constructor_with_plain_param() {
        // Negative space: a plain (non-property, non-decorated) param is not DI.
        let src = r#"
            export class Foo {
                constructor(request: Request) {}
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "foo.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_no_arg_constructor() {
        // Negative space: a bare empty constructor with no params is still dead.
        let src = r#"
            export class Foo {
                constructor() {}
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "foo.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_noop_method_without_comment() {
        // Negative space: a `noop` method with NO comment is still flagged.
        let src = r#"
            export class Foo {
                public noop() {}
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "foo.ts");
        assert_eq!(diags.len(), 1);
    }
}
