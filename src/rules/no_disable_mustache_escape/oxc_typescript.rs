//! no-disable-mustache-escape OxcCheck backend — flag disabling of HTML
//! escaping in template engines.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const DISABLE_PATTERNS: &[(&str, &str, &str)] = &[
    ("escapeMarkup", "false", "escapeMarkup = false"),
    ("escape", "false", "escape = false"),
    ("noEscape", "true", "noEscape: true"),
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression, AstType::ObjectExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["escapeMarkup", "noEscape"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::AssignmentExpression(assign) => {
                check_assignment(assign, ctx, diagnostics);
            }
            AstKind::ObjectExpression(obj) => {
                check_object_properties(obj, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

fn check_assignment(
    assign: &oxc_ast::ast::AssignmentExpression,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let prop_name = match &assign.left {
        oxc_ast::ast::AssignmentTarget::StaticMemberExpression(mem) => {
            Some(mem.property.name.as_str())
        }
        oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(id) => {
            Some(id.name.as_str())
        }
        _ => None,
    };
    let Some(prop) = prop_name else { return };

    let val_text = match &assign.right {
        Expression::BooleanLiteral(b) => if b.value { "true" } else { "false" },
        _ => return,
    };

    for &(name, bad_val, desc) in DISABLE_PATTERNS {
        if prop == name && val_text == bad_val {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, assign.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Disabling HTML escaping via `{}` — keep escaping enabled to prevent XSS.",
                    desc,
                ),
                severity: Severity::Error,
                span: None,
            });
            return;
        }
    }
}

fn check_object_properties(
    obj: &oxc_ast::ast::ObjectExpression,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for prop in &obj.properties {
        let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop else { continue };

        let key_text = match &p.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };

        let val_text = match &p.value {
            Expression::BooleanLiteral(b) => if b.value { "true" } else { "false" },
            _ => continue,
        };

        for &(name, bad_val, desc) in DISABLE_PATTERNS {
            if key_text == name && val_text == bad_val {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, p.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Disabling HTML escaping via `{}` — keep escaping enabled to prevent XSS.",
                        desc,
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_escape_markup_false_assignment() {
        assert_eq!(run_on("options.escapeMarkup = false;").len(), 1);
    }

    #[test]
    fn flags_escape_markup_property() {
        assert_eq!(run_on("const x = { escapeMarkup: false };").len(), 1);
    }

    #[test]
    fn flags_no_escape_true() {
        assert_eq!(run_on("const x = { noEscape: true };").len(), 1);
    }

    #[test]
    fn allows_escape_enabled() {
        assert!(run_on("const x = { escapeMarkup: true };").is_empty());
    }
}
