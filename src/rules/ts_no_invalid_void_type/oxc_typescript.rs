//! ts-no-invalid-void-type OXC backend — flag `void` used outside return
//! type annotations and generic type arguments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
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
            // Interface call signatures like `(ctx: X): Foo | void` — the
            // `return_type` is the boundary, reached before TSInterfaceDeclaration.
            AstKind::TSCallSignatureDeclaration(cs) => {
                if let Some(ret) = &cs.return_type
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

// `<T = void>` — `void` as the default of a generic type parameter is valid
// TypeScript (it sets `T` when the caller omits the argument). The constraint
// position (`<T extends void>`) is NOT exempted here.
fn is_generic_param_default(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    void_start: u32,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::TSTypeParameter(param) = ancestor.kind() {
            return param.default.as_ref().is_some_and(|default| {
                void_start >= default.span().start && void_start < default.span().end
            });
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
        if is_generic_param_default(node, semantic, kw.span.start) {
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

    #[test]
    fn allows_void_as_type_alias_generic_default() {
        // Regression for rbaumier/comply#1094 — `void` as a generic default.
        let src = "export type Fn<T = void> = (...values: any[]) => T;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_as_function_generic_default() {
        // Regression for rbaumier/comply#1094 — azure-sdk-for-js poller.
        let src = "export function poll<TResponse, TResult = void>(\
                   p: (r: TResponse) => Promise<TResult>) {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_void_as_generic_constraint() {
        // The constraint position is still invalid, unlike the default.
        let diags = run_on("type Fn<T extends void> = () => T;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_void_in_call_signature_return_union() {
        // Regression for rbaumier/comply#1719 — vuejs/pinia PiniaPlugin: `void`
        // in a `| void` union return type of an interface call signature.
        let src = "interface PiniaPlugin { (context: PiniaPluginContext): Partial<X> | void }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_void_in_call_signature_param_union() {
        // Negative space: `void` in a union outside the return type (here a
        // parameter annotation of a call signature) is still invalid.
        let src = "interface F { (ctx: string | void): number }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }
}
