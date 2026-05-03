//! no-async-constructor oxc backend — flag `async constructor()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{MethodDefinitionKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::MethodDefinition]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["constructor"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::MethodDefinition(method) = node.kind() else { return };

        if method.kind != MethodDefinitionKind::Constructor {
            return;
        }

        // Check the method name is literally "constructor".
        let name = match &method.key {
            PropertyKey::StaticIdentifier(ident) => ident.name.as_str(),
            _ => return,
        };
        if name != "constructor" {
            return;
        }

        if !method.value.r#async {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, method.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Constructors cannot be `async` — use a static async factory method instead.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
