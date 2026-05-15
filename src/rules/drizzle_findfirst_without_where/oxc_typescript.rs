//! drizzle-findfirst-without-where oxc backend — flag `db.query.<table>.findFirst()`
//! whose options don't include `where:`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn callee_is_findfirst(callee: &Expression, source: &str) -> bool {
    let Expression::StaticMemberExpression(member) = callee else { return false };
    if member.property.name.as_str() != "findFirst" {
        return false;
    }
    let obj_span = member.object.span();
    let obj_text = &source[obj_span.start as usize..obj_span.end as usize];
    obj_text.starts_with("db.query.")
        || obj_text.starts_with("tx.query.")
        || obj_text.starts_with("trx.query.")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["findFirst"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !callee_is_findfirst(&call.callee, ctx.source) {
            return;
        }
        // Inspect the first argument's object-literal properties. The
        // `where` key counts whether written as `where: filter`, the
        // shorthand `where`, or spread (`...filters` — we can't see
        // inside, so play safe and skip).
        let Some(first_arg) = call.arguments.first() else { return };
        let oxc_ast::ast::Argument::ObjectExpression(obj) = first_arg else { return };
        let mut has_where = false;
        for prop in obj.properties.iter() {
            match prop {
                ObjectPropertyKind::ObjectProperty(p) => {
                    if let PropertyKey::StaticIdentifier(id) = &p.key
                        && id.name.as_str() == "where"
                    {
                        has_where = true;
                        break;
                    }
                    if let PropertyKey::Identifier(id) = &p.key
                        && id.name.as_str() == "where"
                    {
                        has_where = true;
                        break;
                    }
                }
                // Spread element — we can't see through `...x`, assume
                // it might carry `where` and skip the diagnostic.
                ObjectPropertyKind::SpreadProperty(_) => {
                    has_where = true;
                    break;
                }
            }
        }
        if has_where {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.findFirst()` without `where:` returns an arbitrary row — pass a filter to scope the query.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
