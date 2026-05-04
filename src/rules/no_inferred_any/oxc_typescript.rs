//! no-inferred-any OXC backend — detect patterns whose inferred type is `any`.
//!
//! Three patterns:
//!   1. Explicit `: any` type annotation.
//!   2. `JSON.parse(...)` without `as`/`satisfies`.
//!   3. `.json()` without `as`/`satisfies`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_ts_or_tsx(ctx: &CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "ts" || ext == "tsx"
}

/// True if the call expression appears as the operand of a surrounding
/// `as`/`satisfies` — walking up through parent nodes.
fn is_narrowed(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let mut current_id = node_id;
    loop {
        let parent_id = semantic.nodes().parent_id(current_id);
        if parent_id == current_id {
            break;
        }
        let parent = semantic.nodes().get_node(parent_id);
        match parent.kind() {
            AstKind::TSAsExpression(_) | AstKind::TSSatisfiesExpression(_) => return true,
            AstKind::ParenthesizedExpression(_) | AstKind::AwaitExpression(_) => {
                current_id = parent_id;
            }
            _ => break,
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_ts_or_tsx(ctx) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                // Pattern 1: explicit `: any` annotation.
                AstKind::TSAnyKeyword(kw) => {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, kw.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Explicit `any` annotation — use a concrete type or `unknown`."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                // Patterns 2 and 3: JSON.parse / .json() without narrowing.
                AstKind::CallExpression(call) => {
                    let Expression::StaticMemberExpression(member) = &call.callee else {
                        continue;
                    };
                    let prop = member.property.name.as_str();
                    let is_json_parse = prop == "parse" && {
                        if let Expression::Identifier(obj) = &member.object {
                            obj.name.as_str() == "JSON"
                        } else {
                            false
                        }
                    };
                    let is_response_json = prop == "json";

                    if !is_json_parse && !is_response_json {
                        continue;
                    }
                    if is_narrowed(node.id(), semantic) {
                        continue;
                    }

                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    let message = if is_json_parse {
                        "`JSON.parse()` returns `any` — add a type assertion or `satisfies` clause."
                    } else {
                        "`.json()` returns `any` — add a type assertion or `satisfies` clause."
                    };
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: message.into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                _ => {}
            }
        }

        diagnostics
    }
}
