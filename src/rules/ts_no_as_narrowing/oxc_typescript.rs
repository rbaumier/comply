//! ts-no-as-narrowing OxcCheck backend — forbid `as` used to narrow types.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, is_inside_type_predicate_fn, is_outer_as_unknown_double_cast,
    name_is_generic_type_param_in_scope, operand_is_typed_as_generic_param,
    resolves_to_branded_primitive,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSType, TSTypeName};
use oxc_span::GetSpan;

pub struct Check;

/// Generic utility types whose `<…>` application NARROWS a value out of a
/// broader type by filtering its members (`NonNullable<T>` drops null/undefined,
/// `Exclude`/`Extract` filter a union, `Pick` filters keys, `Required` filters
/// `undefined` off optional props) — a runtime type-predicate / `in` / `typeof`
/// check is the checkable alternative this rule recommends. The intrinsic
/// string-transformation types (`Uppercase`/`Lowercase`/`Capitalize`/
/// `Uncapitalize`) are deliberately absent: they MAP each string to a
/// transformed string rather than filtering a set, so no runtime check yields
/// their result (and for a generic argument the transformation is uninferable,
/// making the `as` the only viable annotation) — casting to them is structural,
/// not a narrowing smell.
const NARROWING_UTILITY_TYPES: &[&str] = &[
    "NonNullable",
    "Exclude",
    "Extract",
    "Required",
    "Readonly",
    "Pick",
];

/// The standardized DOM event interfaces. Listed explicitly rather than matched
/// by a bare `*Event` suffix: a suffix would also exempt user-defined domain
/// events (`OrderCreatedEvent`, `DomainEvent`), whose `as`-cast IS the narrowing
/// this rule should catch. The set grows only as new event interfaces are
/// standardized; keeping it explicit is the deliberate trade — a not-yet-listed
/// interface flags until added, but no user-defined `*Event` is ever masked.
const DOM_EVENT_INTERFACES: &[&str] = &[
    "Event",
    "UIEvent",
    "CustomEvent",
    "MouseEvent",
    "PointerEvent",
    "WheelEvent",
    "DragEvent",
    "KeyboardEvent",
    "InputEvent",
    "CompositionEvent",
    "TouchEvent",
    "FocusEvent",
    "ClipboardEvent",
    "SubmitEvent",
    "FormDataEvent",
    "BeforeUnloadEvent",
    "AnimationEvent",
    "TransitionEvent",
    "ToggleEvent",
    "PopStateEvent",
    "HashChangeEvent",
    "PageTransitionEvent",
    "StorageEvent",
    "MessageEvent",
    "CloseEvent",
    "ErrorEvent",
    "PromiseRejectionEvent",
    "ProgressEvent",
    "SecurityPolicyViolationEvent",
    "DeviceMotionEvent",
    "DeviceOrientationEvent",
    "MediaQueryListEvent",
    "GamepadEvent",
];

/// Built-in web-platform interface types whose `as` cast is an idiomatic DOM
/// narrowing, not a smell. The DOM and File System APIs hand back a broad
/// supertype — DOM queries return `HTMLElement | null` / `Element | null`,
/// `EventTarget.target` is `EventTarget | null`, a listener's argument is the
/// base `Event`, a directory reader yields `FileSystemEntry` — so casting to the
/// concrete platform interface to reach interface-specific members is the
/// standard pattern (`instanceof` is equivalent but verbose). Covers:
/// - DOM element interfaces: bare `Element`, and `HTML*Element` / `SVG*Element` /
///   `MathML*Element`.
/// - Base DOM tree interfaces: `Node`, `EventTarget`, `ShadowRoot`,
///   `DocumentFragment`. (`Document` and `Window` are intentionally omitted:
///   they are commonly shadowed by user types — e.g. a MongoDB/Prisma
///   `Document` — so exempting them risks masking real narrowings, and the
///   reported false positives do not need them.)
/// - DOM event interfaces: the enumerated `DOM_EVENT_INTERFACES` (`MouseEvent`,
///   `KeyboardEvent`, `DragEvent`, `ClipboardEvent`, …).
/// - File System API entries: `FileSystem*Entry` (`FileSystemFileEntry`,
///   `FileSystemDirectoryEntry`, the base `FileSystemEntry`).
fn is_dom_interface_type(name: &str) -> bool {
    name == "Element"
        || ((name.starts_with("HTML") || name.starts_with("SVG") || name.starts_with("MathML"))
            && name.ends_with("Element"))
        || matches!(name, "Node" | "EventTarget" | "ShadowRoot" | "DocumentFragment")
        || DOM_EVENT_INTERFACES.contains(&name)
        || (name.starts_with("FileSystem") && name.ends_with("Entry"))
}

