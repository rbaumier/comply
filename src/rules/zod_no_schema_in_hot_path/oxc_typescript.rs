//! zod-no-schema-in-hot-path OxcCheck backend — flag `z.*` calls whose nearest
//! enclosing function looks like a React component or a request handler, or
//! that sit inside a loop body.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // The callee must be a member expression chain rooted at `z`.
        if !is_z_chain(&call.callee, ctx.source) {
            return;
        }

        let nodes = semantic.nodes();
        let mut current_id = node.id();

        loop {
            let parent_id = nodes.parent_id(current_id);
            if parent_id == current_id {
                // Reached root.
                return;
            }
            let parent = nodes.get_node(parent_id);
            match parent.kind() {
                AstKind::ForStatement(_)
                | AstKind::ForInStatement(_)
                | AstKind::WhileStatement(_)
                | AstKind::DoWhileStatement(_) => {
                    if call_references_loop_binding(call, parent.kind(), ctx.source) {
                        return;
                    }
                    let span = call.span();
                    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Zod schema built inside a loop body — hoist it outside the \
                                  loop so it is only constructed once."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
                AstKind::Function(func) => {
                    if is_hot_scope_oxc(func, parent_id, semantic, ctx.source) {
                        let span = call.span();
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Zod schema built inside a React component or request \
                                      handler — hoist it to module scope so it is only \
                                      constructed once."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    return;
                }
                AstKind::ArrowFunctionExpression(_) => {
                    if is_hot_scope_arrow(parent_id, semantic, ctx.source) {
                        let span = call.span();
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Zod schema built inside a React component or request \
                                      handler — hoist it to module scope so it is only \
                                      constructed once."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    return;
                }
                AstKind::Program(_) => return,
                _ => {}
            }
            current_id = parent_id;
        }
    }
}

fn starts_uppercase(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
}

/// Check if a `z.*` member expression chain is rooted at the identifier `z`.
fn is_z_chain(expr: &Expression<'_>, source: &str) -> bool {
    match expr {
        Expression::StaticMemberExpression(member) => is_z_chain(&member.object, source),
        Expression::ComputedMemberExpression(member) => is_z_chain(&member.object, source),
        Expression::CallExpression(call) => is_z_chain(&call.callee, source),
        Expression::Identifier(ident) => ident.name == "z",
        _ => false,
    }
}

fn function_name_from_oxc<'a>(
    func: &oxc_ast::ast::Function<'a>,
    func_node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> Option<String> {
    // function_declaration: has its own name.
    if let Some(id) = &func.id {
        return Some(id.name.to_string());
    }
    // Arrow assigned to variable_declarator: check parent.
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(func_node_id);
    if parent_id != func_node_id
        && let AstKind::VariableDeclarator(decl) = nodes.get_node(parent_id).kind() {
            let name = &source[decl.id.span().start as usize..decl.id.span().end as usize];
            return Some(name.to_string());
        }
    None
}

/// Known framework request/response types whose presence in a parameter type
/// annotation reliably identifies an HTTP handler.
fn is_framework_handler_type(type_text: &str) -> bool {
    const FRAMEWORK_TYPES: &[&str] = &[
        "NextApiRequest",
        "NextApiResponse",
        "NextRequest",
        "NextResponse",
        "Request",
        "Response",
    ];
    FRAMEWORK_TYPES.iter().any(|t| type_text.contains(t))
}

fn is_request_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "req" || lower == "request"
}

fn is_response_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "res" || lower == "response"
}

/// Recognise an HTTP request handler by its parameters. A lone `ctx`/`req`/`res`
/// is not enough — `ctx` in particular is a ubiquitous name for domain context
/// objects. A function is a handler only when it takes both a request-shaped and
/// a response-shaped parameter, or a parameter annotated with a known framework
/// request/response type.
fn looks_like_handler_params(params: &oxc_ast::ast::FormalParameters<'_>, source: &str) -> bool {
    let mut has_request = false;
    let mut has_response = false;
    for param in &params.items {
        let pattern_span = param.pattern.span();
        let name = &source[pattern_span.start as usize..pattern_span.end as usize];
        if is_request_name(name) {
            has_request = true;
        }
        if is_response_name(name) {
            has_response = true;
        }
        if let Some(ann) = &param.type_annotation {
            let type_text = &source[ann.span.start as usize..ann.span.end as usize];
            if is_framework_handler_type(type_text) {
                return true;
            }
        }
    }
    has_request && has_response
}

/// Collect binding names introduced by a loop's own header — the index in a
/// `for (let i = …; …)` or the key in a `for (const k in …)`. These are the
/// bindings a dynamic schema inside the loop legitimately varies over.
fn loop_binding_names(loop_kind: AstKind<'_>, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let decl = match loop_kind {
        AstKind::ForStatement(stmt) => match &stmt.init {
            Some(oxc_ast::ast::ForStatementInit::VariableDeclaration(decl)) => Some(decl),
            _ => None,
        },
        AstKind::ForInStatement(stmt) => match &stmt.left {
            oxc_ast::ast::ForStatementLeft::VariableDeclaration(decl) => Some(decl),
            _ => None,
        },
        _ => None,
    };
    if let Some(decl) = decl {
        for declarator in &decl.declarations {
            let span = declarator.id.span();
            names.push(source[span.start as usize..span.end as usize].to_string());
        }
    }
    names
}

