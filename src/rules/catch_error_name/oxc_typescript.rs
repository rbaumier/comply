//! catch-error-name oxc backend for TypeScript / JavaScript / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CatchClause]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CatchClause(clause) = node.kind() else {
            return;
        };

        // Bare `catch {}` — no parameter to check.
        let Some(param) = &clause.param else { return };

        // Only flag simple identifiers — destructuring patterns are fine.
        let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &param.pattern else {
            return;
        };

        let name = ident.name.as_str();

        if super::is_acceptable_name(name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, ident.span.start as usize);

        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "catch-error-name".into(),
            message: format!(
                "The catch parameter `{name}` should be named `{}`.",
                super::EXPECTED
            ),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_catch_e() {
        let d = run_on("try {} catch (e) { console.log(e); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`e`"));
        assert!(d[0].message.contains("`error`"));
    }

    #[test]
    fn flags_catch_err() {
        let d = run_on("try {} catch (err) { throw err; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`err`"));
    }

    #[test]
    fn flags_catch_ex() {
        let d = run_on("try {} catch (ex) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_catch_exception() {
        let d = run_on("try {} catch (exception) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_catch_error() {
        assert!(run_on("try {} catch (error) { throw error; }").is_empty());
    }

    #[test]
    fn allows_suffixed_error() {
        assert!(run_on("try {} catch (parseError) {}").is_empty());
    }

    #[test]
    fn allows_underscore() {
        assert!(run_on("try {} catch (_) {}").is_empty());
    }

    #[test]
    fn allows_bare_catch() {
        assert!(run_on("try {} catch {}").is_empty());
    }

    #[test]
    fn allows_destructured_catch() {
        assert!(run_on("try {} catch ({ message }) {}").is_empty());
    }

    #[test]
    fn allows_inner_error_for_nested_catches() {
        let src = "try { try { a(); } catch (innerError) { b(); } } catch (error) {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn line_col_is_correct() {
        let d = run_on("try {} catch (e) {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 1);
        assert_eq!(d[0].column, 15);
    }
}