/// Whether the cast operand is a freshly-constructed value: an object literal
/// (`{} as RouteModules`), an array literal (`[1, 2] as ReadonlyArray<number>`),
/// a primitive literal (`"idle" as RevalidationState`), or a `new` expression
/// (`new String(value) as SafeHtml`). Casting such an operand is a
/// construction-time type ascription, not a narrowing of a pre-existing binding
/// — there is no variable to refine with a type predicate or `in`/`typeof`
/// check, so the rule's remediation does not apply.
fn operand_is_constructed_value(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::ObjectExpression(_)
            | Expression::ArrayExpression(_)
            | Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::TemplateLiteral(_)
            | Expression::BigIntLiteral(_)
            | Expression::RegExpLiteral(_)
            | Expression::NewExpression(_)
    )
}

/// Whether the cast operand is one of the global objects (`globalThis`,
/// `window`, `self`, `global`). `(globalThis as Interface).prop` is the
/// canonical idiom for augmenting a global object with a custom property: the
/// cast ADDS a property the base global type does not structurally declare, the
/// opposite of narrowing. No type predicate / `in` / `typeof` guard can make TS
/// believe a global has a property it doesn't declare, so the rule's
/// remediation does not apply. Parentheses are peeled so `(globalThis as T)` is
/// treated identically to `globalThis as T`.
fn operand_is_global_object(expr: &Expression) -> bool {
    matches!(
        expr.without_parentheses(),
        Expression::Identifier(id)
            if matches!(id.name.as_str(), "globalThis" | "window" | "self" | "global")
    )
}

