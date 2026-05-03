//! OXC backend for angular-no-subscribe-without-unsubscribe.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_angular_file(source: &str) -> bool {
    source.contains("@angular/")
        || source.contains("@Component")
        || source.contains("@Injectable")
        || source.contains("@Directive")
}

fn file_has_unsubscribe_pattern(source: &str) -> bool {
    source.contains("takeUntilDestroyed")
        || source.contains("takeUntil(")
        || source.contains("DestroyRef")
        || source.contains(".unsubscribe(")
        || source.contains("Subscription")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".unsubscribe("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !is_angular_file(ctx.source) {
            return;
        }
        if file_has_unsubscribe_pattern(ctx.source) {
            return;
        }

        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name != "subscribe" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.subscribe()` without `takeUntilDestroyed` / `DestroyRef` / explicit unsubscribe leaks the subscription.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
