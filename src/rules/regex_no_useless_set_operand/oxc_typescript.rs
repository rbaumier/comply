//! regex-no-useless-set-operand OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::RegExpFlags;
use std::sync::Arc;

pub struct Check;

/// Detects useless operands in `v`-flag character class set operations.
fn has_useless_set_op(pattern: &str) -> bool {
    let complementary_pairs: &[(&str, &str)] = &[
        (r"\d", r"\w"),
        (r"\w", r"\W"),
        (r"\d", r"\D"),
        (r"\s", r"\S"),
    ];

    for &(a, b) in complementary_pairs {
        let intersection = format!("[{a}&&{b}]");
        let intersection_rev = format!("[{b}&&{a}]");
        let subtraction = format!("[{a}--{b}]");
        let subtraction_rev = format!("[{b}--{a}]");

        if pattern.contains(&intersection)
            || pattern.contains(&intersection_rev)
            || pattern.contains(&subtraction)
            || pattern.contains(&subtraction_rev)
        {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(re) = node.kind() else { return };

        if !re.regex.flags.contains(RegExpFlags::V) {
            return;
        }
        let pattern = re.regex.pattern.text.as_str();
        if !has_useless_set_op(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Useless operand in character class set operation.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
