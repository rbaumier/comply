//! no-boolean-flag-param OXC backend — flag function parameters typed as boolean.
//!
//! A boolean parameter is exempt when it is the function's first parameter and
//! the function's declared return type is also `boolean`: a boolean-in /
//! boolean-out signature is a transform over the boolean (e.g. a debounce
//! hook), not a mode flag selecting between behaviors.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const PREDICATE_PREFIXES: &[&str] = &[
    "is", "has", "should", "can", "will", "did", "was",
];

/// Standard HTML/React controlled-component props that must be boolean.
const ALLOWED_NAMES: &[&str] = &[
    "open", "checked", "disabled", "enabled", "hidden", "required", "selected",
    "readOnly", "multiple", "autoFocus", "autoPlay", "defer", "async",
    "noValidate", "defaultOpen", "defaultChecked",
];

fn has_predicate_prefix(name: &str) -> bool {
    PREDICATE_PREFIXES.iter().any(|prefix| {
        name.strip_prefix(prefix).is_some_and(|rest| {
            rest.is_empty() || rest.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        })
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::FormalParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::FormalParameter(param) = node.kind() else {
            return;
        };

        // Check type annotation is `: boolean`
        let Some(ts_type) = param
            .type_annotation
            .as_ref()
            .map(|ann| &ann.type_annotation)
        else {
            return;
        };

        if !matches!(
            ts_type,
            oxc_ast::ast::TSType::TSBooleanKeyword(_)
        ) {
            return;
        }

        let name = match &param.pattern {
            oxc_ast::ast::BindingPattern::BindingIdentifier(id) => id.name.as_str(),
            _ => "<flag>",
        };

        if ALLOWED_NAMES.contains(&name) || has_predicate_prefix(name) {
            return;
        }

        // Only runtime functions can have a mode-flag split out of them. A
        // type-level callable position (TSFunctionType, TSCallSignatureDeclaration,
        // TSConstructSignatureDeclaration, TSMethodSignature, …) is a pure type
        // annotation with no body, so the "split into two functions" advice is
        // meaningless. Require an actual runtime function parent.
        if !is_runtime_function_param(node, semantic) {
            return;
        }

        if is_boolean_transform_subject(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Boolean parameter '{name}' controls a branch — split \
                 into two named functions instead. A ternary or options \
                 object is not a fix; the boolean must disappear from \
                 the signature entirely."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when the parameter's enclosing parameter list belongs to an actual
/// runtime function (a `Function` — which includes class/object methods, whose
/// value is a `Function` node — or an `ArrowFunctionExpression`). Every other
/// parent is a type-level callable signature with no body, which cannot be
/// split and must not be flagged. The allowlist is positive so that new
/// type-level node kinds are skipped by default.
fn is_runtime_function_param<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let params_node = nodes.parent_node(node.id());
    if !matches!(params_node.kind(), AstKind::FormalParameters(_)) {
        return false;
    }
    matches!(
        nodes.parent_node(params_node.id()).kind(),
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
    )
}

/// True when the parameter is the function's subject rather than a mode flag:
/// it is the first parameter of a function whose declared return type is also
/// `boolean` (a boolean-in/boolean-out transform, e.g. `useDelayedFlag`).
fn is_boolean_transform_subject<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let AstKind::FormalParameter(param) = node.kind() else {
        return false;
    };
    let nodes = semantic.nodes();
    let params_node = nodes.parent_node(node.id());
    let AstKind::FormalParameters(params) = params_node.kind() else {
        return false;
    };
    if params.items.first().is_none_or(|first| first.span != param.span) {
        return false;
    }
    match nodes.parent_node(params_node.id()).kind() {
        AstKind::Function(func) => returns_boolean(func.return_type.as_deref()),
        AstKind::ArrowFunctionExpression(arrow) => returns_boolean(arrow.return_type.as_deref()),
        _ => false,
    }
}

fn returns_boolean(return_type: Option<&oxc_ast::ast::TSTypeAnnotation<'_>>) -> bool {
    return_type.is_some_and(|ann| {
        matches!(ann.type_annotation, oxc_ast::ast::TSType::TSBooleanKeyword(_))
    })
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_bare_boolean_param() {
        assert_eq!(run("function send(urgent: boolean) {}").len(), 1);
    }

    #[test]
    fn allows_predicate_prefix() {
        assert!(run("function f(isReady: boolean) {}").is_empty());
        assert!(run("function f(hasAccess: boolean) {}").is_empty());
    }

    // Regression for #272: a `can*` authz-gate flag is predicate-prefixed and
    // exempt — a column factory's `canEdit` must not be flagged (in either the
    // bare or destructured form).
    #[test]
    fn allows_can_prefix_authz_flag() {
        assert!(run("function getTeamsColumns(canEdit: boolean) {}").is_empty());
        assert!(
            run("function getTeamsColumns({ canEdit }: { canEdit: boolean }) {}").is_empty()
        );
    }

    // Regression for #910: a spin-delay hook debounces a boolean signal — the
    // boolean is the data the function transforms (boolean in, boolean out),
    // not a mode flag. Exact reproducer from the issue.
    #[test]
    fn no_fp_debounce_hook_boolean_subject_issue_910() {
        let src = "export function useDelayedFlag(\
                     isActive: boolean,\
                     options: { delayMs: number; minVisibleMs: number },\
                   ): boolean {\
                     const delay = isActive ? options.delayMs : options.minVisibleMs;\
                     return isActive && delay > 0;\
                   }";
        assert!(run(src).is_empty(), "got {:#?}", run(src));
    }

    // Same shape without a predicate-prefixed name (real-world spin-delay
    // hooks take `loading`): the boolean-in/boolean-out exemption must carry it.
    #[test]
    fn allows_first_boolean_param_of_boolean_returning_fn() {
        assert!(
            run("export function useSpinDelay(loading: boolean, options: { delayMs: number }): boolean { return loading; }")
                .is_empty()
        );
        assert!(run("const useDelayed = (active: boolean): boolean => active;").is_empty());
    }

    // A boolean-returning function whose boolean is NOT the first parameter is
    // still a mode flag — `save(data, sendEmail)` must keep firing.
    #[test]
    fn still_flags_mode_flag_in_boolean_returning_fn() {
        assert_eq!(
            run("function save(data: string, sendEmail: boolean): boolean { return sendEmail; }")
                .len(),
            1
        );
    }

    // A first boolean param without a boolean return type is still a flag.
    #[test]
    fn still_flags_first_boolean_param_without_boolean_return() {
        assert_eq!(run("function send(urgent: boolean): void {}").len(), 1);
    }

    // Regression for #3316: a boolean param inside a `TSFunctionType` (here used
    // as a generic argument) is a pure type annotation with no body — there is
    // no runtime function to split, so it must not be flagged.
    #[test]
    fn no_fp_boolean_param_in_ts_function_type_issue_3316() {
        let src =
            "declare const v: SetReturnType<(foo: string, bar: boolean) => number, void>;";
        assert!(run(src).is_empty(), "got {:#?}", run(src));
    }

    // Regression for #3316: a boolean param inside a `TSCallSignatureDeclaration`
    // is type-level only.
    #[test]
    fn no_fp_boolean_param_in_call_signature_issue_3316() {
        assert!(run("type F = {(a1: boolean, ...a2: string[]): number};").is_empty());
    }

    // Regression for #3316: a boolean param inside a `TSConstructSignatureDeclaration`
    // is type-level only.
    #[test]
    fn no_fp_boolean_param_in_construct_signature_issue_3316() {
        assert!(run("type Ctor = { new (flag: boolean): X };").is_empty());
    }

    // Guard: requiring a runtime-function parent must not exempt class/object
    // methods — in oxc a method's value is a `Function` node, so the flag still
    // fires.
    #[test]
    fn still_flags_boolean_flag_param_in_method() {
        assert_eq!(
            run("class Renderer { render(html: string, pretty: boolean) {} }").len(),
            1
        );
        assert_eq!(
            run("const o = { render(html: string, pretty: boolean) {} };").len(),
            1
        );
    }
}
