use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else { return };

        let Some(handler) = &try_stmt.handler else { return };
        let Some(param) = &handler.param else {
            return; // bare `catch { ... }` — nothing to flag.
        };
        // Only handle simple identifier bindings.
        let BindingPattern::BindingIdentifier(ident) = &param.pattern else {
            return;
        };
        let name = ident.name.as_str();

        // Use semantic symbol info to check if the binding is referenced.
        if let Some(symbol_id) = ident.symbol_id.get() {
            let mut refs = semantic.symbol_references(symbol_id);
            if refs.next().is_some() {
                return; // has at least one reference — binding is used.
            }
        } else {
            // No symbol id — fallback to text check in the body.
            let body_src =
                &ctx.source[handler.body.span.start as usize..handler.body.span.end as usize];
            if body_src.contains(name) {
                return;
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, ident.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`catch ({name})` is never used — drop the binding (`catch {{ ... }}`) \
                 or reference `{name}` in the handler."
            ),
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
    fn flags_unused_binding() {
        let d = run_on("try { x(); } catch (e) { return null; }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-catch-without-use");
    }


    #[test]
    fn flags_unused_binding_with_log() {
        let d = run_on("try { x(); } catch (e) { console.log('oops'); }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_used_binding() {
        assert!(run_on("try { x(); } catch (e) { console.log(e); }").is_empty());
    }


    #[test]
    fn allows_rethrow() {
        assert!(run_on("try { x(); } catch (e) { throw e; }").is_empty());
    }


    #[test]
    fn allows_bare_catch() {
        assert!(run_on("try { x(); } catch { return null; }").is_empty());
    }


    #[test]
    fn allows_destructured_binding() {
        // Destructuring is skipped — conservative.
        assert!(run_on("try { x(); } catch ({ code }) { return null; }").is_empty());
    }


    #[test]
    fn allows_used_in_nested_expr() {
        assert!(run_on("try { x(); } catch (e) { return new Error(e.message); }").is_empty());
    }
}