/// Whole-word match: true if `word` appears in `text` not surrounded by other
/// identifier characters.
fn text_references_word(text: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(pos) = text[start..].find(word) {
        let abs = start + pos;
        let before_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        let after = abs + word.len();
        let after_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
        if before_ok && after_ok {
            return true;
        }
        start = abs + word.len();
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// True when a `z.*` call inside a loop references the loop's own binding in its
/// arguments — a dynamic schema that cannot be hoisted out of the loop. Constant
/// schemas (no loop-var reference, or zero-arg calls) are not exempt.
fn call_references_loop_binding(
    call: &oxc_ast::ast::CallExpression<'_>,
    loop_kind: AstKind<'_>,
    source: &str,
) -> bool {
    let bindings = loop_binding_names(loop_kind, source);
    if bindings.is_empty() {
        return false;
    }
    let (Some(first), Some(last)) = (call.arguments.first(), call.arguments.last()) else {
        return false;
    };
    let args_text = &source[first.span().start as usize..last.span().end as usize];
    bindings
        .iter()
        .any(|name| text_references_word(args_text, name))
}

/// True if the function's return type or generic constraints reference a Zod type,
/// indicating it is a schema factory function (not a React component).
fn looks_like_schema_factory(
    return_type: Option<&oxc_ast::ast::TSTypeAnnotation>,
    type_parameters: Option<&oxc_ast::ast::TSTypeParameterDeclaration>,
    source: &str,
) -> bool {
    if let Some(ret) = return_type {
        let text = &source[ret.span.start as usize..ret.span.end as usize];
        if text.contains("Zod") || text.contains("z.Z") {
            return true;
        }
    }
    if let Some(tp) = type_parameters {
        let text = &source[tp.span.start as usize..tp.span.end as usize];
        if text.contains("Zod") || text.contains("z.Z") {
            return true;
        }
    }
    false
}

fn is_hot_scope_oxc<'a>(
    func: &oxc_ast::ast::Function<'a>,
    func_node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    if let Some(name) = function_name_from_oxc(func, func_node_id, semantic, source)
        && starts_uppercase(&name) && name != "Check"
        && !looks_like_schema_factory(
            func.return_type.as_deref(),
            func.type_parameters.as_deref(),
            source,
        ) {
            return true;
        }
    looks_like_handler_params(&func.params, source)
}

fn is_hot_scope_arrow<'a>(
    arrow_node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let arrow = nodes.get_node(arrow_node_id);
    let AstKind::ArrowFunctionExpression(arrow_expr) = arrow.kind() else {
        return false;
    };

    // Check parent for variable name (arrow assigned to const).
    let parent_id = nodes.parent_id(arrow_node_id);
    if parent_id != arrow_node_id
        && let AstKind::VariableDeclarator(decl) = nodes.get_node(parent_id).kind() {
            let name = &source[decl.id.span().start as usize..decl.id.span().end as usize];
            if starts_uppercase(name) && name != "Check"
                && !looks_like_schema_factory(
                    arrow_expr.return_type.as_deref(),
                    arrow_expr.type_parameters.as_deref(),
                    source,
                ) {
                return true;
            }
        }

    looks_like_handler_params(&arrow_expr.params, source)
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_schema_in_react_component() {
        let src = "function MyForm() { const S = z.object({ a: z.string() }); return null; }";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn flags_schema_in_handler() {
        let src = "const handler = (req, res) => { const S = z.object({ a: z.string() }); };";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_module_level_schema() {
        let src = "const S = z.object({ a: z.string() });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_schema_in_plain_helper() {
        let src = "function helper() { const S = z.object({ a: z.string() }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_schema_in_for_loop() {
        let src = "for (let i = 0; i < 10; i++) { const S = z.string(); }";
        assert_eq!(run(src).len(), 1);
    }

    // Regression: generic schema factory functions must not be flagged (#509).
    #[test]
    fn allows_generic_schema_factory_with_zod_type_param() {
        let src = "function PaginatedResponseSchema<TItem extends z.ZodTypeAny>(item: TItem): z.ZodObject<any> { return z.object({ items: z.array(item) }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_generic_schema_factory_with_zod_return_type() {
        let src = "function WrapSchema<T>(schema: T): z.ZodObject<any> { return z.object({ data: schema }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_uppercase_component_without_zod_signature() {
        let src = "function TeamList() { const S = z.object({ a: z.string() }); return null; }";
        assert!(!run(src).is_empty());
    }

    // Regression #2070, Pattern 1: a lone `ctx` domain-context parameter must not
    // mark a converter as an HTTP handler.
    #[test]
    fn allows_converter_with_lone_ctx_param() {
        let src =
            "function convertBaseSchema(schema: JSONSchema, ctx: ConversionContext) { return z.never(); }";
        assert!(run(src).is_empty());
    }

    // Regression #2070, Pattern 1 guard: a real handler with both a framework
    // request and response type still fires.
    #[test]
    fn still_flags_real_handler_with_framework_types() {
        let src =
            "function handler(req: NextApiRequest, res: NextApiResponse) { z.object({}); }";
        assert!(!run(src).is_empty());
    }

    // Regression #2070, Pattern 2: a `z.*` call inside a loop that references the
    // loop's own binding builds a dynamic schema and must not be flagged.
    #[test]
    fn allows_loop_bound_dynamic_schema() {
        let src = "function f(schemasToIntersect: any[]) { let result = schemasToIntersect[0]; for (let i = 2; i < schemasToIntersect.length; i++) { result = z.intersection(result, schemasToIntersect[i]); } return result; }";
        assert!(run(src).is_empty());
    }

    // Regression #2070, Pattern 2 guard: a constant schema rebuilt in a loop with
    // no reference to the loop binding still fires.
    #[test]
    fn still_flags_constant_schema_in_loop() {
        let src = "for (let i = 0; i < 10; i++) { const S = z.string(); }";
        assert_eq!(run(src).len(), 1);
    }
}
