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

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_async_constructor() {
        let src = "class Foo { async constructor() { await init(); } }";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_async_constructor_with_params() {
        let src = "class Foo { async constructor(name: string) { } }";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_regular_constructor() {
        let src = "class Foo { constructor() { } }";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_async_method() {
        let src = "class Foo { async initialize() { } }";
        assert!(run_on(src).is_empty());
    }
}
