use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// `console.<method>` calls whose return type is `void`, paired with the label
/// shown to the user.
const CONSOLE_VOID_CALLS: &[(&str, &str)] = &[
    ("log", "console.log"),
    ("error", "console.error"),
    ("warn", "console.warn"),
    ("info", "console.info"),
    ("debug", "console.debug"),
    ("table", "console.table"),
];

/// The label shown to the user for a flagged call, or `None` if the call is
/// not a known void-returning form.
fn void_call_label(expr: &Expression) -> Option<&'static str> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let method = member.property.name.as_str();

    if method == "forEach" {
        return Some(".forEach");
    }

    if let Expression::Identifier(object) = &member.object
        && object.name.as_str() == "console"
    {
        return CONSOLE_VOID_CALLS
            .iter()
            .find(|(m, _)| *m == method)
            .map(|(_, label)| *label);
    }

    None
}

impl Check {
    fn push(
        &self,
        label: &str,
        offset: u32,
        ctx: &CheckCtx,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Storing the return of `{label}` is always `undefined` — the call returns void."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator, AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["forEach", "console"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::VariableDeclarator(decl) => {
                let Some(init) = &decl.init else { return };
                if let Some(label) = void_call_label(init) {
                    self.push(label, decl.span.start, ctx, diagnostics);
                }
            }
            AstKind::AssignmentExpression(assign) => {
                if let Some(label) = void_call_label(&assign.right) {
                    self.push(label, assign.span.start, ctx, diagnostics);
                }
            }
            _ => {}
        }
    }
}
