//! ts-no-invalid-void-type OXC backend — flag `void` used outside return
//! type annotations and generic type arguments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_return_type_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    void_start: u32,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(f) => {
                if let Some(ret) = &f.return_type
                    && void_start >= ret.span.start && void_start < ret.span.end {
                        return true;
                    }
                return false;
            }
            AstKind::ArrowFunctionExpression(f) => {
                if let Some(ret) = &f.return_type
                    && void_start >= ret.span.start && void_start < ret.span.end {
                        return true;
                    }
                return false;
            }
            // TS function-type signatures like `(open: boolean) => void`
            // (in a parameter / variable annotation, NOT an arrow
            // expression). The whole type's `return_type` is required.
            AstKind::TSFunctionType(ft) => {
                let ret_span = ft.return_type.span;
                return void_start >= ret_span.start && void_start < ret_span.end;
            }
            AstKind::TSConstructorType(ct) => {
                let ret_span = ct.return_type.span;
                return void_start >= ret_span.start && void_start < ret_span.end;
            }
            AstKind::TSMethodSignature(ms) => {
                if let Some(ret) = &ms.return_type
                    && void_start >= ret.span.start && void_start < ret.span.end {
                        return true;
                    }
                return false;
            }
            AstKind::TSTypeAliasDeclaration(_) | AstKind::TSInterfaceDeclaration(_) => {
                break;
            }
            _ => continue,
        }
    }
    false
}

fn is_generic_type_arg(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::TSTypeParameterInstantiation(_) => return true,
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Class(_) => return false,
            _ => continue,
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSVoidKeyword]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSVoidKeyword(kw) = node.kind() else {
            return;
        };

        if is_return_type_context(node, semantic, kw.span.start) {
            return;
        }
        if is_generic_type_arg(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, kw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`void` is only valid as a return type or generic type argument."
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_void_variable() {
        let diags = run_on("let x: void;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_void_parameter() {
        let diags = run_on("function foo(x: void) {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_void_return_type() {
        assert!(run_on("function foo(): void {}").is_empty());
    }

    #[test]
    fn allows_void_in_generic() {
        assert!(run_on("let x: Promise<void>;").is_empty());
    }

    #[test]
    fn allows_void_in_function_type_callback() {
        // Regression for rbaumier/comply#20 — TS function type with void
        // return, common in callback prop declarations.
        let diags = run_on("type OnChange = (open: boolean) => void;");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_void_in_inline_function_type() {
        let src = r#"function setup(cb: (n: number) => void) { cb(1); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_in_method_signature() {
        let src = "interface Listener { onChange(open: boolean): void }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_in_constructor_type() {
        let src = "type Make = new (x: number) => void;";
        assert!(run_on(src).is_empty());
    }
}
