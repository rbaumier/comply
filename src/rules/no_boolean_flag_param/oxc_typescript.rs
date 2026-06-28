//! no-boolean-flag-param OXC backend — flag function parameters typed as boolean.
//!
//! A boolean parameter is exempt when it is the function's first parameter and
//! the function's declared return type is also `boolean`: a boolean-in /
//! boolean-out signature is a transform over the boolean (e.g. a debounce
//! hook), not a mode flag selecting between behaviors.
//!
//! A boolean parameter is also exempt when it is a pure forwarding passthrough:
//! every reference to it is a direct positional argument of a call or `new`
//! expression (`return parse(code, jsx)`). Such a wrapper mirrors the callee's
//! API — the boolean is forwarded as-is, never dispatched on — so it cannot be
//! split into two functions.
//!
//! Finally, a boolean parameter is exempt when its enclosing function is an
//! inline callback passed directly as an argument to a call or `new` expression
//! (`runFunction(args, state, meta, (x, cumulative: boolean) => …)`). The
//! callback's parameter list is the *caller's* invocation contract — the
//! dispatcher invokes it positionally — so the author cannot rename it or split
//! it into two named functions.
//!
//! A boolean parameter is also exempt when it is a class setter's parameter
//! (`set foo(value: boolean) {}`). A setter has exactly one parameter — the
//! value being assigned — and cannot be split into two named functions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
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

        // A class setter's single parameter is the value being assigned, not a
        // mode flag selecting between behaviors. A setter has exactly one
        // parameter and cannot be split into two named functions, so the advice
        // is structurally inapplicable.
        if is_setter_param(node, semantic) {
            return;
        }

        if is_boolean_transform_subject(node, semantic) {
            return;
        }

        if is_forwarded_passthrough_param(node, semantic) {
            return;
        }

        if is_inline_callback_argument_param(node, semantic) {
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
/// runtime function (a `Function` with a body — which includes class/object
/// methods, whose value is a `Function` node — or an `ArrowFunctionExpression`).
/// A bodyless `Function` (an overload signature, an abstract method, or an
/// ambient `declare` function) is a pure type-level declaration with no runtime
/// branching, so it cannot be split and must not be flagged. Every other parent
/// is a type-level callable signature with no body.
/// The allowlist is positive so that new type-level node kinds are skipped by
/// default.
fn is_runtime_function_param<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let params_node = nodes.parent_node(node.id());
    if !matches!(params_node.kind(), AstKind::FormalParameters(_)) {
        return false;
    }
    match nodes.parent_node(params_node.id()).kind() {
        AstKind::Function(func) => func.body.is_some(),
        AstKind::ArrowFunctionExpression(_) => true,
        _ => false,
    }
}

/// True when the parameter belongs to a class setter accessor
/// (`set foo(value: boolean) {}`). The enclosing chain is
/// `FormalParameters → Function` (the setter body) `→ MethodDefinition` whose
/// `kind` is `Set`. A setter has exactly one parameter — the value being
/// assigned — so it cannot be split into two named functions.
fn is_setter_param<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let params_node = nodes.parent_node(node.id());
    if !matches!(params_node.kind(), AstKind::FormalParameters(_)) {
        return false;
    }
    let func_node = nodes.parent_node(params_node.id());
    if !matches!(func_node.kind(), AstKind::Function(_)) {
        return false;
    }
    matches!(
        nodes.parent_node(func_node.id()).kind(),
        AstKind::MethodDefinition(method)
            if method.kind == oxc_ast::ast::MethodDefinitionKind::Set
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

/// True when the boolean param is a pure forwarding passthrough: it has at least
/// one reference and EVERY reference is a direct positional argument of a call or
/// `new` expression (`return wasmParse(code, flag, jsx)`). Such a wrapper mirrors
/// the callee's API — the boolean is forwarded, never dispatched on in this
/// function — so the "split into two functions" advice is inapplicable. A param
/// used in any branch position (`if (flag)`, `flag ? :`, `flag && x`) or returned
/// directly (`return flag`) or unused (empty body, zero references) is NOT a
/// passthrough and stays flagged.
fn is_forwarded_passthrough_param<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let AstKind::FormalParameter(param) = node.kind() else {
        return false;
    };
    // Only a simple named binding (the destructured case is "<flag>", left flagged).
    let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &param.pattern else {
        return false;
    };
    let Some(symbol_id) = id.symbol_id.get() else {
        return false;
    };
    let nodes = semantic.nodes();
    let mut saw_reference = false;
    for reference in semantic.scoping().get_resolved_references(symbol_id) {
        saw_reference = true;
        let ref_span = nodes.kind(reference.node_id()).span();
        let parent = nodes.parent_node(reference.node_id());
        let arguments = match parent.kind() {
            AstKind::CallExpression(call) => &call.arguments,
            AstKind::NewExpression(new_expr) => &new_expr.arguments,
            _ => return false,
        };
        // The reference must be a positional ARGUMENT, not the callee.
        if !arguments.iter().any(|arg| arg.span() == ref_span) {
            return false;
        }
    }
    saw_reference
}

