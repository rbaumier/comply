//! ts-prefer-satisfies oxc backend — flag `{...} as T` / `[...] as T`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    ArrayExpression, ArrayExpressionElement, Expression, ObjectExpression, ObjectPropertyKind,
    PropertyKey, TSType,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAsExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSAsExpression(as_expr) = node.kind() else { return };

        // Test stubs cast partial literals to full library types where
        // `satisfies` is impossible (the target has required fields the stub
        // omits) — skip test files.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        // Only flag when the value side is an object or array literal.
        let is_literal = matches!(
            &as_expr.expression,
            Expression::ObjectExpression(_) | Expression::ArrayExpression(_)
        );
        if !is_literal {
            return;
        }

        // Empty literals are exempt: `satisfies` cannot supply the widening
        // their `as` provides.
        //   - `{} satisfies T` fails to compile (TS1360) when `T` has required
        //     members, so `{} as T` is the canonical deliberate-coercion idiom
        //     (default/fallback props, empty option bags).
        //   - `[]` infers as `never[]`; `[] as T[]` widens it to `T[]` so the
        //     value can seed a typed accumulator or accept later `.push(t)`.
        //     `[] satisfies T[]` compiles but leaves the type `never[]`, losing
        //     the widening — the `as` is semantically irreplaceable.
        let is_empty_literal = match &as_expr.expression {
            Expression::ObjectExpression(obj) => obj.properties.is_empty(),
            Expression::ArrayExpression(arr) => arr.elements.is_empty(),
            _ => false,
        };
        if is_empty_literal {
            return;
        }

        // A non-literal computed key (`{ [dynamicVar]: v }`) gives the object an
        // implicit string index signature; `satisfies T` rejects that signature
        // when `T` has no index signature, so the `as T` cast cannot be
        // mechanically replaced. An array inherits this from any direct
        // object-literal element (`[{ [k]: v }] as T[]`).
        let has_non_literal_computed = match &as_expr.expression {
            Expression::ObjectExpression(obj) => has_non_literal_computed_key(obj),
            Expression::ArrayExpression(arr) => arr.elements.iter().any(|el| {
                matches!(
                    el.as_expression(),
                    Some(Expression::ObjectExpression(obj)) if has_non_literal_computed_key(obj)
                )
            }),
            _ => false,
        };
        if has_non_literal_computed {
            return;
        }

        // `[...expr] as T` where `expr` is not an array literal is a *narrowing*
        // assertion, not a widening one. TypeScript infers the spread's element
        // type from `expr`, which can be broader than `T` — e.g.
        // `[...new Set(xs.filter(Boolean))] as string[]`, whose inferred element
        // type is `(string | false | undefined)[]` because `.filter(Boolean)`
        // is not narrowed. `satisfies string[]` would then fail to compile, so
        // the `as` cannot be mechanically replaced. A spread of an array literal
        // (`[...[1, 2], 3]`) has a known element type and stays in scope.
        if let Expression::ArrayExpression(arr) = &as_expr.expression
            && has_non_literal_spread(arr)
        {
            return;
        }

        // `{ ...expr } as T` where `expr` is not an object/array/string literal
        // is a *narrowing* assertion, not a widening one — the object-literal
        // analogue of the array-spread case above. TypeScript infers the
        // spread's contributed value type from `expr`, which can be broader than
        // `T` — e.g. `{ ...process.env } as Record<string, string>`, where
        // `process.env` is `{ [k: string]: string | undefined }`, so the cast
        // strips `undefined`. `satisfies T` would then fail to compile, so the
        // `as` cannot be mechanically replaced. A spread of an object literal
        // (`{ ...{ a: 1 }, b: 2 }`) has a known type and stays in scope.
        if let Expression::ObjectExpression(obj) = &as_expr.expression
            && has_non_literal_object_spread(obj)
        {
            return;
        }

        // `satisfies unknown` / `satisfies any` are vacuously true — every
        // value satisfies `unknown`/`any`, so the suggestion validates
        // nothing. `literal as unknown` / `literal as any` is a deliberate
        // escape hatch (often the first half of an `as unknown as T`
        // double-assertion), not a widening that `satisfies` can replace.
        if matches!(
            as_expr.type_annotation,
            TSType::TSUnknownKeyword(_) | TSType::TSAnyKeyword(_)
        ) {
            return;
        }

        // Filter out `as const`.
        let type_text = &ctx.source[as_expr.type_annotation.span().start as usize..as_expr.type_annotation.span().end as usize];
        if type_text.trim() == "const" {
            return;
        }

        // `as React.CSSProperties` on an object containing CSS custom property
        // keys (`--*`) is the documented workaround: @types/react removed the
        // index signature, so `satisfies React.CSSProperties` would fail to
        // compile when any key starts with `--`.
        if type_text.trim() == "React.CSSProperties" {
            if let Expression::ObjectExpression(obj) = &as_expr.expression {
                let has_css_custom_prop = obj.properties.iter().any(|prop| {
                    if let ObjectPropertyKind::ObjectProperty(p) = prop {
                        p.key.static_name().is_some_and(|k| k.starts_with("--"))
                    } else {
                        false
                    }
                });
                if has_css_custom_prop {
                    return;
                }
            }
        }

        // `as PropType<T>` is Vue's branded prop-type marker. A runtime
        // constructor value (`String`, or an array of them like
        // `[String, Object]`) never structurally satisfies `PropType<T>`, so
        // `satisfies PropType<T>` fails to compile and the `as` cast is the
        // required idiom. The `<` immediately following the name keeps the match
        // on the exact `PropType` reference, not a longer identifier that begins
        // with it (`PropTypeFoo<…>`).
        if type_text.trim().starts_with("PropType<") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`as Type` on a literal widens the inferred type — use `satisfies Type` to validate without widening.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when the object literal has a computed property key that is not a
