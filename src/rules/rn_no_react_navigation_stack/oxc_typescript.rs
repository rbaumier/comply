//! OxcCheck backend for rn-no-react-navigation-stack — ban `@react-navigation/stack`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@react-navigation/stack", "createStackNavigator"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ImportDeclaration(import) => {
                if import.source.value.as_str() == "@react-navigation/stack" {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, import.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Import from `@react-navigation/stack` is forbidden — use Expo Router.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::CallExpression(call) => {
                if let Expression::Identifier(ident) = &call.callee
                    && ident.name.as_str() == "createStackNavigator" {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, call.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "`createStackNavigator` is forbidden — migrate to Expo Router."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
            }
            _ => {}
        }
    }
}