/// True when the parameter's enclosing function is an inline callback passed
/// directly as a positional argument of a call or `new` expression
/// (`run(args, (x, cumulative: boolean) => …)`). The callback's parameter list
/// is the *caller's* invocation contract — the dispatcher binds arguments
/// positionally — so the author cannot rename it or split it into two named
/// functions. Only an anonymous function/arrow that is itself a direct argument
/// qualifies; a named declaration, or a function nested inside another
/// expression (object property, array element, ternary), does not.
fn is_inline_callback_argument_param<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    // node → FormalParameters → Function | ArrowFunctionExpression
    let params_node = nodes.parent_node(node.id());
    if !matches!(params_node.kind(), AstKind::FormalParameters(_)) {
        return false;
    }
    let func_node = nodes.parent_node(params_node.id());
    let func_span = match func_node.kind() {
        AstKind::Function(func) => func.span,
        AstKind::ArrowFunctionExpression(arrow) => arrow.span,
        _ => return false,
    };
    let parent = nodes.parent_node(func_node.id());
    let arguments = match parent.kind() {
        AstKind::CallExpression(call) => &call.arguments,
        AstKind::NewExpression(new_expr) => &new_expr.arguments,
        _ => return false,
    };
    arguments
        .iter()
        .any(|arg| arg.as_expression().is_some_and(|expr| expr.span() == func_span))
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

    // Regression for #4488: a passthrough wrapper forwards its boolean params
    // verbatim to a WASM binding (`parse`). The booleans are never dispatched on
    // here, so the "split into two functions" advice is inapplicable — the
    // wrapper mirrors the binding's exact API. Exact reproducer from the issue.
    #[test]
    fn no_fp_forwarded_passthrough_params_issue_4488() {
        let src = "export async function parseAsync(\
                     code: string,\
                     allowReturnOutsideFunction: boolean,\
                     jsx: boolean,\
                     _signal?: any,\
                   ) { return parse(code, allowReturnOutsideFunction, jsx); }";
        assert!(run(src).is_empty(), "got {:#?}", run(src));
    }

    // A boolean forwarded as-is to a plain call is a passthrough.
    #[test]
    fn allows_boolean_forwarded_to_call() {
        assert!(run("function f(verbose: boolean) { return log(verbose); }").is_empty());
    }

    // A boolean forwarded as-is to a `new` expression is a passthrough.
    #[test]
    fn allows_boolean_forwarded_to_new() {
        assert!(run("function f(strict: boolean) { return new Parser(strict); }").is_empty());
    }

    // A boolean used in an `if` branch is dispatched on, not forwarded — flagged.
    #[test]
    fn still_flags_boolean_branched_in_if() {
        assert_eq!(run("function f(flag: boolean) { if (flag) doA(); else doB(); }").len(), 1);
    }

    // A boolean used as a ternary test is dispatched on — flagged.
    #[test]
    fn still_flags_boolean_in_ternary() {
        assert_eq!(run("function f(flag: boolean) { return flag ? a() : b(); }").len(), 1);
    }

    // A boolean inside a `&&` short-circuit is not a direct argument — the
    // reference is the operand of a logical expression, so it stays flagged.
    #[test]
    fn still_flags_boolean_in_short_circuit_arg() {
        assert_eq!(run("function f(flag: boolean) { return run(flag && other); }").len(), 1);
    }

    // Regression for #5639: a spreadsheet-engine dispatcher (`runFunction`)
    // binds parsed-formula arguments positionally into an inline callback. The
    // callback's parameter list is the dispatcher's invocation contract — it
    // cannot be renamed or split into two functions — even though the boolean
    // is dispatched on inside an `if`. Exact reproducer shape from the issue.
    #[test]
    fn no_fp_inline_callback_argument_param_issue_5639() {
        let src = "function vlookup(ast: any, state: any) {\
                     return this.runFunction(ast.args, state, this.metadata('VLOOKUP'),\
                       (key: any, rangeValue: any, index: number, sorted: boolean) => {\
                         if (sorted) { return binary(); } else { return linear(); }\
                       });\
                   }";
        assert!(run(src).is_empty(), "got {:#?}", run(src));
    }

    // A boolean param of an inline arrow callback dispatched on in a branch is
    // exempt — the callback signature is the caller's contract.
    #[test]
    fn allows_boolean_in_inline_arrow_callback_argument() {
        let src = "run((x: number, cumulative: boolean) => \
                     cumulative ? cdf(x) : pdf(x));";
        assert!(run(src).is_empty(), "got {:#?}", run(src));
    }

    // A boolean param of an inline function-expression callback argument is
    // exempt for the same reason.
    #[test]
    fn allows_boolean_in_inline_function_expression_argument() {
        let src = "register(function (cumulative: boolean) { \
                     if (cumulative) a(); else b(); });";
        assert!(run(src).is_empty(), "got {:#?}", run(src));
    }

    // Guard: a NAMED function declaration whose body branches on a boolean is
    // still author-owned and splittable — it must stay flagged even if it is
    // later passed somewhere.
    #[test]
    fn still_flags_named_function_with_boolean_branch() {
        assert_eq!(
            run("function compute(cumulative: boolean) { if (cumulative) a(); else b(); }").len(),
            1
        );
    }

    // Guard: a callback nested inside an object-literal argument is NOT a direct
    // call argument — the boolean flag still fires (object-property callbacks
    // are handled by the separate library-callback allowlist, not this gate).
    #[test]
    fn still_flags_boolean_in_callback_inside_object_arg() {
        let src = "configure({ handler: (urgent: boolean) => { if (urgent) a(); else b(); } });";
        assert_eq!(run(src).len(), 1, "got {:#?}", run(src));
    }

    // Regression for #6214: a class setter's single parameter is the value being
    // assigned, not a mode flag — it cannot be split into two named functions.
    // Exact reproducer shape from sindresorhus/got.
    #[test]
    fn no_fp_class_setter_param_issue_6214() {
        let src = "class Options {\
                     set decompress(value: boolean) {\
                       assert.boolean(value);\
                       this.#internals.decompress = value;\
                     }\
                   }";
        assert!(run(src).is_empty(), "got {:#?}", run(src));
    }

    // Guard: a regular method with a boolean flag parameter must STILL flag —
    // only setters are exempt, not ordinary methods.
    #[test]
    fn still_flags_boolean_flag_param_in_regular_method_issue_6214() {
        assert_eq!(
            run("class Renderer { render(html: string, pretty: boolean) {} }").len(),
            1,
        );
        assert_eq!(run("function f(flag: boolean) { if (flag) a(); else b(); }").len(), 1);
    }

    // Regression for #6476: a boolean parameter that appears only in a TypeScript
    // overload signature (a `function` declaration with `body: None`) is a pure
    // type-level construct with no runtime branching — "split into two named
    // functions" is meaningless for a bodyless signature, so it must not be
    // flagged. The implementation here uses `HasProtocolOptions | boolean` (a
    // union, not `boolean`), so nothing is flagged. Exact reproducer from the issue.
    #[test]
    fn no_fp_boolean_param_in_overload_signature_issue_6476() {
        let src = "export function hasProtocol(\
                     inputString: string,\
                     opts?: HasProtocolOptions,\
                   ): boolean;\
                   export function hasProtocol(\
                     inputString: string,\
                     acceptRelative: boolean,\
                   ): boolean;\
                   export function hasProtocol(\
                     inputString: string,\
                     acceptRelative?: HasProtocolOptions | boolean,\
                   ): boolean { return true; }";
        assert!(run(src).is_empty(), "got {:#?}", run(src));
    }

    // Guard: an arrow function with a plain boolean mode flag has a body and is a
    // runtime function — it stays flagged.
    #[test]
    fn still_flags_boolean_flag_param_in_arrow() {
        assert_eq!(run("const send = (urgent: boolean) => {};").len(), 1);
    }
}