fn target_is_narrowing(ty: &TSType, semantic: &oxc_semantic::Semantic) -> bool {
    match ty {
        TSType::TSLiteralType(_) | TSType::TSTemplateLiteralType(_) => true,
        TSType::TSTypeReference(r) => {
            let TSTypeName::IdentifierReference(id) = &r.type_name else { return false };
            let name = id.name.as_str();
            if r.type_arguments.is_some() {
                // Generic utility type like `NonNullable<T>`.
                NARROWING_UTILITY_TYPES.contains(&name)
            } else if is_dom_interface_type(name) {
                false
            } else if resolves_to_branded_primitive(id, semantic) {
                // Branded / opaque primitive (`string & { __brand }`): the brand
                // is a compile-time-only phantom with no runtime representation,
                // so no `typeof`/`in`/type-predicate can mint it. The `as` cast
                // is a construction-time ascription, not a refinable narrowing.
                false
            } else {
                // PascalCase identifier — likely a user-defined narrowing type.
                name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            }
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAsExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSAsExpression(as_expr) = node.kind() else {
            return;
        };

        // Tests cast runtime values after a runtime guard
        // (`expect(x).toBeInstanceOf(Foo); (x as Foo).field`) — the assertion is
        // backed by the guard, not standing in for narrowing. Skip test files.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        // Skip `as const`.
        let type_text = &ctx.source
            [as_expr.type_annotation.span().start as usize..as_expr.type_annotation.span().end as usize];
        if type_text.trim() == "const" {
            return;
        }

        // Skip type-ascriptions of a freshly-constructed operand (`{} as T`,
        // `[1, 2] as T`, `"idle" as Union`, `new String(v) as SafeHtml`). These
        // ascribe a type to a value being built inline; there is no pre-existing
        // binding to narrow, so a type predicate / `in` / `typeof` guard cannot
        // apply.
        if operand_is_constructed_value(&as_expr.expression) {
            return;
        }

        // Skip global-object augmentation casts (`(globalThis as T).prop`).
        // Casting a global object to an interface ADDS a property at the type
        // level rather than refining one out of a union, so the rule's
        // type-predicate / `in` / `typeof` remediation cannot apply.
        if operand_is_global_object(&as_expr.expression) {
            return;
        }

        if !target_is_narrowing(&as_expr.type_annotation, semantic) {
            return;
        }

        // Skip `as TParam` when `TParam` is a generic type parameter on an
        // enclosing function/method/class/interface/type alias. These are
        // structural type-bridge casts (e.g. TanStack Router's
        // `useSearch() as TSearch`), not narrowings.
        if let TSType::TSTypeReference(r) = &as_expr.type_annotation
            && r.type_arguments.is_none()
        {
            let TSTypeName::IdentifierReference(id) = &r.type_name else { return };
            let name = id.name.as_str();
            if name_is_generic_type_param_in_scope(name, node.id(), semantic) {
                return;
            }
        }

        // Skip casts whose operand is statically typed as an enclosing
        // function's generic type parameter (e.g. viem's `parseTransaction`
        // casting `serializedTransaction: serialized` to a concrete branch type
        // after a value-level discriminant check). TypeScript will not reduce
        // the generic type parameter from a runtime check, and a type predicate
        // narrows only the local type, not the generic — so the `as` is the only
        // way to bridge the generic to the concrete type.
        if operand_is_typed_as_generic_param(&as_expr.expression, node.id(), semantic) {
            return;
        }

        // Skip the outer half of `x as unknown as T` — the canonical
        // contravariant-boundary escape hatch (e.g. Drizzle relational types
        // invariant in `TablesRelationalConfig`). The inner cast must be to
        // the `unknown` keyword specifically; `x as Foo as Bar` is NOT
        // exempted. This rule exempts ONLY the outer half (not the inner
        // `as unknown`); parentheses are peeled so `(x as unknown) as T` is
        // treated identically to `x as unknown as T`.
        if is_outer_as_unknown_double_cast(as_expr) {
            return;
        }

        // Skip `as` casts inside the body of a type-predicate function
        // (`value is T`). That function IS the custom type guard this rule
        // recommends; the cast is needed to read properties off the
        // loosely-typed input, so flagging it would be circular advice.
        if is_inside_type_predicate_fn(node.id(), semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Avoid using `as` to narrow types; use a type predicate or `in`/`typeof` check.".into(),
            severity: Severity::Error,
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
    fn flags_pascal_user_type() {
        assert_eq!(run_on("const x = value as AdminUser;").len(), 1);
    }

    #[test]
    fn allows_guarded_cast_in_test_files() {
        // Regression for issue #573: assertion after a runtime `instanceof` guard.
        use crate::rules::file_ctx::{FileCtx, PathSegments};
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        assert!(
            crate::rules::test_helpers::run_rule_with_ctx(&Check, "const c = (apiError as InternalError).cause;", "t.tsx", crate::project::default_static_project_ctx(), &file)
            .is_empty()
        );
    }

    #[test]
    fn flags_literal_target() {
        assert_eq!(run_on("const x = val as 'foo';").len(), 1);
    }

    #[test]
    fn allows_generic_type_param_in_function() {
        // Regression for #114: `as TSearch` where `<TSearch>` is on the
        // enclosing function is a structural type bridge, not a narrowing.
        let src = "function useTypedSearch<TSearch>(api: { useSearch: () => unknown }) {\n\
                   const search = api.useSearch() as TSearch;\n\
                   return search;\n}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_generic_type_param_in_class_method() {
        let src = "class Wrap<T> { unwrap(v: unknown) { return v as T; } }";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn flags_pascal_cased_when_not_generic_param() {
        let diags = run_on("function f() { return x as MyType; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_outer_cast_of_as_unknown_as_chain_drizzle_repro() {
        // Regression for #178: Drizzle's relational types are invariant in
        // `TablesRelationalConfig`, so a structural relabel of a deeply-
        // generic filter requires `as unknown as <Type>`. The outer half
        // must not be flagged as a narrowing.
        let src = "type AnyRelationsFilter = unknown;\n\
                   declare const where: object;\n\
                   const widenedWhere = where as unknown as AnyRelationsFilter;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_simple_as_unknown_as_t() {
        let diags = run_on("const y = x as unknown as Foo;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn flags_single_cast_to_pascal_type() {
        // Negative: a plain `x as Foo` is still a narrowing.
        let diags = run_on("const y = x as Foo;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_double_cast_without_unknown_middle() {
        // Negative: `x as any as Foo` is NOT the canonical escape hatch —
        // the middle must be `unknown` for the exemption to apply. The
        // outer cast (target `Foo`) must still flag as a narrowing.
        let diags = run_on("const y = x as any as Foo;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_parenthesised_unknown_in_double_cast() {
        // Issue #178 follow-up — `(x as unknown) as Foo` is semantically
        // identical to `x as unknown as Foo`.
        let src = "declare const x: unknown; const y = (x as unknown) as Foo;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_dom_element_subtype_cast() {
        // Regression for #1080: DOM query APIs return the broad
        // `HTMLElement | null`, so casting to a specific element interface is
        // the idiomatic refinement, not a narrowing smell.
        let src = "let secretDisplay: HTMLSpanElement = document.getElementById(\"secret-display\") as HTMLSpanElement;\n\
                   let secretButton: HTMLButtonElement = document.getElementById(\"secret-button\") as HTMLButtonElement;\n\
                   const el = document.querySelector(\".foo\") as HTMLInputElement;\n\
                   const svg = node as SVGPathElement;\n\
                   const generic = root as Element;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_non_dom_pascal_type() {
        // Control: a user-defined PascalCase target is still a narrowing,
        // even though it superficially resembles a DOM type name.
        let diags = run_on("const x = value as AdminElement;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_dom_base_interface_event_and_filesystem_casts() {
        // Regression for #6897: DOM/File System APIs hand back a broad supertype
        // (`EventTarget | null`, base `Event`, `FileSystemEntry`), so casting to
        // the concrete platform interface is the idiomatic narrowing, not a smell.
        let src = "const ok = el?.contains(e.target as Node);\n\
                   directive(e as MouseEvent, el, binding);\n\
                   const ev = e as Event;\n\
                   const t = (e as DragEvent).dataTransfer ?? (e as ClipboardEvent).clipboardData ?? null;\n\
                   const fileEntry = item as FileSystemFileEntry;\n\
                   const dirEntry = item as FileSystemDirectoryEntry;\n\
                   const baseEntry = item as FileSystemEntry;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_document_and_window_cast() {
        // Control for #6897: `Document`/`Window` are deliberately NOT recognized
        // as DOM interfaces (they are commonly shadowed by user types — e.g. a
        // MongoDB/Prisma `Document`), so casting to them still flags.
        assert_eq!(run_on("const d = x as Document;").len(), 1);
        assert_eq!(run_on("const w = x as Window;").len(), 1);
    }

    #[test]
    fn still_flags_non_dom_app_type_narrowing() {
        // Control for #6897: an arbitrary app type with no DOM lineage is still a
        // genuine narrowing the rule must catch — the broadened DOM recognizer
        // must not suppress it.
        assert_eq!(run_on("const c = data as UserConfig;").len(), 1);
        assert_eq!(run_on("const v = value as SomeAppType;").len(), 1);
    }

    #[test]
    fn still_flags_non_dom_event_type() {
        // Control for #6897: DOM event interfaces are enumerated, not matched by a
        // bare `*Event` suffix, so a user-defined domain event still flags as a
        // narrowing — `as`-casting it is exactly what the rule should catch.
        assert_eq!(run_on("const p = msg as PurchaseEvent;").len(), 1);
        assert_eq!(run_on("const d = raw as DomainEvent;").len(), 1);
    }

    #[test]
    fn still_flags_triple_parenthesised_as_chain() {
        // `((x as A) as unknown) as B` — the middle isn't a plain `as unknown`
        // of the original value; the inner `as A` is the suspect cast. We
        // don't auto-exempt arbitrary triple casts.
        let src = "declare const x: unknown; const y = ((x as A) as unknown) as B;";
        let diags = run_on(src);
        assert!(!diags.is_empty(), "expected at least one diag for inner `as A` cast");
    }

    #[test]
    fn allows_as_in_arrow_type_predicate_body() {
        // Regression for #1976: an arrow whose return type is a type predicate
        // (`api is WithDispatch`) IS the custom type guard this rule
        // recommends; the `as` casts in its body read properties off the
        // `unknown` input and must not be flagged.
        let src = "const shouldDispatchFromDevtools = (api: unknown): api is WithDispatch =>\n\
                   !!(api as WithDispatch).dispatchFromDevtools &&\n\
                   typeof (api as WithDispatch).dispatch === 'function';";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_as_in_function_type_predicate_body() {
        // Regression for #1976: a `function isFoo(x): x is Foo` guard body.
        let src = "function isFoo(x: unknown): x is Foo { return (x as Foo).bar !== undefined; }";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_as_in_non_predicate_function() {
        // Control for #1976: the same cast in a function WITHOUT a
        // type-predicate return type is still a narrowing and must fire.
        let src = "function f(x: unknown) { return (x as Foo).bar; }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1, "expected one diag: {:?}", diags);
    }

    #[test]
    fn allows_object_literal_ascription() {
        // Regression for #3875: `{} as T` seeds a typed accumulator; the
        // object literal is constructed inline, so there is no binding to
        // narrow with a type predicate.
        let diags = run_on("const seed = {} as RouteModules;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_object_spread_literal_ascription() {
        // Regression for #3875: a spread object literal is still a freshly
        // constructed value, not a pre-existing binding.
        let diags = run_on("const l = { ...link, rel: \"prefetch\", as: \"style\" } as Link;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_string_literal_ascription() {
        // Regression for #3875: `"idle" as Union` ascribes a union member to a
        // primitive literal; there is no variable to refine.
        let diags = run_on("const r = \"idle\" as RevalidationState;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_array_literal_ascription() {
        // Regression for #3875: `[1, 2] as ReadonlyArray<number>` ascribes a
        // type to a freshly constructed array literal.
        let diags = run_on("const a = [1, 2] as ReadonlyArray<number>;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_identifier_operand_narrowing() {
        // Control for #3875: an Identifier operand IS a pre-existing binding;
        // `existingVar as NarrowType` is a genuine narrowing and must fire.
        let diags = run_on("const x = existingVar as NarrowType;");
        assert_eq!(diags.len(), 1, "expected one diag: {:?}", diags);
    }

    #[test]
    fn still_flags_member_expression_operand_narrowing() {
        // Control for #3875: a member-expression operand reads a pre-existing
        // value; `foo.bar as T` is still a narrowing.
        let diags = run_on("const x = foo.bar as Narrowed;");
        assert_eq!(diags.len(), 1, "expected one diag: {:?}", diags);
    }

    #[test]
    fn allows_global_this_augmentation_cast() {
        // Regression for #3837: `(globalThis as Interface)` is the canonical
        // idiom for augmenting `globalThis` with a custom global property — the
        // cast ADDS a property at the type level (the opposite of narrowing).
        // No type predicate / `in` / `typeof` guard can make TS believe
        // `globalThis` has a property it doesn't declare.
        let src = "(globalThis as GlobalWithRegistry).__myRegistry ??= new Map();\n\
                   export const reg = (globalThis as GlobalWithRegistry).__myRegistry!;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_window_self_global_augmentation_casts() {
        // Regression for #3837: `window`/`self`/`global` are the other global
        // objects that the same augmentation idiom applies to.
        assert!(run_on("const a = (window as Foo).x;").is_empty());
        assert!(run_on("const b = (self as Foo).x;").is_empty());
        assert!(run_on("const c = (global as Foo).x;").is_empty());
    }

    #[test]
    fn still_flags_regular_variable_operand_narrowing() {
        // Control for #3837: a regular variable identifier operand (not a
        // global object) is a genuine narrowing and must still fire.
        let diags = run_on("const y = x as SpecificType;");
        assert_eq!(diags.len(), 1, "expected one diag: {:?}", diags);
    }

    #[test]
    fn allows_cast_of_generic_param_typed_argument() {
        // Regression for #4822: viem's `parseTransaction` casts its
        // `serializedTransaction: serialized` argument (typed as the function's
        // own generic parameter) to a concrete branch type after a value-level
        // discriminant check. TypeScript cannot reduce the generic from a
        // runtime check, so the `as` is unavoidable — not a narrowing smell.
        let src = "function parseTransaction<serialized extends CeloTransactionSerialized>(\n\
                   serializedTransaction: serialized,\n\
                   ): unknown {\n\
                   const serializedType = sliceHex(serializedTransaction, 0, 1);\n\
                   if (serializedType === '0x7c')\n\
                   return parseTransactionCIP42(serializedTransaction as TransactionSerializedCIP42);\n\
                   return parseTransaction_op(serializedTransaction as OpStackTransactionSerialized);\n}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_cast_of_generic_param_typed_arg_in_class_method() {
        // #4822: same pattern with the generic parameter on the enclosing class.
        let src = "class Parser<T extends Base> {\n\
                   parse(raw: T) { return decode(raw as Concrete); }\n}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_cast_of_concretely_typed_argument() {
        // Control for #4822: when the operand is typed as a concrete type
        // (not a generic parameter), a type guard CAN narrow it, so the cast
        // is a genuine narrowing and must still fire.
        let src = "function f(x: unknown) { return x as Concrete; }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1, "expected one diag: {:?}", diags);
    }

    #[test]
    fn allows_intrinsic_string_transform_types() {
        // Regression for #6538: scule's `upperFirst`/`lowerFirst` cast to
        // `Capitalize<S>`/`Uncapitalize<S>` where `S extends string` is the
        // function's generic parameter. The intrinsic string-transformation
        // types map each string to a transformed string; for a generic argument
        // TypeScript cannot evaluate the result, so the `as` is the only
        // annotation and no runtime type-predicate / `in` / `typeof` check can
        // yield it. These four are not narrowings.
        let cap = "export function upperFirst<S extends string>(str: S): Capitalize<S> {\n\
                   return (str ? str[0].toUpperCase() + str.slice(1) : \"\") as Capitalize<S>;\n}";
        assert!(run_on(cap).is_empty(), "unexpected diags: {:?}", run_on(cap));
        let uncap = "export function lowerFirst<S extends string>(str: S): Uncapitalize<S> {\n\
                     return (str ? str[0].toLowerCase() + str.slice(1) : \"\") as Uncapitalize<S>;\n}";
        assert!(run_on(uncap).is_empty(), "unexpected diags: {:?}", run_on(uncap));
        let up = "function up<S extends string>(s: S) { return s.toUpperCase() as Uppercase<S>; }";
        assert!(run_on(up).is_empty(), "unexpected diags: {:?}", run_on(up));
        let low = "function low<S extends string>(s: S) { return s.toLowerCase() as Lowercase<S>; }";
        assert!(run_on(low).is_empty(), "unexpected diags: {:?}", run_on(low));
    }

    #[test]
    fn still_flags_genuine_narrowing_utility_types() {
        // Control for #6538: filter-based narrowing utilities still flag — they
        // narrow a value out of a broader type and a runtime check is the
        // checkable alternative, unlike the intrinsic string-transformation
        // types that #6538 exempted.
        assert_eq!(run_on("const a = x as NonNullable<T>;").len(), 1);
        assert_eq!(run_on("const b = x as Exclude<A, B>;").len(), 1);
    }

    #[test]
    fn still_flags_cast_of_destructured_generic_param_element() {
        // Control for #4822: a destructured element of a generic-typed binding
        // (`{ a }: T`) has type `T["a"]`, not `T` — casting it is a genuine
        // narrowing, so the bare-identifier guard must keep it flagged.
        let src = "function f<T extends Base>({ a }: T) { return a as Concrete; }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1, "expected one diag: {:?}", diags);
    }

    #[test]
    fn allows_new_expression_operand_ascription() {
        // Regression for #6967: `new String(value) as SafeHtml` constructs a
        // fresh branded wrapper inline; there is no pre-existing binding to
        // narrow with a type predicate, so the `new` operand must not be flagged.
        let diags = run_on("const s = new String(value) as SafeHtml;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_new_expression_operand_with_user_target() {
        // #6967: a `new` expression cast to any user narrowing target is still a
        // freshly-constructed value, not a refinement of an existing variable.
        let diags = run_on("const m = new Map() as TypedRegistry;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_call_expression_operand_narrowing() {
        // Control for #6967: only `new` constructs a fresh value here. A plain
        // call's result can be a pre-existing reference, so `makeThing() as Foo`
        // is still a narrowing and must fire.
        let diags = run_on("const t = makeThing() as Foo;");
        assert_eq!(diags.len(), 1, "expected one diag: {:?}", diags);
    }

    #[test]
    fn still_flags_identifier_operand_with_new_fix() {
        // Control for #6967: an identifier binding is a pre-existing value;
        // `foo as Bar` remains a genuine narrowing after the new-expression fix.
        let diags = run_on("const x = foo as Bar;");
        assert_eq!(diags.len(), 1, "expected one diag: {:?}", diags);
    }

    #[test]
    fn still_flags_parenthesised_member_operand_with_new_fix() {
        // Control for #6967: a member access on a pre-existing value is a real
        // narrowing; `(obj.prop) as Baz` must still fire.
        let diags = run_on("const y = (obj.prop) as Baz;");
        assert_eq!(diags.len(), 1, "expected one diag: {:?}", diags);
    }

    #[test]
    fn allows_direct_branded_primitive_target() {
        // Regression for #7733: `type Brand = string & { __brand: 'x' }` is a
        // branded / opaque primitive. `__brand` is a compile-time-only phantom,
        // so no `typeof`/`in`/type-predicate can mint it — the `as` is the only
        // way to construct the brand, not a narrowing of a refinable binding.
        let src = "type Brand = string & { __brand: 'x' };\n\
                   const s = \"a\";\n\
                   const b = s as Brand;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_generic_opaque_helper_target() {
        // Regression for #7733: the `Opaque<K, T> = T & { __brand: K }` helper
        // instantiated as `Opaque<'Key', string>` resolves to a branded string
        // once the primitive type-argument substitutes into `T`.
        let src = "type Opaque<K, T> = T & { __brand: K };\n\
                   type Key = Opaque<'Key', string>;\n\
                   function f(x: string) { return x as Key; }";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_branded_primitive_with_concatenation_operand() {
        // Regression for #7733: the discriminator is the cast TARGET (a brand),
        // not the operand — a `+` concatenation operand must not resurrect the
        // diagnostic (`(a + '/' + b) as SegmentRequestKey`).
        let src = "type Opaque<K, T> = T & { __brand: K };\n\
                   type Key = Opaque<'Key', string>;\n\
                   function f(a: string, b: string) { return (a + '/' + b) as Key; }";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_number_and_boolean_branded_primitives() {
        // Regression for #7733: brands over `number`/`boolean` carriers are the
        // same phantom-brand idiom as string brands.
        let num = "type Nb = number & { __b: 'n' };\n\
                   function f(n: number) { return n as Nb; }";
        assert!(run_on(num).is_empty(), "unexpected diags: {:?}", run_on(num));
        let boo = "type Bb = boolean & { __b: 'b' };\n\
                   function g(v: boolean) { return v as Bb; }";
        assert!(run_on(boo).is_empty(), "unexpected diags: {:?}", run_on(boo));
    }

    #[test]
    fn still_flags_literal_union_alias_target() {
        // Control for #7733: an alias resolving to a literal union is a genuine
        // narrowing (no intersection, no primitive keyword) and must keep firing.
        let src = "type U = 'a' | 'b';\n\
                   function f(x: string) { return x as U; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_object_only_intersection_target() {
        // Control for #7733: an intersection of two object types has no
        // primitive-keyword member, so it is not a branded primitive — the
        // branded-primitive gate must not fire and the cast still flags.
        let src = "type A = { a: number };\n\
                   type B = { b: number };\n\
                   type AB = A & B;\n\
                   function f(x: unknown) { return x as AB; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_interface_target() {
        // Control for #7733: a plain interface target is not a branded primitive;
        // casting to it remains a narrowing.
        let src = "interface Config { a: number }\n\
                   function f(x: unknown) { return x as Config; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_opaque_helper_with_object_type_argument() {
        // Control for #7733: the generic-helper path requires the substituted
        // type-argument to be a primitive keyword. `Opaque<'K', { a: number }>`
        // substitutes an object type into `T`, so the intersection carries no
        // primitive member — it is not a branded primitive and still flags.
        let src = "type Opaque<K, T> = T & { __brand: K };\n\
                   type Boxed = Opaque<'K', { a: number }>;\n\
                   function f(x: unknown) { return x as Boxed; }";
        assert_eq!(run_on(src).len(), 1);
    }
}
