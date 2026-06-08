//! ts-no-implicit-any-catch OXC backend — flag `catch (e)` without a type annotation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else { return };
        let Some(handler) = &try_stmt.handler else { return };
        let Some(param) = &handler.param else {
            // `catch { ... }` — no binding, nothing to annotate.
            return;
        };
        // If the catch parameter has a type annotation, it's fine.
        if param.type_annotation.is_some() {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, param.pattern.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "catch binding has no type annotation — it defaults to `any`. \
                      Use `catch (e: unknown)` and narrow the value explicitly."
                .into(),
            severity: Severity::Warning,
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
    fn flags_catch_without_annotation() {
        let diags = run_on("try { f(); } catch (e) { log(e); }");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "ts-no-implicit-any-catch");
    }


    #[test]
    fn allows_catch_with_unknown_annotation() {
        let diags = run_on("try { f(); } catch (e: unknown) { log(e); }");
        assert!(diags.is_empty());
    }


    #[test]
    fn allows_catch_with_any_annotation() {
        let diags = run_on("try { f(); } catch (e: any) { log(e); }");
        assert!(diags.is_empty());
    }


    #[test]
    fn allows_catch_without_binding() {
        let diags = run_on("try { f(); } catch { log('fail'); }");
        assert!(diags.is_empty());
    }
}
