//! security-no-query-without-ownership OxcCheck backend —
//! DB "find by id" calls without an ownership filter.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
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

        let Some(name) = call_function_name_oxc(&call.callee, ctx.source) else {
            return;
        };
        if !is_find_by_id(&name) {
            return;
        }

        if path_is_script_or_internal(ctx.path) {
            return;
        }

        if !is_in_route_handler(node.id(), semantic, ctx.source) {
            return;
        }

        // Scan the full call text for an ownership filter
        let full_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        if has_ownership_filter(full_text) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{name}` has no ownership filter (userId/orgId/tenantId) — possible IDOR."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Extract the dotted function name from a call expression callee.
fn call_function_name_oxc(callee: &Expression<'_>, source: &str) -> Option<String> {
    match callee {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(member) => {
            let obj = call_function_name_oxc(&member.object, source)?;
            Some(format!("{}.{}", obj, member.property.name))
        }
        _ => None,
    }
}

fn is_find_by_id(name: &str) -> bool {
    let Some(method) = name.rsplit('.').next() else {
        return false;
    };
    matches!(
        method,
        "findById" | "findOne" | "findUnique" | "findFirst" | "getById"
    )
}

fn has_ownership_filter(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("userid")
        || lower.contains("user_id")
        || lower.contains("ownerid")
        || lower.contains("owner_id")
        || lower.contains("orgid")
        || lower.contains("org_id")
        || lower.contains("tenantid")
        || lower.contains("tenant_id")
        || lower.contains("accountid")
        || lower.contains("account_id")
}

fn path_is_script_or_internal(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    let lower = s.to_ascii_lowercase();
    for marker in [
        "/scripts/",
        "/jobs/",
        "/cron/",
        "/seed/",
        "/seeds/",
        "/admin/",
        "/migrations/",
    ] {
        if lower.contains(marker) {
            return true;
        }
    }
    false
}

fn is_in_route_handler(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node_id;

    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);

        match parent.kind() {
            // Express/Hono/Fastify-style: app.get(...), app.post(...)
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(member) = &call.callee {
                    let prop = member.property.name.as_str();
                    if matches!(prop, "get" | "post" | "put" | "patch" | "delete" | "all") {
                        return true;
                    }
                }
            }
            // Next.js / Remix-style: export function GET(req) { ... }
            AstKind::Function(func) => {
                if let Some(id) = &func.id {
                    let name = id.name.as_str();
                    if matches!(
                        name,
                        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
                    ) {
                        // Check if exported
                        let gp_id = nodes.parent_id(parent_id);
                        if gp_id != parent_id
                            && let AstKind::ExportNamedDeclaration(_) =
                                nodes.get_node(gp_id).kind()
                            {
                                return true;
                            }
                    }
                }
                // Check for request-like parameter names
                if function_has_request_param_oxc(&func.params, source) {
                    return true;
                }
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                if function_has_request_param_oxc(&arrow.params, source) {
                    return true;
                }
            }
            _ => {}
        }
        current_id = parent_id;
    }
}

fn function_has_request_param_oxc(
    params: &oxc_ast::ast::FormalParameters<'_>,
    source: &str,
) -> bool {
    for param in &params.items {
        let text = &source[param.span.start as usize..param.span.end as usize];
        let first = text
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");
        if matches!(first, "req" | "request" | "ctx" | "context") {
            return true;
        }
    }
    false
}