/// string/number literal (`{ [expr]: v }` where `expr` is an identifier, member
/// expression, substituting template literal, …). Such a key makes TypeScript
/// infer an implicit string index signature, which `satisfies T` rejects against
/// a target lacking an index signature. A literal computed key (`{ ['foo']: 1 }`,
/// `{ [0]: 1 }`) names a known property and is unaffected.
fn has_non_literal_computed_key(obj: &ObjectExpression<'_>) -> bool {
    obj.properties.iter().any(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { return false };
        p.computed && !matches!(p.key, PropertyKey::StringLiteral(_) | PropertyKey::NumericLiteral(_))
    })
}

/// True when the array literal spreads a non-array-literal expression
/// (`[...call()]`, `[...ident]`, `[...obj.prop]`, …). TypeScript infers the
/// spread's element type from that expression, which may be broader than the
/// `as` target, making the cast a narrowing assertion that `satisfies` cannot
/// reproduce. Spreading an array literal (`[...[1, 2]]`) has a known element
/// type and does not trigger this.
fn has_non_literal_spread(arr: &ArrayExpression<'_>) -> bool {
    arr.elements.iter().any(|el| {
        matches!(
            el,
            ArrayExpressionElement::SpreadElement(s)
                if !matches!(s.argument, Expression::ArrayExpression(_))
        )
    })
}

