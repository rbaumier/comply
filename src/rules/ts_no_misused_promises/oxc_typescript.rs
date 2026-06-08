//! ts-no-misused-promises oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

/// Callees whose first argument is supposed to be a void-returning
/// function. Each of these passes `Promise` returns silently —
/// they're either dropped (useEffect) or treated as truthy in
/// follow-up callbacks (.forEach).
const VOID_CALLBACK_CALLEES: &[&str] = &[
    // React effect hooks.
    "useEffect",
    "useLayoutEffect",
    "useInsertionEffect",
];

const VOID_CALLBACK_METHODS: &[&str] = &[
    "forEach",
    // DOM event listeners.
    "addEventListener",
];

fn callee_expects_void_callback(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => {
            VOID_CALLBACK_CALLEES.contains(&id.name.as_str())
        }
        Expression::StaticMemberExpression(m) => {
            let prop = m.property.name.as_str();
            VOID_CALLBACK_METHODS.contains(&prop)
                || VOID_CALLBACK_CALLEES.contains(&prop)
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !callee_expects_void_callback(call) {
            return;
        }
        let Some(arg) = call.arguments.first() else {
            return;
        };
        let (is_async, span) = match arg {
            Argument::ArrowFunctionExpression(a) => (a.r#async, a.span),
            Argument::FunctionExpression(f) => (f.r#async, f.span),
            _ => return,
        };
        if !is_async {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Async callback passed to a void-returning slot — the returned \
                      Promise is dropped and errors disappear silently."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_async_use_effect_callback() {
        let src = r#"useEffect(async () => { await fetch("/api"); }, []);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_async_for_each() {
        let src = r#"items.forEach(async (item) => { await process(item); });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_sync_callback() {
        let src = r#"useEffect(() => { fetch("/api"); }, []);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_iife_wrapped_async() {
        let src = r#"useEffect(() => { void (async () => { await x; })(); }, []);"#;
        assert!(run(src).is_empty());
    }
}
