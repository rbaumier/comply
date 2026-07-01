//! use-type-alias OxcCheck backend — detect repeated complex inline type
//! annotations via oxc AST.
//!
//! Two-pass via `run_on_semantic`: iterate all nodes collecting union/intersection
//! type text, then report duplicates.

use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use oxc_span::GetSpan;

pub struct Check;

/// True if `t` is a single keyword type (`string`, `number`, …) or a
/// plain type-reference identifier — i.e. a one-token type without
/// nested structure.
fn is_simple_type(t: &TSType) -> bool {
    matches!(
        t,
        TSType::TSNullKeyword(_)
            | TSType::TSUndefinedKeyword(_)
            | TSType::TSStringKeyword(_)
            | TSType::TSNumberKeyword(_)
            | TSType::TSBooleanKeyword(_)
            | TSType::TSBigIntKeyword(_)
            | TSType::TSAnyKeyword(_)
            | TSType::TSUnknownKeyword(_)
            | TSType::TSNeverKeyword(_)
            | TSType::TSObjectKeyword(_)
            | TSType::TSVoidKeyword(_)
            | TSType::TSSymbolKeyword(_)
            | TSType::TSTypeReference(_)
    )
}

fn is_null_or_undefined(t: &TSType) -> bool {
    matches!(
        t,
        TSType::TSNullKeyword(_) | TSType::TSUndefinedKeyword(_)
    )
}

/// A pattern like `T | null`, `T | undefined`, `null | undefined` —
/// short, structurally trivial, and almost always semantically distinct
/// at each call site (a nullable DSN is a different concept from a
/// nullable CSP host). Promoting these to a shared alias hurts more
/// than it helps.
fn is_trivial_nullable_union(types: &[TSType]) -> bool {
    if types.len() != 2 {
        return false;
    }
    if !types.iter().all(is_simple_type) {
        return false;
    }
    types.iter().any(is_null_or_undefined)
}