/// True when the object literal spreads a non-literal expression
/// (`{ ...ident }`, `{ ...obj.prop }`, `{ ...call() }`, …). TypeScript infers
/// the spread's contributed value type from that expression, which may be
/// broader than the `as` target, making the cast a narrowing assertion that
/// `satisfies` cannot reproduce. Spreading an object/array/string literal
/// (`{ ...{ a: 1 } }`) has a known type and does not trigger this.
fn has_non_literal_object_spread(obj: &ObjectExpression<'_>) -> bool {
    obj.properties.iter().any(|prop| {
        matches!(
            prop,
            ObjectPropertyKind::SpreadProperty(s)
                if !matches!(
                    s.argument,
                    Expression::ObjectExpression(_)
                        | Expression::ArrayExpression(_)
                        | Expression::StringLiteral(_)
                )
        )
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

    fn run_ts(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_object_literal_cast() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const x = { a: 1 } as Config;", "t.ts").len(), 1);
    }

    #[test]
    fn allows_stub_cast_in_test_files() {
        // Regression for issue #573: partial stubs can't use `satisfies`.
        use crate::rules::file_ctx::{FileCtx, PathSegments};
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        assert!(
            crate::rules::test_helpers::run_rule_with_ctx(&Check, "const a = { api: { getSession: async () => null } } as Auth;", "t.tsx", crate::project::default_static_project_ctx(), &file)
            .is_empty()
        );
    }

    #[test]
    fn flags_array_literal_cast() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const y = [1, 2] as Tuple;", "t.ts").len(), 1);
    }

    // Regression test for #3881: an empty object literal cast cannot use
    // `satisfies` (`{} satisfies T` is TS1360 when T has required members),
    // so `{} as T` must not be flagged.
    #[test]
    fn allows_empty_object_literal_cast() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const x = {} as T;", "t.ts").is_empty());
    }

    #[test]
    fn allows_empty_object_fallback_cast() {
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "const o = pOptions || ({} as ResourceOptions<T, S>);", "t.ts")
                .is_empty()
        );
    }

    #[test]
    fn still_flags_non_empty_object_literal_cast() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const x = { a: 1 } as T;", "t.ts").len(), 1);
    }

    // Regression test for #6195: `[]` infers as `never[]`; `[] as T[]` widens
    // it to `T[]` so subsequent `.push()`/accumulation type-checks.
    // `[] satisfies T[]` does not widen (stays `never[]`), so the `as` is
    // irreplaceable — an empty array literal cast must not be flagged.
    #[test]
    fn allows_empty_array_literal_cast() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const a = [] as T[];", "t.ts").is_empty());
    }

    // #6195: empty-array reduce accumulator seed — `as T[]` widens the seed so
    // the accumulator parameter is typed; `satisfies` cannot.
    #[test]
    fn allows_empty_array_reduce_seed() {
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                "const r = data.reduce((acc, d) => { acc.push(d); return acc; }, [] as LocaleObjectData[]);",
                "t.ts",
            )
            .is_empty()
        );
    }

    // #6195: union-element empty array — same widening requirement.
    #[test]
    fn allows_empty_array_union_element_cast() {
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "const children = [] as (Node | string)[];", "t.ts").is_empty()
        );
    }

    // #6195 boundary: only the empty array literal is structurally unusable
    // with `satisfies` (a `never[]` value can accept no element). A non-empty
    // literal carries its own inferred element type, so it stays in scope.
    #[test]
    fn still_flags_non_empty_array_literal_cast() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const a = [x, y] as T[];", "t.ts").len(), 1);
    }

    #[test]
    fn allows_non_literal_cast() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const x = foo as Config;", "t.ts").is_empty());
    }

    #[test]
    fn allows_as_const() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const x = [1, 2] as const;", "t.ts").is_empty());
    }

    // Regression test for #6138: `satisfies unknown`/`satisfies any` are
    // vacuously true, so `literal as unknown` / `literal as any` must not be
    // flagged — these are deliberate escape hatches `satisfies` cannot replace.
    #[test]
    fn allows_array_literal_as_unknown() {
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "const t = [] as unknown as [undefined, undefined];", "t.ts")
                .is_empty()
        );
    }

    #[test]
    fn allows_object_literal_as_any() {
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "const n = { tag: '', props: { children: jsxNode } } as any;", "t.ts")
                .is_empty()
        );
    }

    #[test]
    fn allows_object_literal_as_unknown() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const x = { a: 1 } as unknown;", "t.ts").is_empty());
    }

    // Negative space: a concrete cast target that `satisfies` can validate must
    // still fire even when `any`/`unknown` appear nested inside the type.
    #[test]
    fn still_flags_concrete_type_with_nested_unknown() {
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "const x = { a: 1 } as Record<string, unknown>;", "t.ts").len(),
            1
        );
    }

    #[test]
    fn allows_satisfies() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const x = { a: 1 } satisfies Config;", "t.ts").is_empty());
    }

    // Regression test for #569: `as React.CSSProperties` on an object with
    // CSS custom properties is necessary — `satisfies` would fail to compile
    // because @types/react has no index signature for `--*` keys.
    #[test]
    fn allows_css_custom_props_as_react_css_properties() {
        assert!(crate::rules::test_helpers::run_rule(&Check, r#"import type React from 'react';
const style = { "--my-var": "100px" } as React.CSSProperties;"#, "t.tsx")
        .is_empty());
    }

    #[test]
    fn allows_css_custom_props_with_spread() {
        assert!(crate::rules::test_helpers::run_rule(&Check, r#"import type React from 'react';
const s = {
    "--sidebar-width": "200px",
    "--sidebar-width-icon": "48px",
    ...extra,
} as React.CSSProperties;"#, "t.tsx")
        .is_empty());
    }

    #[test]
    fn still_flags_react_css_properties_without_custom_props() {
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "const s = { color: 'red' } as React.CSSProperties;", "t.tsx").len(),
            1
        );
    }

    // Regression test for #6280: a non-literal computed key (`[attr]`) gives the
    // object an implicit string index signature; `satisfies T` rejects it when
    // `T` (a named interface / discriminated union) has no index signature, so
    // the `as T` cast is irreplaceable. Object case.
    #[test]
    fn allows_object_with_non_literal_computed_key() {
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                "const r = { [metaKey]: `${prefix}:${type}`, content: value.url } as MetaGeneric;",
                "t.ts",
            )
            .is_empty()
        );
    }

    // #6280: array of object literals with a non-literal computed key —
    // `[{ [attr]: v, ...rest }] as T[]` must not be flagged.
    #[test]
    fn allows_array_of_objects_with_non_literal_computed_key() {
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                "const a = [{ [attr]: fixedKey, ...sanitizedValue }] as UnheadMeta[];",
                "t.ts",
            )
            .is_empty()
        );
    }

    // #6280 boundary: a literal computed key (`['lit']`, `[0]`) names a known
    // property — no implicit index signature, `satisfies` works — so the cast
    // must still be flagged.
    #[test]
    fn still_flags_literal_computed_key() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const x = { ['lit']: 1 } as T;", "t.ts").len(), 1);
    }

    #[test]
    fn still_flags_numeric_computed_key() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const x = { [0]: 1 } as T;", "t.ts").len(), 1);
    }

    // #6280 boundary: an all-static-key object literal stays in scope.
    #[test]
    fn still_flags_all_static_keys() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const x = { a: 1, b: 2 } as T;", "t.ts").len(), 1);
    }

    // Regression test for #6544: `[...new Set(xs.filter(Boolean))] as string[]`
    // spreads a non-literal whose inferred element type is broader than
    // `string` (`(string | false | undefined)[]`), so `as string[]` is a
    // narrowing assertion — `satisfies string[]` would not compile. The spread
    // of a non-array-literal must suppress the diagnostic.
    #[test]
    fn allows_array_with_non_literal_spread() {
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                "const watchingFiles = [...new Set(xs.filter(Boolean))] as string[];",
                "t.ts",
            )
            .is_empty()
        );
    }

    // #6544: spreading a plain identifier is equally a narrowing risk.
    #[test]
    fn allows_array_with_identifier_spread() {
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "const a = [...items, extra] as Foo[];", "t.ts").is_empty()
        );
    }

    // #6544 boundary: spreading an array literal has a known element type, so
    // the cast can still be replaced with `satisfies` — must still flag.
    #[test]
    fn still_flags_array_literal_spread() {
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "const a = [...[1, 2], 3] as number[];", "t.ts").len(),
            1
        );
    }

    // Regression test for #6611: `{ ...process.env, ...options.env } as
    // Record<string, string>` spreads non-literals whose value type is broader
    // than `string` (`process.env` values are `string | undefined`), so `as
    // Record<string, string>` is a narrowing assertion — `satisfies` would not
    // compile. A spread of a non-literal source must suppress the diagnostic.
    #[test]
    fn allows_object_with_non_literal_spread() {
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                "const env = { ...process.env, ...options.env } as Record<string, string>;",
                "t.ts",
            )
            .is_empty()
        );
    }

    // #6611: a single identifier spread is equally a narrowing risk.
    #[test]
    fn allows_object_with_identifier_spread() {
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "const o = { ...opts } as T;", "t.ts").is_empty()
        );
    }

    // #6611 boundary: spreading an object literal has a known type, so the cast
    // can still be replaced with `satisfies` — must still flag.
    #[test]
    fn still_flags_object_literal_spread() {
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "const o = { ...{ a: 1 }, b: 2 } as T;", "t.ts").len(),
            1
        );
    }

    // Regression test for #6849: Vue's `[String, Object] as PropType<T>` is the
    // idiomatic props pattern — runtime constructors never structurally satisfy
    // the branded `PropType<T>`, so `satisfies` would not compile and the `as`
    // is required. Must not be flagged.
    #[test]
    fn allows_array_of_constructors_as_prop_type() {
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                "const x = { type: [String, Object] as PropType<AsTag | Component> };",
                "t.ts",
            )
            .is_empty()
        );
    }

    // #6849: minimal single-constructor PropType cast.
    #[test]
    fn allows_array_literal_as_prop_type() {
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "const p = [String] as PropType<Foo>;", "t.ts").is_empty()
        );
    }

    // #6849 boundary: anchor on the exact `PropType` reference — a longer
    // identifier that merely contains `PropType` as a substring is a normal
    // concrete type that `satisfies` can validate, so it must still flag.
    #[test]
    fn still_flags_substring_prop_type_name() {
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "const x = [a, b] as MyPropType<Foo>;", "t.ts").len(),
            1
        );
    }

    // #6849 boundary: the trailing `<` is load-bearing — an identifier that
    // *begins* with `PropType` (`PropTypeFoo`) is a distinct concrete type that
    // `satisfies` can validate, so it must still flag. Guards against dropping
    // the `<` and re-introducing the false positive on prefix-extension names.
    #[test]
    fn still_flags_prefix_extension_prop_type_name() {
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "const x = [a, b] as PropTypeFoo<X>;", "t.ts").len(),
            1
        );
    }
}