/// Canonical grouping key for a commutative type form (union/intersection).
///
/// Collects each direct member's source text (whitespace-trimmed), sorts the
/// members lexicographically, then joins them with `sep`. Because unions and
/// intersections are unordered, `number | string`, `string | number` and
/// `number|string` all map to the same key — reordered occurrences of one
/// type are counted together instead of split into separate groups. oxc
/// keeps a flat same-kind chain (`A | B | C`) flat, so sorting the direct
/// members canonicalizes the common case.
fn canonical_key(members: &[TSType], source: &str, sep: &str) -> String {
    let mut parts: Vec<&str> = members
        .iter()
        .map(|m| {
            let span = m.span();
            source[span.start as usize..span.end as usize].trim()
        })
        .collect();
    parts.sort_unstable();
    parts.join(sep)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Type-test files (`.test-d.` infix, `test-d/`, `test-dts/`, …) assert
        // the exact inferred type at each `expectType<T>()`; repeating an inline
        // type there is intentional, and extracting a shared alias would change
        // what is tested (an alias is a new nominal type in conditional checks).
        let path_str = ctx.path.to_string_lossy();
        if path_str.contains(".test.")
            || path_str.contains(".spec.")
            || path_str.contains("__tests__")
            || path_str.contains("_test.")
            || crate::rules::path_utils::is_type_test_file(ctx.path)
            || crate::rules::path_utils::is_type_compilation_test_path(ctx.path)
        {
            return vec![];
        }

        let mut annotation_lines: FxHashMap<String, Vec<usize>> = FxHashMap::default();

        for node in semantic.nodes().iter() {
            let (span, members, sep) = match node.kind() {
                AstKind::TSUnionType(u) => {
                    // A trivial nullable union (`T | null`, `T | undefined`)
                    // is rarely a shared domain concept — counting it
                    // produces a steady stream of "rename to StringOrNull"
                    // suggestions that destroy local readability.
                    if is_trivial_nullable_union(&u.types) {
                        continue;
                    }
                    (u.span, &u.types, " | ")
                }
                AstKind::TSIntersectionType(i) => (i.span, &i.types, " & "),
                _ => continue,
            };

            // Skip nested union/intersection — only count the outermost. A
            // union/intersection that is structurally a component of an
            // enclosing union/intersection is nested and must not be reported
            // on its own. Walk up through transparent type containers so the
            // nesting is recognized through them:
            //   - the `<…>` generic arguments (`TSTypeParameterInstantiation`)
            //     and the type references wrapping them (`TSTypeReference`), so
            //     a union inside the generic argument of an enclosing
            //     intersection (e.g. `A & Partial<Record<"x" | "y", V>>`) is
            //     recognized as nested;
            //   - parentheses (`TSParenthesizedType`), so a parenthesized
            //     member of a union/intersection (e.g. the `(string & {})` in
            //     `"a" | "b" | (string & {})`) is recognized as nested rather
            //     than counted as a free-standing inline type.
            let mut ancestor = semantic.nodes().parent_node(node.id());
            while matches!(
                ancestor.kind(),
                AstKind::TSTypeParameterInstantiation(_)
                    | AstKind::TSTypeReference(_)
                    | AstKind::TSParenthesizedType(_)
            ) {
                ancestor = semantic.nodes().parent_node(ancestor.id());
            }
            if matches!(ancestor.kind(), AstKind::TSUnionType(_) | AstKind::TSIntersectionType(_)) {
                continue;
            }

            // Skip occurrences that are part of a type *definition* rather than a
            // usage-site annotation:
            //   - inside a type alias declaration: each alias names a distinct
            //     domain concept regardless of structural identity;
            //   - as part of a generic type-parameter declaration — its
            //     constraint or default (`T extends X | Y` / `T = X | Y`):
            //     overload signatures must independently redeclare their type
            //     parameters, so the repetition is structurally forced, not a
            //     copy-paste smell a shared alias would simplify.
            // Counting either as a duplicate produces false positives.
            {
                let mut cur_id = node.id();
                let mut in_definition = false;
                loop {
                    let p = semantic.nodes().parent_node(cur_id);
                    if p.id() == cur_id {
                        break;
                    }
                    if matches!(
                        p.kind(),
                        AstKind::TSTypeAliasDeclaration(_) | AstKind::TSTypeParameter(_)
                    ) {
                        in_definition = true;
                        break;
                    }
                    cur_id = p.id();
                }
                if in_definition {
                    continue;
                }
            }

            let text = &ctx.source[span.start as usize..span.end as usize];
            if text.len() <= 5 {
                continue;
            }

            // Skip unions/intersections that reference an enclosing-scope type
            // parameter (`TT | ST`, `DB & T`, `N | CTEBuilderCallback<N>`): each
            // textual occurrence is a different concrete type per instantiation,
            // and TypeScript forbids hoisting them to a class-body-local alias —
            // a module-level generic alias only renames the duplication.
            if members.iter().any(|m| {
                crate::oxc_helpers::type_references_enclosing_type_parameter(m, semantic)
            }) {
                continue;
            }

            // Key on the canonical (member-sorted) form so commutative
            // reorderings of one type — `number | string` vs `string | number`
            // — aggregate into a single group and count, rather than splitting
            // into two redundant alias suggestions for the same concept.
            let (line, _) = byte_offset_to_line_col(ctx.source, span.start as usize);
            annotation_lines
                .entry(canonical_key(members, ctx.source, sep))
                .or_default()
                .push(line);
        }

        let mut diagnostics = Vec::new();
        for (annotation, lines) in &annotation_lines {
            if lines.len() >= 2 {
                for &line_num in lines {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: line_num,
                        column: 1,
                        rule_id: "use-type-alias".into(),
                        message: format!(
                            "Inline type `{}` appears {} times \u{2014} extract a type alias.",
                            annotation,
                            lines.len()
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diagnostics.sort_by_key(|d| d.line);
        diagnostics
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_with_path(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
    }

    #[test]
    fn flags_repeated_complex_union() {
        let src = r#"
            const a: string | number | boolean = 1 as any;
            const b: string | number | boolean = 2 as any;
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn ignores_repeated_nullable_union() {
        // Regression for rbaumier/comply#31 — `string | null` is too
        // generic to share an alias for; distinct call sites are nearly
        // always semantically distinct concepts.
        let src = r#"
            export type Config = { sentryDsn: string | null };
            type CspConnectSource = string | null;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_repeated_optional_union() {
        let src = r#"
            type A = number | undefined;
            type B = number | undefined;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_nullish_pair() {
        let src = r#"
            type A = null | undefined;
            type B = null | undefined;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_complex_union_in_function_params() {
        // `{ a: string } | null` in function parameters is a usage site, not
        // a declaration — repeated usage still warrants extraction.
        let src = r#"
            function a(x: { a: string } | null) {}
            function b(x: { a: string } | null) {}
            function c(x: { a: string } | null) {}
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn no_fp_on_semantically_distinct_type_aliases() {
        // Regression #379 — two type aliases sharing the same structural type
        // must not be flagged; each alias names a distinct domain concept.
        let src = r#"
            type ApiResponse = string | number | boolean;
            type CacheEntry = string | number | boolean;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_in_test_file() {
        // Regression #799 — repeated union in .test.ts must not fire.
        let src = r#"
            const a: 'a' | 'b' | 'c' = 'a';
            const b: 'a' | 'b' | 'c' = 'b';
            const c: 'a' | 'b' | 'c' = 'c';
        "#;
        assert!(run_with_path(src, "foo.test.ts").is_empty());
    }

    #[test]
    fn no_fp_in_spec_file() {
        // Regression #799 — repeated union in .spec.ts must not fire.
        let src = r#"
            function a(x: { data: string } | { error: string }) {}
            function b(x: { data: string } | { error: string }) {}
        "#;
        assert!(run_with_path(src, "foo.spec.ts").is_empty());
    }

    #[test]
    fn no_fp_in_test_d_dir() {
        // Regression #799 — repeated intersection in test-d/ must not fire.
        let src = r#"
            type A = { x: number } & { y: string };
            type B = { x: number } & { y: string };
        "#;
        assert!(run_with_path(src, "test-d/foo.ts").is_empty());
    }

    #[test]
    fn no_fp_in_test_dts_dir() {
        // Regression #7080 (vuejs/pinia) — repeated inline type in a Vue-ecosystem
        // `test-dts/` type-compilation-test file is intentional (each expectType
        // asserts the exact inferred type) and must not fire.
        let src = r#"
            expectType<{ light: ComputedRef<'off' | 'on'> }>(a());
            expectType<{ light: ComputedRef<'off' | 'on'> }>(b());
            expectType<{ light: ComputedRef<'off' | 'on'> }>(c());
        "#;
        assert!(run_with_path(src, "packages/pinia/test-dts/store.ts").is_empty());
    }

    #[test]
    fn no_fp_on_test_d_infix_file() {
        // Regression #7080 (vuejs/pinia) — a `.test-d.` filename infix (living
        // beside its source, not under a test-d/ directory) is the tsd type-test
        // convention and must not fire.
        let src = r#"
            expectType<{ light: ComputedRef<'off' | 'on'> }>(a());
            expectType<{ light: ComputedRef<'off' | 'on'> }>(b());
            expectType<{ light: ComputedRef<'off' | 'on'> }>(c());
        "#;
        assert!(run_with_path(src, "packages/pinia/src/mapHelpers.test-d.ts").is_empty());
    }

    #[test]
    fn normal_ts_file_still_flagged() {
        // Regression #799 — the guard must not suppress non-test files.
        let src = r#"
            const a: 'a' | 'b' | 'c' = 'a';
            const b: 'a' | 'b' | 'c' = 'b';
        "#;
        assert!(!run_with_path(src, "foo.ts").is_empty());
    }

    #[test]
    fn still_flags_repeated_concrete_object_union() {
        // Positive control for #6256 — a fully concrete union with no type
        // parameters must still be flagged; the type-parameter guard must not
        // erode the rule's core value.
        let src = r#"
            function f(x: { a: number } | { b: string }) {}
            function g(x: { a: number } | { b: string }) {}
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn no_fp_on_class_type_parameter_union() {
        // #6256 — `TT | ST` composed of class type parameters cannot be hoisted
        // to a class-body-local alias and is a different concrete type per
        // instantiation; it must not be flagged.
        let src = r#"
            class Builder<TT, ST> {
                a(): TT | ST { return null as any; }
                b(): TT | ST { return null as any; }
                c(): TT | ST { return null as any; }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_type_parameter_intersection() {
        // #6256 — `DB & T` (intersection of enclosing type parameters), and
        // `TB & string` (type parameter intersected with a keyword) are
        // instantiation-dependent and must not be flagged.
        let src = r#"
            class ReadonlyKysely<DB> {
                a<T>(): DB & T { return null as any; }
                b<T>(): DB & T { return null as any; }
            }
            function widen<TB>(): TB & string { return null as any; }
            function widen2<TB>(): TB & string { return null as any; }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_type_parameter_in_nested_type_argument() {
        // #6256 — the type parameter is reachable only inside a generic type
        // argument (`Foo<N>`), not as a direct union member; the recursion into
        // type arguments must still exempt it.
        let src = r#"
            class CTE<N> {
                a(): string | CTEBuilderCallback<N> { return null as any; }
                b(): string | CTEBuilderCallback<N> { return null as any; }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_kysely_merge_query_builder() {
        // #6256 reproduction — `TT | ST` recurring across a generic class's
        // implements clause and method signatures (kysely-org/kysely).
        let src = r#"
            export class WheneableMergeQueryBuilder<DB, TT extends keyof DB, ST extends keyof DB, O>
                implements
                    MultiTableReturningInterface<DB, TT | ST, O>,
                    OutputInterface<DB, TT | ST, O> {
                returning(): SelectQueryBuilder<DB, TT | ST, O> { return null as any; }
                output(): MergeResult | TT | ST { return null as any; }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_generic_type_parameter_constraint() {
        // Regression #6537 — `string | readonly string[]` repeated across
        // overload signatures only ever appears as a generic type-parameter
        // constraint (`T extends ...`), which TypeScript forces each overload
        // to redeclare. The repetition is structurally forced, not a
        // copy-paste smell a shared alias would simplify.
        let src = r#"
            export function pascalCase<
              T extends string | readonly string[],
              UserCaseOptions extends CaseOptions = CaseOptions,
            >(str: T, opts?: CaseOptions): PascalCase<T>;
            export function pascalCase<
              T extends string | readonly string[],
              UserCaseOptions extends CaseOptions = CaseOptions,
            >(str?: T, opts?: UserCaseOptions): string {
              return "";
            }
            export function camelCase<
              T extends string | readonly string[],
            >(str: T): CamelCase<T>;
            export function camelCase<
              T extends string | readonly string[],
            >(str?: T): string {
              return "";
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_repeated_union_at_value_sites() {
        // Negative control for #6537 — the constraint-position exemption does
        // not globally whitelist the union: repeated at real value-binding
        // annotation sites the same union must still fire.
        let src = r#"
            function a(x: string | readonly string[]) {}
            function b(x: string | readonly string[]) {}
            function c(x: string | readonly string[]) {}
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn constraint_occurrence_does_not_suppress_value_site_duplicates() {
        // The constraint occurrence is exempt, but value-site repetitions of the
        // same union count on their own — the fix excludes only constraint
        // positions, it does not whitelist the union everywhere.
        let src = r#"
            function id<T extends string | readonly string[]>(x: T): T { return x; }
            function a(x: string | readonly string[]) {}
            function b(x: string | readonly string[]) {}
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn no_fp_on_inner_union_in_generic_arg() {
        // Regression #6587 — the inner union `"unbuild" | "build"` is a
        // structural component of the enclosing intersection, reached through
        // the generic arguments of `Record<>`. It must not be reported on its
        // own: only the outer intersection counts, exactly once per occurrence.
        let src = r#"
            const pkg: PackageJson & Partial<Record<"unbuild" | "build", BuildConfig>> = load();
            function build(pkg: PackageJson & Partial<Record<"unbuild" | "build", BuildConfig>>) {}
        "#;
        let diags = run(src);
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().all(|d| d.message.contains("PackageJson & Partial")));
    }

    #[test]
    fn still_flags_inner_union_not_inside_enclosing_union_or_intersection() {
        // Control for #6587 — when the repeated inner union is NOT a component
        // of an enclosing union/intersection (`Record<>` is the whole
        // annotation, not wrapped in a `&`/`|`), the walk reaches the type
        // annotation, not a union/intersection, so it is still evaluated and a
        // repeated occurrence is flagged as before.
        let src = r#"
            const a: Record<"alpha" | "beta", number> = make();
            const b: Record<"alpha" | "beta", number> = make();
        "#;
        let diags = run(src);
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().all(|d| d.message.contains(r#""alpha" | "beta""#)));
    }

    #[test]
    fn merges_commutative_union_orderings() {
        // Regression #6663 — `number | string` and `string | number` are the
        // same TypeScript type (unions are unordered). Each ordering appears
        // once (below the threshold), but they must aggregate into a single
        // group that crosses it, yielding ONE canonical suggestion.
        let src = r#"
            interface IPXModifiers {
                quality: number | string;
                width:   string | number;
            }
        "#;
        let diags = run(src);
        assert_eq!(diags.len(), 2, "both orderings counted as one group");
        let messages: std::collections::HashSet<_> =
            diags.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages.len(), 1, "a single canonical suggestion");
        assert!(diags[0].message.contains("`number | string`"));
        assert!(diags[0].message.contains("appears 2 times"));
    }

    #[test]
    fn no_fp_on_single_occurrence() {
        // A type that occurs only once stays below the threshold even after
        // canonicalization — the merge must not over-count one occurrence.
        let src = r#"
            interface A {
                only: number | string;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn distinct_unions_stay_separate() {
        // Normalization sorts members but must not merge genuinely distinct
        // types: `number | string` and `number | boolean` differ. Each
        // commutative pair merges within its own group, leaving two groups.
        let src = r#"
            interface A {
                a: number | string;
                b: number | boolean;
                c: string | number;
                d: boolean | number;
            }
        "#;
        let diags = run(src);
        assert_eq!(diags.len(), 4);
        let messages: std::collections::HashSet<_> =
            diags.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages.len(), 2, "two distinct canonical groups");
        assert!(messages.iter().any(|m| m.contains("`number | string`")));
        assert!(messages.iter().any(|m| m.contains("`boolean | number`")));
    }

    #[test]
    fn no_fp_on_parenthesized_intersection_in_union() {
        // Regression #6844 (colinhacks/zod, errors.ts) — `(string & {})` is the
        // open-ended-string-union idiom appearing as a parenthesized member of
        // an enclosing union. The intersection sits under a `TSParenthesizedType`
        // wrapper, so the nesting walk must see through it and not count the
        // intersection as a free-standing inline type. Each enclosing union is
        // distinct, so nothing crosses the threshold.
        let src = r#"
            interface A {
                readonly origin: "number" | "int" | (string & {});
            }
            interface B {
                readonly origin: "bigint" | "date" | (string & {});
            }
            interface C {
                readonly format: "email" | "url" | (string & {});
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_parenthesized_intersection_in_mapped_type_key() {
        // Regression #6844 (colinhacks/zod, locales/zh-TW.ts) — `(string & {})`
        // as a member of a mapped-type-key union (`[k in X | (string & {})]`) is
        // still a parenthesized member of an enclosing union and must not be
        // counted on its own.
        let src = r#"
            const A: { [k in "a" | "b" | (string & {})]?: string } = {};
            const B: { [k in "c" | "d" | (string & {})]?: string } = {};
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_repeated_parenthesized_intersection_not_in_union() {
        // Positive control for #6844 — a parenthesized intersection that is NOT
        // a member of an enclosing union/intersection (the parens wrap the whole
        // annotation) is still a free-standing inline type. The fix sees through
        // the parentheses to the type annotation, not to a union, so a repeated
        // occurrence is still flagged.
        let src = r#"
            function a(x: ({ a: number } & { b: string })) {}
            function b(x: ({ a: number } & { b: string })) {}
            function c(x: ({ a: number } & { b: string })) {}
        "#;
        assert!(!run(src).is_empty());
    }
}
