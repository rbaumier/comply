//! Type-aware rules delegated to oxlint --type-aware.
//!
//! These rules require the TypeScript type checker via tsgolint/typescript-go.
//! They run automatically when oxlint-tsgolint is installed.

use crate::diagnostic::Severity;
use crate::rules::backend::{Backend, PostFilter};
use crate::rules::meta::RuleMeta;
use crate::rules::{Language, RuleDef};
use std::sync::Arc;

pub fn register_all() -> Vec<RuleDef> {
    vec![
        // ══════════════════════════════════════════════════════════════════
        // ASYNC / PROMISES
        // ══════════════════════════════════════════════════════════════════
        entry(
            "no-floating-promises",
            "no-floating-promises",
            "Promises must be awaited, caught, or explicitly voided.",
            "Add `await`, `.catch()`, or `void` prefix.",
        ),
        entry(
            "no-misused-promises",
            "no-misused-promises",
            "Promise used in a void context or as a boolean condition.",
            "Await the promise or check its resolved value explicitly.",
        ),
        entry_with_filter(
            "await-thenable",
            "await-thenable",
            "`await` on a non-thenable value has no effect.",
            "Remove the `await` or ensure the value is a Promise.",
            Some(Arc::new(AwaitThenableFilter)),
        ),
        // `require-await` is intentionally NOT delegated: comply ships the
        // native `no-async-without-await` rule, which carries the contract-case
        // exceptions (explicit `Promise<…>` return type, concise-body arrows)
        // that keep it from contradicting `promise-function-async` on no-op
        // `Promise<void>` stubs. The raw tsgolint variant lacks those and
        // re-introduces an unsatisfiable rule pair. See #283.
        entry_with_filter(
            "promise-function-async",
            "promise-function-async",
            "Function returns a Promise but is not marked `async`.",
            "Add the `async` keyword to the function declaration.",
            Some(Arc::new(PromiseFunctionAsyncFilter)),
        ),
        entry(
            "prefer-promise-reject-errors",
            "prefer-promise-reject-errors",
            "`Promise.reject()` should receive an Error, not a primitive.",
            "Pass an Error instance: `Promise.reject(new Error(msg))`.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // TYPE SAFETY — ANY LEAKS
        // ══════════════════════════════════════════════════════════════════
        // `no-explicit-any` is enforced by the native oxc rule `ts-no-explicit-any`
        // (the canonical id); the syntactic `any` keyword needs no type program, so
        // the type-aware variant is not registered here (one finding, one id — #5768).
        entry_with_filter(
            "no-unsafe-argument",
            "no-unsafe-argument",
            "Passing `any` to a typed parameter defeats type safety.",
            "Add a type assertion or fix the source of `any`.",
            Some(Arc::new(TypeTestFileFilter)),
        ),
        entry_with_filter(
            "no-unsafe-assignment",
            "no-unsafe-assignment",
            "Assigning `any` to a typed variable defeats type safety.",
            "Add a type assertion or fix the source of `any`.",
            Some(Arc::new(NoUnsafeAssignmentFilter)),
        ),
        entry_with_filter(
            "no-unsafe-call",
            "no-unsafe-call",
            "Calling a value typed as `any` is unsafe.",
            "Add proper types or use a type guard.",
            Some(Arc::new(TypeTestFileFilter)),
        ),
        entry_with_filter(
            "no-unsafe-member-access",
            "no-unsafe-member-access",
            "Accessing a member on `any` is unsafe.",
            "Add proper types or use a type guard.",
            Some(Arc::new(TypeTestFileFilter)),
        ),
        entry(
            "no-unsafe-return",
            "no-unsafe-return",
            "Returning `any` from a typed function defeats type safety.",
            "Add a type assertion or fix the source of `any`.",
        ),
        entry(
            "no-unsafe-declaration-merging",
            "no-unsafe-declaration-merging",
            "Interface/class merging can bypass type checking.",
            "Avoid declaration merging or use separate types.",
        ),
        entry(
            "no-unsafe-enum-comparison",
            "no-unsafe-enum-comparison",
            "Comparing enum with non-enum value is error-prone.",
            "Compare with enum members only.",
        ),
        entry(
            "no-unsafe-function-type",
            "no-unsafe-function-type",
            "The `Function` type accepts any function — use explicit signatures.",
            "Replace with a specific function type: `(arg: T) => R`.",
        ),
        entry(
            "no-unsafe-unary-minus",
            "no-unsafe-unary-minus",
            "Unary minus on non-number type is error-prone.",
            "Ensure the operand is a number.",
        ),
        entry(
            "use-unknown-in-catch-callback-variable",
            "use-unknown-in-catch-callback-variable",
            "Catch callback parameter should be `unknown`, not `any`.",
            "Type the catch parameter as `unknown` and narrow it.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // BOOLEAN / CONDITIONS
        // ══════════════════════════════════════════════════════════════════
        // entry(
        //     "strict-boolean-expressions",
        //     "strict-boolean-expressions",
        //     "Condition is not explicitly boolean — implicit coercion is error-prone.",
        //     "Use an explicit comparison: `!== undefined`, `> 0`, `Boolean(x)`.",
        // ),
        entry_with_filter(
            "no-unnecessary-condition",
            "no-unnecessary-condition",
            "Condition is always truthy or always falsy based on types.",
            "Remove the condition or fix the type.",
            Some(Arc::new(NoUnnecessaryConditionFilter)),
        ),
        entry(
            "no-unnecessary-boolean-literal-compare",
            "no-unnecessary-boolean-literal-compare",
            "`x === true` is verbose — use `x` directly.",
            "Remove the comparison: `if (x)` instead of `if (x === true)`.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // NULLISH
        // ══════════════════════════════════════════════════════════════════
        entry(
            "prefer-nullish-coalescing",
            "prefer-nullish-coalescing",
            "Use `??` instead of `||` to only coalesce `null`/`undefined`.",
            "Replace `||` with `??` to avoid falsy-value bugs.",
        ),
        entry(
            "prefer-optional-chain",
            "prefer-optional-chain",
            "Use `?.` instead of `&& x.y` for cleaner null checks.",
            "Replace `x && x.y` with `x?.y`.",
        ),
        entry(
            "no-non-null-asserted-nullish-coalescing",
            "no-non-null-asserted-nullish-coalescing",
            "`x! ?? y` is contradictory — `!` asserts non-null.",
            "Remove the `!` or the `??`.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // ARRAYS
        // ══════════════════════════════════════════════════════════════════
        entry(
            "no-for-in-array",
            "no-for-in-array",
            "`for...in` iterates over array indices as strings, not values.",
            "Use `for...of` or `.forEach()` instead.",
        ),
        entry(
            "prefer-find",
            "prefer-find",
            "`.filter()[0]` is less efficient than `.find()`.",
            "Use `.find()` to get the first matching element.",
        ),
        entry(
            "prefer-includes",
            "prefer-includes",
            "`.indexOf() !== -1` is less readable than `.includes()`.",
            "Use `.includes()` for membership checks.",
        ),
        entry(
            "require-array-sort-compare",
            "require-array-sort-compare",
            "`.sort()` without a comparator converts elements to strings.",
            "Provide an explicit comparator: `.sort((a, b) => a - b)`.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // ERRORS
        // ══════════════════════════════════════════════════════════════════
        entry_with_filter(
            "only-throw-error",
            "only-throw-error",
            "Throwing a non-Error value loses stack trace information.",
            "Throw an Error instance: `throw new Error(message)`.",
            Some(Arc::new(OnlyThrowErrorFilter)),
        ),
        // ══════════════════════════════════════════════════════════════════
        // METHODS / THIS
        // ══════════════════════════════════════════════════════════════════
        entry_with_filter(
            "unbound-method",
            "unbound-method",
            "Method passed as callback loses its `this` binding.",
            "Bind the method: `.bind(this)` or use an arrow function.",
            Some(Arc::new(UnboundMethodFilter)),
        ),
        entry(
            "no-this-alias",
            "no-this-alias",
            "`const self = this` is legacy — use arrow functions.",
            "Replace callback with arrow function to preserve `this`.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // OPERATORS
        // ══════════════════════════════════════════════════════════════════
        entry(
            "restrict-plus-operands",
            "restrict-plus-operands",
            "`+` operator between incompatible types may cause unexpected coercion.",
            "Ensure both operands are the same type.",
        ),
        entry(
            "restrict-template-expressions",
            "restrict-template-expressions",
            "Interpolating `any` or complex objects in templates is error-prone.",
            "Convert to string explicitly or add proper types.",
        ),
        entry(
            "dot-notation",
            "dot-notation",
            "`obj[\"prop\"]` should be `obj.prop` when possible.",
            "Use dot notation for known property names.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // UNIONS / ENUMS
        // ══════════════════════════════════════════════════════════════════
        entry(
            "switch-exhaustiveness-check",
            "switch-exhaustiveness-check",
            "Switch statement does not handle all union members.",
            "Add cases for missing union members or a default case.",
        ),
        entry(
            "no-mixed-enums",
            "no-mixed-enums",
            "Enum mixes string and number members, which is confusing.",
            "Use either all string or all number members.",
        ),
        entry(
            "no-duplicate-enum-values",
            "no-duplicate-enum-values",
            "Enum has duplicate values — likely a copy-paste error.",
            "Use unique values for each enum member.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // TYPES — REDUNDANT / UNNECESSARY
        // ══════════════════════════════════════════════════════════════════
        entry(
            "no-unnecessary-type-assertion",
            "no-unnecessary-type-assertion",
            "Type assertion `as T` is unnecessary — value is already that type.",
            "Remove the type assertion.",
        ),
        entry(
            "no-unnecessary-type-constraint",
            "no-unnecessary-type-constraint",
            "`<T extends unknown>` is redundant — `unknown` is the default.",
            "Remove the constraint: `<T>`.",
        ),
        entry_with_filter(
            "no-unnecessary-type-parameters",
            "no-unnecessary-type-parameters",
            "Type parameter is never used or could be `unknown`.",
            "Remove the unused type parameter.",
            Some(Arc::new(EqualProbeFilter)),
        ),
        entry(
            "no-unnecessary-template-expression",
            "no-unnecessary-template-expression",
            "`` `${x}` `` is verbose when `x` is already a string.",
            "Use `x` directly without template literal.",
        ),
        entry(
            "no-unnecessary-type-conversion",
            "no-unnecessary-type-conversion",
            "`String(x)` is unnecessary when `x` is already a string.",
            "Remove the conversion.",
        ),
        entry(
            "no-unnecessary-parameter-property-assignment",
            "no-unnecessary-parameter-property-assignment",
            "`this.x = x` is redundant when `x` is a parameter property.",
            "Remove the assignment — parameter property handles it.",
        ),
        entry_with_filter(
            "no-redundant-type-constituents",
            "no-redundant-type-constituents",
            "`string | \"foo\"` is redundant — the literal is subsumed.",
            "Remove the redundant type constituent.",
            Some(Arc::new(NoRedundantTypeConstituentsFilter)),
        ),
        entry(
            "no-duplicate-type-constituents",
            "no-duplicate-type-constituents",
            "`A | A` has duplicate — likely a copy-paste error.",
            "Remove the duplicate type constituent.",
        ),
        // `no-inferrable-types` is enforced by the native oxc rule
        // `ts-no-inferrable-types` (the canonical id); a redundant annotation on a
        // literal initializer is syntactic, so the type-aware variant is not
        // registered here (one finding, one id — #5768).
        // ══════════════════════════════════════════════════════════════════
        // TYPES — BAD PATTERNS
        // ══════════════════════════════════════════════════════════════════
        entry(
            "no-wrapper-object-types",
            "no-wrapper-object-types",
            "`String` should be `string` — use primitive types.",
            "Use lowercase primitive: `string`, `number`, `boolean`.",
        ),
        entry_with_filter(
            "no-invalid-void-type",
            "no-invalid-void-type",
            "`void` is only valid as a return type, not a variable type.",
            "Use `undefined` for variables, `void` only for returns.",
            Some(Arc::new(NoInvalidVoidTypeFilter)),
        ),
        entry(
            "no-misused-new",
            "no-misused-new",
            "Interface with `new()` or class with `constructor` type is wrong.",
            "Use proper constructor signature.",
        ),
        entry_with_filter(
            "no-empty-interface",
            "no-empty-interface",
            "Empty interface has no members — use `type` or remove it.",
            "Add members, use `type = {}`, or remove the interface.",
            Some(Arc::new(NoEmptyInterfaceFilter)),
        ),
        entry(
            "no-empty-object-type",
            "no-empty-object-type",
            "`{}` matches any non-nullish value — probably not intended.",
            "Use `object`, `Record<string, unknown>`, or a specific type.",
        ),
        entry(
            "no-extraneous-class",
            "no-extraneous-class",
            "Class with only static members should be a plain object/module.",
            "Export functions directly instead of static class methods.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // TYPES — VOID / RETURN
        // ══════════════════════════════════════════════════════════════════
        entry(
            "no-confusing-void-expression",
            "no-confusing-void-expression",
            "Void expression used where a value is expected.",
            "Don't use void expression as a value.",
        ),
        entry(
            "no-meaningless-void-operator",
            "no-meaningless-void-operator",
            "`void x` has no effect — the value is already discarded.",
            "Remove the `void` operator.",
        ),
        entry_with_filter(
            "strict-void-return",
            "strict-void-return",
            "Function declared void but caller expects a value.",
            "Fix the return type or don't use the return value.",
            Some(Arc::new(StrictVoidReturnFilter)),
        ),
        entry(
            "consistent-return",
            "consistent-return",
            "Function should either always return a value or never.",
            "Add return statements to all branches or none.",
        ),
        entry(
            "return-await",
            "return-await",
            "`return await` is unnecessary outside of try/catch.",
            "Remove `await` from the return statement.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // TYPES — EXPORTS / IMPORTS
        // ══════════════════════════════════════════════════════════════════
        entry(
            "consistent-type-exports",
            "consistent-type-exports",
            "Type-only exports should use `export type`.",
            "Add the `type` keyword: `export type { Foo }`.",
        ),
        // `consistent-type-imports` is enforced via the oxlint passthrough
        // (delegated/ts.rs, canonical id `consistent-type-imports`); it is
        // scope-based and needs no type program, so the type-aware variant is not
        // registered here (one finding, one id — #5768).
        entry(
            "no-import-type-side-effects",
            "no-import-type-side-effects",
            "`import type` should not have side effects.",
            "Split type imports from value imports.",
        ),
        entry(
            "no-useless-empty-export",
            "no-useless-empty-export",
            "`export {}` has no effect in a module.",
            "Remove the empty export.",
        ),
        entry(
            "no-require-imports",
            "no-require-imports",
            "`require()` is CommonJS — use ES `import`.",
            "Replace with `import x from 'module'`.",
        ),
        entry(
            "no-var-requires",
            "no-var-requires",
            "`const x = require()` is CommonJS — use ES `import`.",
            "Replace with `import x from 'module'`.",
        ),
        entry(
            "triple-slash-reference",
            "triple-slash-reference",
            "`/// <reference>` is legacy — use `import`.",
            "Replace with ES import statement.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // STYLE — EXPLICIT TYPES
        // ══════════════════════════════════════════════════════════════════
        entry(
            "explicit-function-return-type",
            "explicit-function-return-type",
            "Function should have an explicit return type.",
            "Add a return type annotation: `function f(): T { }`.",
        ),
        entry(
            "explicit-module-boundary-types",
            "explicit-module-boundary-types",
            "Exported function should have explicit parameter and return types.",
            "Add type annotations to all parameters and return type.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // STYLE — CONSISTENCY
        // ══════════════════════════════════════════════════════════════════
        // `array-type` is a syntactic check (no type info needed) and is
        // already provided as `typescript/array-type` via the oxlint path in
        // delegated/ts.rs. Registering it again here emitted the same
        // diagnostic twice at the same location — see #289.
        entry(
            "consistent-generic-constructors",
            "consistent-generic-constructors",
            "Specify type arguments on constructor: `new Map<K, V>()`.",
            "Move type arguments to constructor call.",
        ),
        entry(
            "prefer-as-const",
            "prefer-as-const",
            "`\"foo\" as const` is cleaner than `\"foo\" as \"foo\"`.",
            "Use `as const` for literal assertions.",
        ),
        entry(
            "prefer-for-of",
            "prefer-for-of",
            "`for (const x of arr)` is cleaner than index-based loop.",
            "Use `for...of` when you don't need the index.",
        ),
        entry(
            "prefer-function-type",
            "prefer-function-type",
            "`() => T` is cleaner than `{ (): T }`.",
            "Use arrow function type syntax.",
        ),
        entry(
            "class-literal-property-style",
            "class-literal-property-style",
            "Use `readonly x = 5` instead of getter for constants.",
            "Replace getter with readonly property.",
        ),
        entry_with_filter(
            "unified-signatures",
            "unified-signatures",
            "Overloads can be unified into a single signature.",
            "Use union type in single signature instead of overloads.",
            Some(Arc::new(UnifiedSignaturesFilter)),
        ),
        entry(
            "related-getter-setter-pairs",
            "related-getter-setter-pairs",
            "Getter and setter have incompatible types.",
            "Ensure getter return type matches setter parameter type.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // STYLE — PREFER
        // ══════════════════════════════════════════════════════════════════
        entry(
            "prefer-regexp-exec",
            "prefer-regexp-exec",
            "`.exec()` is faster than `.match()` for single matches.",
            "Use `regex.exec(str)` instead of `str.match(regex)`.",
        ),
        entry(
            "prefer-string-starts-ends-with",
            "prefer-string-starts-ends-with",
            "`.indexOf() === 0` is less readable than `.startsWith()`.",
            "Use `.startsWith()` or `.endsWith()`.",
        ),
        entry(
            "prefer-return-this-type",
            "prefer-return-this-type",
            "Method returning `this` should use `this` return type.",
            "Change return type to `this` for method chaining.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // RESTRICTIONS — BAN
        // ══════════════════════════════════════════════════════════════════
        entry(
            "ban-ts-comment",
            "ban-ts-comment",
            "`@ts-ignore` and `@ts-nocheck` suppress type errors dangerously.",
            "Fix the type error instead of suppressing it.",
        ),
        entry_with_filter(
            "ban-types",
            "ban-types",
            "`Object`, `{}`, `Function` are too loose — use specific types.",
            "Use `object`, `Record<>`, or explicit function signatures.",
            Some(Arc::new(BanTypesFilter)),
        ),
        entry(
            "no-namespace",
            "no-namespace",
            "TypeScript namespaces are legacy — use ES modules.",
            "Convert namespace to ES module exports.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // OTHER
        // ══════════════════════════════════════════════════════════════════
        entry_with_filter(
            "no-deprecated",
            "no-deprecated",
            "Using deprecated API that may be removed in future.",
            "Replace with the recommended alternative.",
            Some(Arc::new(NoDeprecatedFilter)),
        ),
        entry(
            "no-base-to-string",
            "no-base-to-string",
            "`.toString()` on object without override returns `[object Object]`.",
            "Implement custom `.toString()` or use JSON.stringify.",
        ),
        entry_with_filter(
            "no-implied-eval",
            "no-implied-eval",
            "`setTimeout(\"code\")` executes string as code like eval.",
            "Pass a function instead of a string.",
            Some(Arc::new(NoImpliedEvalFilter)),
        ),
        entry_with_filter(
            "no-misused-spread",
            "no-misused-spread",
            "Spread `...x` on incompatible type loses data.",
            "Ensure spread is used on the correct type.",
            Some(Arc::new(NoMisusedSpreadFilter)),
        ),
    ]
}

fn entry(
    id: &'static str,
    rule_name: &'static str,
    description: &'static str,
    remediation: &'static str,
) -> RuleDef {
    entry_with_filter(id, rule_name, description, remediation, None)
}

fn entry_with_filter(
    id: &'static str,
    rule_name: &'static str,
    description: &'static str,
    remediation: &'static str,
    post_filter: Option<Arc<dyn PostFilter>>,
) -> RuleDef {
    // Leak the prefixed string to get a 'static lifetime.
    // This is intentional — rule definitions live for the entire process.
    let oxlint_key: &'static str = Box::leak(format!("typescript/{rule_name}").into_boxed_str());

    let backends: Vec<(Language, Backend)> = [Language::TypeScript, Language::Tsx]
        .iter()
        .map(|&lang| (lang, Backend::Tsgolint { rule: oxlint_key, post_filter: post_filter.as_ref().map(Arc::clone) }))
        .collect();

    RuleDef {
        meta: RuleMeta {
            id,
            description,
            remediation,
            severity: Severity::Error,
            doc_url: Some("https://typescript-eslint.io/rules/"),
            categories: &["typescript", "type-aware"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        backends,
    }
}

// ── await-thenable post-filter ─────────────────────────────────────────────
//
// RTL's `render()` is synchronous and returns `RenderResult`, not a Promise.
// `await`ing a non-thenable is valid JS (no-op at runtime) and idiomatic in
// `async` test bodies alongside real awaits like `await userEvent.click()`.
// tsgolint correctly identifies the non-thenable `await`, but in a test file
// the pattern is intentional — suppress it. (Closes #449)

struct AwaitThenableFilter;

impl PostFilter for AwaitThenableFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, _source: Option<&str>) -> bool {
        !is_test_path(&diag.path)
    }
}

fn is_test_path(path: &std::path::Path) -> bool {
    let lower = path.to_string_lossy().replace('\\', "/");
    lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.contains("/__tests__/")
        || lower.starts_with("__tests__/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.starts_with("tests/")
        || lower.starts_with("test/")
}

// ── tsd type-test post-filter (no-unsafe-call / -argument / -member-access) ──
//
// tsd type-test files (`.test-d.ts`, `test-d/`, dtslint, etc.) assert type
// relationships at the declaration level: `expectType<string>(api.method())`.
// They are processed only by tsd's type-checker and never run — the file's
// runtime output is discarded. Without the tsd tsconfig context, the type-aware
// backend cannot resolve the asserted symbols, so they degrade to the `error`
// type, which behaves like `any`; that makes the `no-unsafe-*` family fire on
// every assertion. Since the operations exist solely to be type-checked, not
// executed, drop the diagnostic. The signal is the tsd type-test convention via
// the shared [`crate::rules::path_utils::is_type_test_file`] predicate — narrow
// enough that ordinary `.test.`/`.spec.` runtime unit tests, where an unsafe
// `any` is a genuine bug, still flag. (Closes #5741)

struct TypeTestFileFilter;

impl PostFilter for TypeTestFileFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, _source: Option<&str>) -> bool {
        !crate::rules::path_utils::is_type_test_file(&diag.path)
    }
}

// ── unbound-method post-filter ─────────────────────────────────────────────
//
// typescript-eslint documents that `unbound-method` should be disabled in test
// files: standard mock/spy inspection (`expect(vi.mocked(x).method)`,
// `jest.spyOn(obj, 'method')`) references a method to read its call records
// without invoking it detached, so there is no `this`-binding hazard. Suppress
// it in test files. (Closes #4369)

struct UnboundMethodFilter;

impl PostFilter for UnboundMethodFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, _source: Option<&str>) -> bool {
        !is_test_path(&diag.path)
    }
}

// ── ban-types post-filter ──────────────────────────────────────────────────
//
// `string & {}` is a well-known TypeScript pattern to widen a literal union
// while preserving autocomplete. `{}` is an intersection operand, not a
// standalone empty-object type annotation, so ban-types firing on it is a
// false positive. (Closes #748)

struct BanTypesFilter;

impl PostFilter for BanTypesFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        if diag.line == 0 {
            return true;
        }
        let Some(src) = source else { return true };
        let line = src.lines().nth(diag.line - 1).unwrap_or("");
        !is_intersection_member(line)
    }
}

fn is_intersection_member(line: &str) -> bool {
    let bytes = line.as_bytes();
    has_ampersand_then_empty_braces(bytes) || has_empty_braces_then_ampersand(bytes)
}

fn has_ampersand_then_empty_braces(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j + 1 < bytes.len() && bytes[j] == b'{' && bytes[j + 1] == b'}' {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn has_empty_braces_then_ampersand(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'}' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'&' {
                return true;
            }
        }
        i += 1;
    }
    false
}

// ── no-empty-interface post-filter ─────────────────────────────────────────
//
// An empty single-`extends` interface whose extends type arguments reference a
// type that participates in a recursion cycle back to the interface cannot be
// rewritten as a `type` alias — TypeScript rejects `type Foo = X<…Foo…>` with
// "circularly references itself". The empty-interface form is the documented
// workaround for recursive type aliases. Two cycle shapes are exempted:
//   1. Direct: `interface Foo extends X<Foo> {}` — own name in own extends args
//      (incl. nested generics, e.g. `extends A<B<Foo>>`).
//   2. Mutual: `interface Foo extends X<G> {}` + `type G = … | Foo | …` — the
//      extends args reference a union/intersection alias `G` that lists `Foo`
//      back (gcanti/io-ts pattern). (Closes #5293)

struct NoEmptyInterfaceFilter;

impl PostFilter for NoEmptyInterfaceFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else { return true };
        !nei_is_recursive_alias_interface(src, diag.line)
    }
}

// Reconstruct the interface declaration starting at the diagnostic line and
// decide whether it is a recursive nominal alias that must stay an interface.
fn nei_is_recursive_alias_interface(src: &str, line_1based: usize) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    // Gather the header text from the flagged line up to the opening body brace
    // `{`. Single-line declarations are the common case; allow a few lines for
    // wrapped headers.
    let end = (line_1based + 5).min(lines.len());
    let header: String = lines[line_1based - 1..end].join("\n");

    let Some((name, extends_args)) = nei_parse_extends_args(&header) else {
        return false;
    };
    let arg_names = nei_extends_arg_names(&extends_args);
    // Direct self-recursion: the interface name appears in its own extends args.
    if arg_names.iter().any(|n| n == &name) {
        return true;
    }
    // Mutual recursion: an args type name resolves to a union/intersection alias
    // that lists this interface back.
    arg_names
        .iter()
        .any(|g| g != &name && nei_alias_references(src, g, &name))
}

// Parse `interface <Name> extends <Base>(<Args>) {` from the header, returning
// the interface name and the raw text inside the extends clause's outermost
// `<…>` type-argument list. Returns `None` when there is no parameterized
// extends clause (no `<…>` before the body `{`), so a plain
// `interface Foo extends Bar {}` is never exempted here.
fn nei_parse_extends_args(header: &str) -> Option<(String, String)> {
    let after_kw = header.split_once("interface")?.1;
    let after_kw = after_kw.trim_start();
    let name: String = after_kw
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    if name.is_empty() {
        return None;
    }
    let rest = &after_kw[name.len()..];
    // Stop at the body brace; the extends clause and its args live before it.
    let body = rest.find('{').unwrap_or(rest.len());
    let heritage = &rest[..body];
    let after_extends = heritage.split_once("extends")?.1;

    // Capture the args of the first parameterized base in the heritage clause.
    // An interface extending two generic bases (`extends X<A>, Y<Foo>`) only has
    // its first base scanned — conservative: a missed self-reference keeps the
    // diagnostic, never wrongly suppresses one.
    let bytes = after_extends.as_bytes();
    let open = after_extends.find('<')?;
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    return Some((name, after_extends[open + 1..i].to_string()));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

// Collect the top-level identifier names referenced in a type-argument list,
// excluding property-access suffixes (`t.UnionType` yields `t`, not `UnionType`).
fn nei_extends_arg_names(args: &str) -> Vec<String> {
    let bytes = args.as_bytes();
    let mut names = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            // Skip the segment if it is a property access (`.Name`): the head of
            // a qualified name (`t` in `t.UnionType`) is the only identifier we
            // keep, the suffix is not a free type reference.
            let preceded_by_dot = start > 0 && bytes[start - 1] == b'.';
            if !preceded_by_dot {
                names.push(args[start..i].to_string());
            }
        } else {
            i += 1;
        }
    }
    names
}

// Whether `src` declares `type <alias> = …` whose RHS references `iface` as a
// union/intersection member or type argument — the mutual-recursion link.
fn nei_alias_references(src: &str, alias: &str, iface: &str) -> bool {
    let Some(rhs) = nei_type_alias_rhs(src, alias) else {
        return false;
    };
    nei_extends_arg_names(&rhs).iter().any(|n| n == iface)
}

// Extract the right-hand side of `type <alias> = …` up to the terminating `;`.
// Scans every `type <alias>` occurrence so a longer-named alias sharing the
// prefix (`type GenerableUnion` vs `type Generable`) does not mask the match.
fn nei_type_alias_rhs(src: &str, alias: &str) -> Option<String> {
    let needle = format!("type {alias}");
    let mut search_from = 0usize;
    while let Some(rel) = src[search_from..].find(&needle) {
        let start = search_from + rel;
        search_from = start + needle.len();
        let after = &src[start + needle.len()..];
        let Some(eq) = after.find('=') else { continue };
        // Between the alias name and `=` only whitespace is allowed, otherwise
        // this is a different, longer-named alias (`Generable` vs `GenerableX`).
        if !after[..eq].trim().is_empty() {
            continue;
        }
        let rhs = &after[eq + 1..];
        return Some(nei_alias_rhs_slice(rhs));
    }
    None
}

// Bound a type-alias RHS: stop at the first `;`, a blank line, or the next
// top-level declaration keyword, so an unterminated union doesn't swallow the
// rest of the file.
fn nei_alias_rhs_slice(rhs: &str) -> String {
    if let Some(semi) = rhs.find(';') {
        return rhs[..semi].to_string();
    }
    let mut out = String::new();
    for line in rhs.lines() {
        let trimmed = line.trim_start();
        if !out.is_empty()
            && (trimmed.is_empty()
                || trimmed.starts_with("type ")
                || trimmed.starts_with("interface ")
                || trimmed.starts_with("export ")
                || trimmed.starts_with("const ")
                || trimmed.starts_with("function "))
        {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

// ── no-unsafe-assignment post-filter ──────────────────────────────────────
//
// Drop `no-unsafe-assignment` diagnostics when casting to Vite's `PluginOption`
// type. `rollup-plugin-visualizer` returns Rollup's Plugin type, which is
// structurally valid for PluginOption but causes a type mismatch due to
// version skew in the Plugin type definitions.

struct NoUnsafeAssignmentFilter;

impl PostFilter for NoUnsafeAssignmentFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else { return true };
        !is_plugin_option_cast_fp(src, diag.line)
            && !is_vitest_asymmetric_matcher_fp(src, diag.line)
    }
}

// Vitest/Jest asymmetric-matcher factories. Each is typed to return `any`
// (their public typings), so nesting one inside an `objectContaining({ … })`
// or assigning it to a typed location trips `no-unsafe-assignment`. The matcher
// is a runtime assertion object consumed by the equality engine — there is no
// type hole to close, and rewriting to avoid the `any` is impossible without
// casting Vitest's public API. These call signatures are test-only (the `expect`
// global does not exist in production code), so a line-level signature match is a
// safe, scope-free drop (#5770).
const VITEST_ASYMMETRIC_MATCHERS: &[&str] = &[
    "expect.any(",
    "expect.anything(",
    "expect.stringMatching(",
    "expect.stringContaining(",
    "expect.objectContaining(",
    "expect.arrayContaining(",
    "expect.closeTo(",
];

fn is_vitest_asymmetric_matcher_fp(src: &str, line_1based: usize) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    let line_text = lines[line_1based - 1];
    VITEST_ASYMMETRIC_MATCHERS
        .iter()
        .any(|matcher| line_text.contains(matcher))
}

fn is_plugin_option_cast_fp(src: &str, line_1based: usize) -> bool {
    if !imports_plugin_option_from_vite(src) {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    lines[line_1based - 1].contains("as PluginOption")
}

fn imports_plugin_option_from_vite(src: &str) -> bool {
    src.lines().any(|line| {
        line.contains("PluginOption")
            && (line.contains("from \"vite\"") || line.contains("from 'vite'"))
    })
}

// ── only-throw-error post-filter ───────────────────────────────────────────
//
// TanStack Router's `notFound()` and `redirect()` return marker objects, not
// Error subclasses. The router intercepts them when thrown — this is the
// framework's documented control-flow idiom.

struct OnlyThrowErrorFilter;

impl PostFilter for OnlyThrowErrorFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else { return true };
        !is_tanstack_control_flow_fp(src, diag.line)
    }
}

fn is_tanstack_control_flow_fp(src: &str, line_1based: usize) -> bool {
    if !imports_tanstack_router(src) {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    let line_text = lines[line_1based - 1];
    line_text.contains("throw")
        && (line_text.contains("notFound(") || line_text.contains("redirect("))
}

fn imports_tanstack_router(src: &str) -> bool {
    src.contains("from \"@tanstack/react-router\"")
        || src.contains("from '@tanstack/react-router'")
        || src.contains("from \"@tanstack/router-core\"")
        || src.contains("from '@tanstack/router-core'")
}

// ── promise-function-async post-filter ────────────────────────────────────
//
// Two FP shapes are suppressed:
// 1. Functions returning an explicit non-Promise return type (e.g.
//    Effect.Effect<…>) — making them async would wrap the effect in a Promise.
// 2. Concise pass-through arrow callbacks — adding async only wraps the
//    already-pending Promise in an extra microtask with no semantic benefit.

struct PromiseFunctionAsyncFilter;

impl PostFilter for PromiseFunctionAsyncFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else { return true };
        !pfa_returns_explicit_non_promise(src, diag.line, diag.column)
            && !pfa_is_passthrough_arrow(src, diag.line, diag.column)
    }
}

fn pfa_byte_offset(src: &str, line: usize, col: usize) -> Option<usize> {
    if line == 0 || col == 0 {
        return None;
    }
    let mut offset = 0usize;
    for (idx, l) in src.lines().enumerate() {
        if idx + 1 == line {
            return Some(offset + (col - 1).min(l.len()));
        }
        offset += l.len() + 1;
    }
    None
}

fn pfa_return_type_annotation(after: &str) -> Option<String> {
    let bytes = after.as_bytes();
    let open = after.find('(')?;
    let mut depth = 0i32;
    let mut i = open;
    let mut close = None;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let close = close?;
    let mut j = close + 1;
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }
    if bytes.get(j) != Some(&b':') {
        return None;
    }
    j += 1;
    let start = j;
    let (mut angle, mut paren) = (0i32, 0i32);
    while j < bytes.len() {
        match bytes[j] {
            b'<' => angle += 1,
            b'>' if angle > 0 => angle -= 1,
            b'(' => paren += 1,
            b')' if paren > 0 => paren -= 1,
            b'{' | b';' if angle == 0 && paren == 0 => break,
            b'=' if angle == 0 && paren == 0 && bytes.get(j + 1) == Some(&b'>') => break,
            _ => {}
        }
        j += 1;
    }
    Some(after[start..j].trim().to_string())
}

fn pfa_returns_explicit_non_promise(src: &str, line: usize, col: usize) -> bool {
    let Some(offset) = pfa_byte_offset(src, line, col) else {
        return false;
    };
    if !src.is_char_boundary(offset) {
        return false;
    }
    let Some(ret) = pfa_return_type_annotation(&src[offset..]) else {
        return false;
    };
    !ret.contains("Promise")
}

fn pfa_is_passthrough_arrow(src: &str, line: usize, col: usize) -> bool {
    let Some(offset) = pfa_byte_offset(src, line, col) else {
        return false;
    };
    if !src.is_char_boundary(offset) {
        return false;
    }
    let after = &src[offset..];
    let bytes = after.as_bytes();
    let Some(open) = after.find('(') else {
        return false;
    };
    let mut depth = 0i32;
    let mut i = open;
    let mut close = None;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let Some(close) = close else {
        return false;
    };
    let mut j = close + 1;
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }
    if bytes.get(j) == Some(&b':') {
        return false;
    }
    if bytes.get(j) != Some(&b'=') || bytes.get(j + 1) != Some(&b'>') {
        return false;
    }
    j += 2;
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }
    if bytes.get(j) != Some(&b'{') {
        return true;
    }
    pfa_is_single_return_block_no_await(&after[j..])
}

fn pfa_is_single_return_block_no_await(block: &str) -> bool {
    let bytes = block.as_bytes();
    if bytes.first() != Some(&b'{') {
        return false;
    }
    let mut depth = 0i32;
    let mut content_start = None;
    let mut content_end = None;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                depth += 1;
                if depth == 1 {
                    content_start = Some(i + 1);
                }
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    content_end = Some(i);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let (start, end) = match (content_start, content_end) {
        (Some(s), Some(e)) => (s, e),
        _ => return false,
    };
    let content = block[start..end].trim();
    if content.contains("await") {
        return false;
    }
    if !content.starts_with("return") {
        return false;
    }
    let after_return = &content["return".len()..];
    if !after_return.starts_with(|c: char| c.is_whitespace() || c == ';') {
        return false;
    }
    let (mut angle, mut paren, mut bracket, mut brace) = (0i32, 0i32, 0i32, 0i32);
    let mut semicolons = 0usize;
    for b in after_return.bytes() {
        match b {
            b'<' => angle += 1,
            b'>' if angle > 0 => angle -= 1,
            b'(' => paren += 1,
            b')' if paren > 0 => paren -= 1,
            b'[' => bracket += 1,
            b']' if bracket > 0 => bracket -= 1,
            b'{' => brace += 1,
            b'}' if brace > 0 => brace -= 1,
            b';' if angle == 0 && paren == 0 && bracket == 0 && brace == 0 => semicolons += 1,
            _ => {}
        }
    }
    semicolons <= 1
}

// ── no-unnecessary-condition post-filter (composite) ─────────────────────
//
// Three FP shapes are dropped:
// 1. Elysia lifecycle-hook callbacks with `??` — `.derive` fields are
//    runtime-undefined on short-circuit paths even though TS types them as set.
// 2. Discriminated-union exhaustiveness gates — `=== "literal"` followed by a
//    `: never = <discriminant>` binding within 50 lines.
// 3. Optional chains/`??` flagged right after a non-narrowing jest/vitest
//    assertion on the same variable — `expect(V).not.toBeNull()` (and the
//    other non-narrowing matchers) does not narrow `V`'s static type, so a
//    later `V?.…` / `V ?? …` is still required by `tsc`.

const NUC_ELYSIA_HOOK_OPENERS: &[&str] = &[
    ".mapResponse(",
    ".onError(",
    ".onResponse(",
    ".onAfterResponse(",
    ".onRequest(",
    ".onTransform(",
    ".onParse(",
    ".onBeforeHandle(",
    ".onAfterHandle(",
    ".beforeHandle(",
    ".afterHandle(",
];

struct NoUnnecessaryConditionFilter;

impl PostFilter for NoUnnecessaryConditionFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else {
            return true;
        };
        !nuc_is_elysia_lifecycle_nullish_fp(src, diag.line)
            && !nuc_is_exhaustiveness_gate_fp(src, diag.line)
            && !nuc_is_jest_nonnull_assertion_fp(src, diag.line)
    }
}

fn nuc_is_elysia_lifecycle_nullish_fp(src: &str, line_1based: usize) -> bool {
    if !nuc_imports_elysia(src) {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    let line_text = lines[line_1based - 1];
    if !line_text.contains("??") {
        return false;
    }
    let start = line_1based.saturating_sub(100).max(1);
    for i in (start..line_1based).rev() {
        let l = lines[i - 1];
        if NUC_ELYSIA_HOOK_OPENERS.iter().any(|h| l.contains(h)) {
            return true;
        }
    }
    false
}

fn nuc_imports_elysia(src: &str) -> bool {
    src.contains("from \"elysia\"") || src.contains("from 'elysia'")
}

fn nuc_is_exhaustiveness_gate_fp(src: &str, line_1based: usize) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    let flagged = lines[line_1based - 1];
    let Some(lhs) = nuc_extract_comparison_lhs(flagged) else {
        return false;
    };
    if lhs.is_empty() {
        return false;
    }
    let needle = format!(": never = {lhs}");
    let window_start = line_1based;
    let window_end = (window_start + 50).min(lines.len());
    lines[window_start..window_end]
        .iter()
        .any(|l| l.contains(&needle))
}

// Non-narrowing jest/vitest matchers: they assert non-nullishness at runtime
// but do NOT narrow the variable's static type, so `tsc` still requires a
// later `?.` / `??` on the same variable.
const NUC_JEST_NONNULL_MATCHERS: &[&str] = &[
    ".not.toBeNull(",
    ".not.toBeUndefined(",
    ".toBeDefined(",
    ".toBeTruthy(",
    ".not.toBeFalsy(",
];

fn nuc_is_jest_nonnull_assertion_fp(src: &str, line_1based: usize) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    let flagged = lines[line_1based - 1];
    let Some(var) = nuc_extract_chain_root(flagged) else {
        return false;
    };
    let start = line_1based.saturating_sub(25).max(1);
    (start..line_1based)
        .rev()
        .map(|i| lines[i - 1])
        .any(|l| nuc_line_has_nonnull_assertion(l, &var))
}

// The root identifier of the first optional-chain (`?.`) or nullish-coalescing
// (`??`) on the line: walk left over identifier characters from that operator.
fn nuc_extract_chain_root(line: &str) -> Option<String> {
    let opt = nuc_find_substr(line, "?.");
    let coalesce = nuc_find_substr(line, "??");
    let op_idx = match (opt, coalesce) {
        (None, None) => return None,
        (Some(i), None) => i,
        (None, Some(j)) => j,
        (Some(i), Some(j)) => i.min(j),
    };
    let prefix = &line[..op_idx];
    let start = prefix
        .char_indices()
        .rev()
        .find(|(_, c)| !nuc_is_ident_char(*c))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let root = &prefix[start..];
    if root.is_empty() {
        return None;
    }
    Some(root.to_owned())
}

// True when the line asserts non-nullishness on exactly `var` via a
// non-narrowing matcher: `expect(<var>)` (word-boundaried on `var`, allowing
// whitespace inside the parens) followed by one of the matchers on the line.
fn nuc_line_has_nonnull_assertion(line: &str, var: &str) -> bool {
    let mut search_from = 0;
    while let Some(rel) = nuc_find_substr(&line[search_from..], "expect(") {
        let open = search_from + rel + "expect(".len();
        let inner = line[open..].trim_start();
        if let Some(after_var) = inner.strip_prefix(var) {
            // Word boundary: the char after `var` must not extend the identifier.
            let boundary_ok = after_var
                .chars()
                .next()
                .is_none_or(|c| !nuc_is_ident_char(c));
            if boundary_ok
                && after_var.trim_start().starts_with(')')
                && NUC_JEST_NONNULL_MATCHERS.iter().any(|m| line.contains(m))
            {
                return true;
            }
        }
        search_from = open;
    }
    false
}

fn nuc_is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

fn nuc_extract_comparison_lhs(line: &str) -> Option<String> {
    let op_idx = nuc_find_first_op(line)?;
    let raw = line[..op_idx].trim();
    if raw.is_empty() {
        return Some(String::new());
    }
    let start = raw
        .char_indices()
        .rev()
        .skip_while(|(_, c)| c.is_alphanumeric() || *c == '_' || *c == '$' || *c == '.')
        .next()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    Some(raw[start..].to_owned())
}

fn nuc_find_first_op(line: &str) -> Option<usize> {
    let eq3 = nuc_find_substr(line, "===");
    let neq = nuc_find_substr(line, "!==");
    match (eq3, neq) {
        (None, None) => None,
        (Some(i), None) => Some(i),
        (None, Some(j)) => Some(j),
        (Some(i), Some(j)) => Some(i.min(j)),
    }
}

fn nuc_find_substr(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|w| w == needle.as_bytes())
}

// ── no-misused-spread post-filter ─────────────────────────────────────────
//
// Drops FPs when spreading a class instance into a `new <X>Error(...)` call.
// Library error constructors (Better Auth's APIError, etc.) take a plain
// Record body — spreading is intentional interop. (Closes #554)

struct NoMisusedSpreadFilter;

impl PostFilter for NoMisusedSpreadFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else {
            return true;
        };
        !nms_is_error_constructor_interop_spread(src, diag.line)
    }
}

fn nms_is_error_constructor_interop_spread(src: &str, line_1based: usize) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    if !lines[line_1based - 1].contains("...") {
        return false;
    }
    let start = line_1based.saturating_sub(8);
    let end = (line_1based + 1).min(lines.len());
    let window = lines[start..end].join("\n");
    window.contains("new ") && window.contains("Error(")
}

// ── unified-signatures post-filter ─────────────────────────────────────────
//
// `unified-signatures` flags overload pairs it believes collapse into one
// signature with a union or optional parameter. But when overloads declare
// generic type parameters whose constraints differ between them, each overload
// encodes a distinct, per-overload type relationship that a single merged
// signature cannot express. The canonical case is the DOM `addEventListener`
// typed-wrapper pattern (used by `lib.dom.d.ts` itself): each overload pairs a
// target constraint with the matching event map —
//
//   function on<T extends Window,   U extends keyof WindowEventMap>(t: T, e: U, …): R;
//   function on<T extends Document, U extends keyof DocumentEventMap>(t: T, e: U, …): R;
//
// Here `U`'s constraint (`keyof <X>EventMap`) is correlated with `T`'s
// constraint (`X`), and that correlation differs per overload; unifying into a
// union parameter would erase which event names are valid for which target.
//
// Drop the diagnostic when the overload group it points at has two members
// whose type-parameter constraint lists differ. Overloads with no generics, or
// with identical generic constraints (differing only in a unionizable value
// parameter or an optional trailing parameter), stay flagged. (Closes #5506)

struct UnifiedSignaturesFilter;

impl PostFilter for UnifiedSignaturesFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        if diag.line == 0 {
            return true;
        }
        let Some(src) = source else {
            return true;
        };
        !us_overload_group_has_divergent_generic_constraints(src, &diag.path, diag.line)
    }
}

/// True when the function-overload group whose signatures span `line_1based`
/// contains two members with differing type-parameter constraint lists. Parses
/// the file and inspects every overload `Function` (a declaration with no body)
/// sharing the diagnostic's name; correlated-generic overloads (DOM
/// `addEventListener` style) carry distinct constraints and are not unifiable.
fn us_overload_group_has_divergent_generic_constraints(
    src: &str,
    path: &std::path::Path,
    line_1based: usize,
) -> bool {
    use crate::oxc_helpers::{byte_offset_to_line_col, with_oxc_parse};
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;

    with_oxc_parse(src, path, |semantic| {
        // Group overload signatures (body-less function declarations) by name,
        // keeping each member's source line range and its constraint fingerprint.
        let mut groups: rustc_hash::FxHashMap<&str, Vec<(usize, usize, String)>> =
            rustc_hash::FxHashMap::default();
        for node in semantic.nodes().iter() {
            let AstKind::Function(func) = node.kind() else {
                continue;
            };
            if func.body.is_some() {
                continue; // implementation signature, not an overload
            }
            let Some(id) = func.id.as_ref() else {
                continue;
            };
            let span = func.span();
            let (start_line, _) = byte_offset_to_line_col(src, span.start as usize);
            let (end_line, _) = byte_offset_to_line_col(src, span.end as usize);
            let fingerprint = us_type_param_constraints(func, src);
            groups
                .entry(id.name.as_str())
                .or_default()
                .push((start_line, end_line, fingerprint));
        }

        groups.values().any(|members| {
            if !members.iter().any(|(start, end, _)| line_1based >= *start && line_1based <= *end) {
                return false;
            }
            // Divergent when any two members carry different constraint lists,
            // and at least one of them actually declares constrained generics.
            let mut iter = members.iter().map(|(_, _, fp)| fp);
            let Some(first) = iter.next() else {
                return false;
            };
            let all_same = iter.clone().all(|fp| fp == first);
            let any_constrained =
                members.iter().any(|(_, _, fp)| !fp.is_empty());
            !all_same && any_constrained
        })
    })
}

/// A stable fingerprint of an overload's generic type-parameter constraints:
/// the source text of each `T extends …` constraint, joined. Empty when the
/// overload has no type parameters or none of them are constrained. Two
/// overloads with equal fingerprints have interchangeable generics.
fn us_type_param_constraints(func: &oxc_ast::ast::Function, src: &str) -> String {
    use oxc_span::GetSpan;
    let Some(params) = func.type_parameters.as_ref() else {
        return String::new();
    };
    let mut parts: Vec<&str> = Vec::new();
    for param in &params.params {
        if let Some(constraint) = param.constraint.as_ref() {
            let span = constraint.span();
            parts.push(&src[span.start as usize..span.end as usize]);
        }
    }
    parts.join("|")
}

// ── no-deprecated post-filter ──────────────────────────────────────────────
//
// Two false-positive shapes are dropped:
//
//  1. A re-export forwards a symbol for backward compatibility — it is not a use
//     of the deprecated API. tsgolint flags the specifier inside the re-export,
//     e.g. `export { Line } from './shapes/Line'` where `Line` is `@deprecated`.
//     Dropping the export here means library consumers are still warned at their
//     own import sites while the maintainer keeps the compat barrel. (Closes
//     #5325)
//
//  2. A use inside a test file. Backward-compat test suites exist to verify a
//     deprecated API still works for existing consumers, so they must call it —
//     flagging that call is circular. This mirrors the test-file suppression
//     that typescript-eslint documents for sibling rules (await-thenable,
//     unbound-method). (Closes #5326)
//
// Genuine uses (calling, instantiating, referencing, or import-and-use of a
// deprecated symbol) in production code still fire.

struct NoDeprecatedFilter;

impl PostFilter for NoDeprecatedFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        if is_test_path(&diag.path) {
            return false;
        }
        let Some(src) = source else {
            return true;
        };
        !nd_is_reexport_specifier(src, diag.line)
    }
}

// True when the diagnostic's line is part of an `export … from '…'` re-export
// statement. Covers the named (`export { X } from`, `export { X as Y } from`)
// and namespace (`export * from`, `export * as NS from`) forms. A local
// `export { X }` without a `from` clause is the deprecated declaration itself
// and is not exempted.
fn nd_is_reexport_specifier(src: &str, line_1based: usize) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    // Find the line that starts the statement enclosing the diagnostic: scan
    // upward (the specifier may sit several lines below `export {`) for a line
    // whose first token is `export`. Bound the walk so an unterminated brace
    // can't run off to the top of the file.
    let mut start = None;
    for i in (1..=line_1based).rev() {
        if line_1based - i > 50 {
            break;
        }
        if lines[i - 1].trim_start().starts_with("export") {
            start = Some(i);
            break;
        }
    }
    let Some(start) = start else {
        return false;
    };
    // Determine the statement span and whether it is a re-export. The diagnostic
    // line must fall *within* that span — otherwise a genuine use sitting below
    // an unrelated `export … from` (common in barrel files) would be wrongly
    // dropped.
    let end = (start + 50).min(lines.len());
    let stmt = lines[start - 1..end].join("\n");
    let Some(stmt_line_span) = nd_reexport_line_span(&stmt) else {
        return false;
    };
    line_1based <= start + stmt_line_span
}

// When `stmt` (starting at an `export` line) is an `export … from '…'`
// re-export, returns the number of additional lines the statement spans past
// its first line (0 for a single-line re-export); `None` when it is not a
// re-export with a module source. The end is the line that carries the
// `from '…'` clause.
fn nd_reexport_line_span(stmt: &str) -> Option<usize> {
    let rest = stmt.trim_start().strip_prefix("export")?;
    // Limit to the first statement: cut at the first `;`.
    let stmt_body = match rest.find(';') {
        Some(i) => &rest[..i],
        None => rest,
    };
    let from_at = nd_from_source_offset(stmt_body)?;
    // Re-anchor the offset onto the original `stmt` (it was trimmed/prefixed).
    let prefix_len = stmt.len() - stmt.trim_start().len() + "export".len();
    let abs = prefix_len + from_at;
    Some(stmt[..abs].matches('\n').count())
}

// Byte offset, within `stmt_body`, of a `from` keyword followed by a
// string-literal module source (single or double quoted) — i.e. a re-export's
// `from '…'` clause. `None` when the body has no such clause (a local export or
// a declaration). The keyword is word-boundaried so `from` inside an identifier
// (e.g. `computeFrom`) does not match.
fn nd_from_source_offset(stmt_body: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(rel) = stmt_body[search_from..].find("from") {
        let at = search_from + rel;
        let before_ok = at == 0
            || !stmt_body[..at]
                .chars()
                .next_back()
                .is_some_and(nd_is_ident_char);
        let after = &stmt_body[at + "from".len()..];
        let after_ok = after.chars().next().is_none_or(|c| !nd_is_ident_char(c));
        if before_ok && after_ok {
            let src_part = after.trim_start();
            if src_part.starts_with('\'') || src_part.starts_with('"') {
                return Some(at);
            }
        }
        search_from = at + "from".len();
    }
    None
}

fn nd_is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

// ── no-redundant-type-constituents post-filter ────────────────────────────
//
// Drops two false-positive shapes:
//
//  1. The `keyof T & string` narrowing idiom. The `& string` constituent is
//     intentional — it filters out numeric/symbol keys that `keyof T` can
//     include. Checked in a ±5-line window around the diagnostic to handle
//     multi-line satisfies clauses.
//
//  2. The "error type that acts as 'any'" diagnostic on a constituent that is
//     an imported type reference. When the type-aware backend cannot resolve an
//     external package's types, those references degrade to the `error` type,
//     which behaves like `any` and collapses the union. An unresolved import is
//     not a genuine `any` — it carries a real (if locally-opaque) type — so the
//     other union members are not actually redundant. Genuine `any`/`never`
//     constituents use the separate "overrides all other types" message and
//     stay flagged.

const NRTC_WINDOW: usize = 5;

struct NoRedundantTypeConstituentsFilter;

impl PostFilter for NoRedundantTypeConstituentsFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else {
            return true;
        };
        if nrtc_is_keyof_string_narrowing_fp(src, diag.line) {
            return false;
        }
        !nrtc_is_unresolved_import_error_type_fp(&diag.message, src)
    }
}

/// True when the diagnostic is the "error type that acts as 'any'" variant *and*
/// the flagged type name is brought in by an `import` in this file. tsgolint
/// emits this message only for the `error` type; an external import the backend
/// could not resolve degrades to `error`, so flagging the rest of the union as
/// redundant is a false positive. Genuine `any`/`never` constituents use the
/// distinct "overrides all other types" message and are never matched here.
fn nrtc_is_unresolved_import_error_type_fp(message: &str, src: &str) -> bool {
    let Some(type_name) = nrtc_error_type_name(message) else {
        return false;
    };
    nrtc_is_imported_ident(src, type_name)
}

/// Extracts the leading identifier of the flagged type from an "error type that
/// acts as 'any'" message. The message starts with `'<typeName>' is an 'error'
/// type …`; for a generic instantiation (`Feature<any>`) only the base
/// identifier (`Feature`) is returned, since that is what an import binds.
fn nrtc_error_type_name(message: &str) -> Option<&str> {
    const MARKER: &str = "is an 'error' type that acts as 'any'";
    if !message.contains(MARKER) {
        return None;
    }
    let rest = message.strip_prefix('\'')?;
    let quoted = &rest[..rest.find('\'')?];
    let base = quoted.split('<').next()?.trim();
    let ident_end = base
        .find(|c: char| !nrtc_is_ident_byte(c as u8))
        .unwrap_or(base.len());
    let ident = &base[..ident_end];
    if ident.is_empty() {
        None
    } else {
        Some(ident)
    }
}

/// True when `name` appears as an imported binding in any `import` statement of
/// the source — named (`import { Name }`), aliased (`import { X as Name }`),
/// default (`import Name from`) or namespace (`import * as Name`). Each import
/// statement is scanned as a block, so multi-line named-import lists are handled,
/// and the trailing module specifier is excluded so a package name containing
/// the type identifier is not mistaken for a binding.
fn nrtc_is_imported_ident(src: &str, name: &str) -> bool {
    nrtc_import_binding_regions(src).any(|region| nrtc_text_binds_ident(region, name))
}

/// Yields, for each `import` statement, the slice of source covering its binding
/// clause — from just after the `import` keyword up to the `from` keyword (or the
/// statement's `;`/newline for side-effect-free forms). The module-specifier
/// string is never included, so identifiers inside a package name are ignored.
fn nrtc_import_binding_regions(src: &str) -> impl Iterator<Item = &str> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = src[search_from..].find("import") {
        let kw_start = search_from + rel;
        let kw_end = kw_start + "import".len();
        let before_ok = kw_start == 0 || !nrtc_is_ident_byte(bytes[kw_start - 1]);
        let after_ok = kw_end >= bytes.len()
            || bytes[kw_end].is_ascii_whitespace()
            || bytes[kw_end] == b'{'
            || bytes[kw_end] == b'*';
        search_from = kw_end;
        if !before_ok || !after_ok {
            continue;
        }
        let rest = &src[kw_end..];
        // Bindings end at the ` from ` keyword; statements with no `from`
        // (`import 'side-effect'`) end at the next `;` or newline.
        let region_end = nrtc_find_from_keyword(rest)
            .or_else(|| rest.find([';', '\n']))
            .unwrap_or(rest.len());
        out.push(&rest[..region_end]);
        search_from = kw_end + region_end;
    }
    out.into_iter()
}

/// Finds the byte offset of the ` from ` keyword (whitespace-delimited) in an
/// import statement's binding region. Matches the keyword form, not the substring
/// inside identifiers like `fromList`.
fn nrtc_find_from_keyword(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let needle = b"from";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let before_ok = i == 0 || bytes[i - 1].is_ascii_whitespace();
            let after = i + needle.len();
            let after_ok = after >= bytes.len()
                || bytes[after].is_ascii_whitespace()
                || bytes[after] == b'\'';
            if before_ok && after_ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn nrtc_text_binds_ident(region: &str, name: &str) -> bool {
    let bytes = region.as_bytes();
    let nb = name.as_bytes();
    let mut i = 0;
    while i + nb.len() <= bytes.len() {
        if &bytes[i..i + nb.len()] == nb {
            let before_ok = i == 0 || !nrtc_is_ident_byte(bytes[i - 1]);
            let after = i + nb.len();
            let after_ok = after >= bytes.len() || !nrtc_is_ident_byte(bytes[after]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn nrtc_is_keyof_string_narrowing_fp(src: &str, line_1based: usize) -> bool {
    if line_1based == 0 {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    let lo = line_1based.saturating_sub(NRTC_WINDOW + 1);
    let hi = (line_1based + NRTC_WINDOW).min(lines.len());
    lines[lo..hi]
        .iter()
        .any(|line| nrtc_has_keyof_string_intersection(line))
}

fn nrtc_has_keyof_string_intersection(line: &str) -> bool {
    if !line.contains("keyof") {
        return false;
    }
    let bytes = line.as_bytes();
    let needle = b"& string";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if bytes[i..i + needle.len()] == *needle {
            let after = i + needle.len();
            let after_ok = after >= bytes.len() || !nrtc_is_ident_byte(bytes[after]);
            if after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn nrtc_is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

// ── no-unnecessary-type-parameters post-filter (equal-probe) ──────────────
//
// Two FP shapes are dropped:
// 1. The type-challenges `Equal<X, Y>` probe idiom: `<T>() => T extends X ? 1 : 2`.
//    The `<T>` is load-bearing for the structural comparison.
// 2. Multi-line function/overload signatures where the type parameter is
//    referenced in the parameter list on a line below its declaration (e.g.
//    inside a union member of a callback parameter type). tsgolint misses
//    these later references and wrongly reports the parameter as unused.

struct EqualProbeFilter;

impl PostFilter for EqualProbeFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else {
            return true;
        };
        !ep_is_equal_probe_fp(src, diag.line) && !ep_is_multiline_param_fp(src, diag.line)
    }
}

fn ep_is_equal_probe_fp(src: &str, line_1based: usize) -> bool {
    if line_1based == 0 {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    let Some(&line) = lines.get(line_1based - 1) else {
        return false;
    };
    if ep_has_unit_conditional(line) {
        return true;
    }
    if ep_has_generic_arrow_fn(line) {
        for next_line in lines.iter().skip(line_1based).take(3) {
            if ep_has_unit_conditional_expr(next_line) {
                return true;
            }
        }
    }
    false
}

fn ep_is_multiline_param_fp(src: &str, line_1based: usize) -> bool {
    if line_1based == 0 {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    let Some(&decl_line) = lines.get(line_1based - 1) else {
        return false;
    };
    let Some(param_name) = ep_extract_type_param_name(decl_line) else {
        return false;
    };
    let mut paren_depth: i32 = 0;
    for b in decl_line.bytes() {
        match b {
            b'(' => paren_depth += 1,
            b')' => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    break;
                }
            }
            _ => {}
        }
    }
    for next_line in lines.iter().skip(line_1based).take(15) {
        // The portion of this line that belongs to the parameter list, i.e. up to
        // the byte that closes the signature's parens or opens its body. A type
        // parameter referenced anywhere in that region is a genuine signature use
        // that tsgolint missed because the reference sits on a later line.
        let mut param_region_end = next_line.len();
        let mut closed = false;
        for (idx, b) in next_line.bytes().enumerate() {
            match b {
                b'(' => paren_depth += 1,
                b')' => {
                    paren_depth -= 1;
                    if paren_depth < 0 {
                        param_region_end = idx;
                        closed = true;
                        break;
                    }
                }
                b'{' if paren_depth <= 0 => {
                    param_region_end = idx;
                    closed = true;
                    break;
                }
                _ => {}
            }
        }
        if ep_contains_word(&next_line[..param_region_end], &param_name) {
            return true;
        }
        if closed {
            break;
        }
    }
    false
}

fn ep_has_unit_conditional(line: &str) -> bool {
    if !ep_has_generic_arrow_fn(line) {
        return false;
    }
    ep_has_unit_conditional_expr(line)
}

fn ep_has_unit_conditional_expr(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 7 <= bytes.len() {
        if &bytes[i..i + 7] == b"extends"
            && (i == 0 || !ep_is_ident_byte(bytes[i - 1]))
            && (i + 7 == bytes.len() || !ep_is_ident_byte(bytes[i + 7]))
        {
            if let Some((q_pos, c_pos)) = ep_find_ternary_after(line, i + 7) {
                let arm1 = line[q_pos + 1..c_pos].trim();
                let rest = &line[c_pos + 1..];
                let end = rest
                    .find(|c: char| c == ')' || c == ',' || c == ';')
                    .unwrap_or(rest.len());
                let arm2 = rest[..end].trim();
                if ep_is_unit_literal(arm1) && ep_is_unit_literal(arm2) {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn ep_has_generic_arrow_fn(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let start = i + 1;
        if start >= bytes.len() || !ep_is_ident_start(bytes[start]) {
            i += 1;
            continue;
        }
        let Some(close) = ep_find_matching_angle(bytes, i + 1) else {
            i += 1;
            continue;
        };
        let after_gt = close + 1;
        if after_gt >= bytes.len() || bytes[after_gt] != b'(' {
            i += 1;
            continue;
        }
        let after_open = after_gt + 1;
        if after_open >= bytes.len() || bytes[after_open] != b')' {
            i += 1;
            continue;
        }
        return true;
    }
    false
}

fn ep_find_matching_angle(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth: i32 = 1;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn ep_extract_type_param_name(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let start = i + 1;
            if start < bytes.len() && ep_is_ident_start(bytes[start]) {
                let mut j = start;
                while j < bytes.len() && ep_is_ident_byte(bytes[j]) {
                    j += 1;
                }
                if j > start {
                    return Some(String::from_utf8_lossy(&bytes[start..j]).into_owned());
                }
            }
        }
        i += 1;
    }
    None
}

fn ep_contains_word(line: &str, word: &str) -> bool {
    let word_bytes = word.as_bytes();
    let line_bytes = line.as_bytes();
    if word_bytes.is_empty() || line_bytes.len() < word_bytes.len() {
        return false;
    }
    let mut i = 0;
    while i + word_bytes.len() <= line_bytes.len() {
        if &line_bytes[i..i + word_bytes.len()] == word_bytes {
            let before_ok = i == 0 || !ep_is_ident_byte(line_bytes[i - 1]);
            let after_pos = i + word_bytes.len();
            let after_ok = after_pos >= line_bytes.len() || !ep_is_ident_byte(line_bytes[after_pos]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn ep_find_ternary_after(line: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut depth_paren: i32 = 0;
    let mut depth_angle: i32 = 0;
    let mut i = from;
    let mut q_pos: Option<usize> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'(' => depth_paren += 1,
            b')' => depth_paren -= 1,
            b'<' => depth_angle += 1,
            b'>' => depth_angle -= 1,
            b'?' if depth_paren == 0 && depth_angle == 0 => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'?' {
                    i += 2;
                    continue;
                }
                q_pos = Some(i);
                break;
            }
            _ => {}
        }
        i += 1;
    }
    let q = q_pos?;
    let mut depth_paren: i32 = 0;
    let mut depth_angle: i32 = 0;
    let mut j = q + 1;
    while j < bytes.len() {
        let b = bytes[j];
        match b {
            b'(' => depth_paren += 1,
            b')' => depth_paren -= 1,
            b'<' => depth_angle += 1,
            b'>' => depth_angle -= 1,
            b':' if depth_paren == 0 && depth_angle == 0 => return Some((q, j)),
            _ => {}
        }
        j += 1;
    }
    None
}

fn ep_is_unit_literal(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    matches!(s, "true" | "false" | "null" | "undefined")
        || ep_is_numeric_literal(s)
        || ep_is_string_literal(s)
}

fn ep_is_numeric_literal(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let start = if bytes[0] == b'-' || bytes[0] == b'+' { 1 } else { 0 };
    let rest = &s[start..];
    !rest.is_empty()
        && rest.chars().all(|c| c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '_')
}

fn ep_is_string_literal(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
            || (bytes[0] == b'`' && bytes[bytes.len() - 1] == b'`'))
}

fn ep_is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

fn ep_is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

// ── strict-void-return post-filter ────────────────────────────────────────
//
// Four FP shapes are dropped:
// 1. `vi.fn()` mocks — inline or aliased via const/let/var. (Closes #…)
// 2. `renderHook(() => …)` callbacks — the callback must return the hook
//    value. A 2-line window is used to avoid bleeding into adjacent calls.
// 3. `new Promise(r => setTimeout(r, ms))` executors — the executor's `void`
//    return type discards the timer handle (the canonical sleep idiom).
// 4. Concise arrows whose body is a side-effecting collection mutation
//    (`x => acc.push(x)`, `() => set.add(x)`) in a void-callback slot — the
//    mutator's return value is incidental and discarded.

struct StrictVoidReturnFilter;

impl PostFilter for StrictVoidReturnFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else {
            return true;
        };
        !svr_is_vi_fn_fp(src, diag.line, diag.column)
            && !svr_is_render_hook_fp(src, diag.line)
            && !svr_is_promise_executor_timer_fp(src, diag.line)
            && !svr_is_concise_mutator_fp(src, diag.line, diag.column)
    }
}

/// Collection-mutation methods whose return value is incidental — the call is
/// made for its side effect, the returned length / set / boolean is discarded.
const SVR_MUTATOR_METHODS: &[&str] = &[
    "push", "unshift", "splice", "add", "set", "delete", "clear",
];

/// True when the diagnostic sits on a concise-body arrow (`=>` not opening a
/// block) whose body *is* a method call to a known collection mutator
/// (`(x) => acc.push(x)`, `() => set.add(x)`). TypeScript permits a `() => void`
/// callback to return any type — the mutator's result is discarded, only its
/// side effect matters, so the incidental return is not a real value leak.
///
/// The body must be the bare mutator call: `a.push(x) + 1` or `cond ? a.push(x)
/// : f()` still return a real value and stay flagged.
fn svr_is_concise_mutator_fp(src: &str, line_1based: usize, column_1based: usize) -> bool {
    let Some(line) = src.lines().nth(line_1based.saturating_sub(1)) else {
        return false;
    };
    // Anchor to the offending arrow: slice from the diagnostic column so a line
    // with multiple callbacks resolves to the right one.
    let from = column_1based.saturating_sub(1).min(line.len());
    if !line.is_char_boundary(from) {
        return false;
    }
    let Some(rel_arrow) = line[from..].find("=>") else {
        return false;
    };
    let body = line[from + rel_arrow + 2..].trim_start();
    // Concise body only: a block body `=> { … }` is never flagged by the rule.
    if body.starts_with('{') {
        return false;
    }
    svr_body_is_mutator_call(body)
}

/// True when `body` *is* a method call `<receiver-chain>.<mutator>(…)` for one
/// of the known collection mutators: the receiver chain starts at the body head
/// and only enclosing-context closers (`,`/`;`/`)`/`}`) follow the closing paren
/// — so the call is the whole body, not a sub-expression of a larger value.
fn svr_body_is_mutator_call(body: &str) -> bool {
    let bytes = body.as_bytes();
    // Consume the receiver member-access chain from the head; it must end at the
    // call's opening paren (`acc.push` → stops at `(`, `this.items.add` → `(`).
    let mut open = 0;
    while open < bytes.len() && svr_is_receiver_byte(bytes[open]) {
        open += 1;
    }
    // The chain must be followed by `(` and contain a `.` (a method call, not a
    // bare identifier like `(push) => push`).
    if bytes.get(open) != Some(&b'(') {
        return false;
    }
    let chain = &body[..open];
    let Some(dot) = chain.rfind('.') else {
        return false;
    };
    let receiver = &chain[..dot];
    let method = &chain[dot + 1..];
    // Receiver must be a non-empty member-access chain; method a known mutator.
    if receiver.is_empty() || !SVR_MUTATOR_METHODS.contains(&method) {
        return false;
    }
    let Some(close) = svr_matching_paren(body, open) else {
        return false;
    };
    // After the call, only closers of the enclosing context may remain —
    // `,`/`;`/`)`/`}` (the wrapping call, object, or block). Anything else
    // (`+ 1`, `|| f()`) means the call is a sub-expression of a larger value.
    body[close + 1..]
        .trim()
        .trim_matches([',', ';', ')', '}', ' ', '\t'])
        .is_empty()
}

/// A byte that may appear in a receiver member-access chain head:
/// `acc`, `this.items`, `a?.b`, `obj!.set` → idents plus `.`/`?`/`!`.
fn svr_is_receiver_byte(b: u8) -> bool {
    svr_is_ident_byte(b) || b == b'.' || b == b'?' || b == b'!'
}

/// Index of the `)` matching the `(` at `open`, accounting for nesting. Returns
/// `None` if unbalanced (e.g. the call spills onto the next line).
fn svr_matching_paren(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    for (idx, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }
    None
}

/// Timer / scheduling APIs whose return value is idiomatically discarded.
const SVR_TIMER_CALLS: &[&str] = &[
    "setTimeout(",
    "setInterval(",
    "clearTimeout(",
    "clearInterval(",
    "setImmediate(",
    "requestAnimationFrame(",
    "cancelAnimationFrame(",
    "queueMicrotask(",
];

/// True when the diagnostic sits on a `new Promise(...)` executor whose concise
/// body returns a timer/scheduling call (`new Promise(r => setTimeout(r, ms))`).
/// The Promise executor is typed `(resolve, reject) => void`, so TypeScript
/// permits and discards the returned timer handle — the canonical sleep idiom,
/// not a leaked value.
fn svr_is_promise_executor_timer_fp(src: &str, line_1based: usize) -> bool {
    let Some(line) = src.lines().nth(line_1based.saturating_sub(1)) else {
        return false;
    };
    line.contains("new Promise(") && SVR_TIMER_CALLS.iter().any(|t| line.contains(t))
}

fn svr_is_vi_fn_fp(src: &str, line_1based: usize, column_1based: usize) -> bool {
    let Some(line) = src.lines().nth(line_1based.saturating_sub(1)) else {
        return false;
    };
    if svr_has_vi_fn_call_at_or_after(line, column_1based) {
        return true;
    }
    let Some(ident) = svr_identifier_at(line, column_1based) else {
        return false;
    };
    svr_is_vi_fn_alias(src, &ident)
}

fn svr_has_vi_fn_call_at_or_after(line: &str, column_1based: usize) -> bool {
    let col0 = column_1based.saturating_sub(1).min(line.len());
    line[col0..].contains("vi.fn(")
}

fn svr_identifier_at(line: &str, column_1based: usize) -> Option<String> {
    let bytes = line.as_bytes();
    if column_1based == 0 || column_1based > bytes.len() {
        return None;
    }
    let start = column_1based - 1;
    if !line.is_char_boundary(start) {
        return None;
    }
    if !svr_is_ident_start(bytes[start]) {
        return None;
    }
    let mut end = start;
    while end < bytes.len() && svr_is_ident_byte(bytes[end]) {
        end += 1;
    }
    if !line.is_char_boundary(end) {
        return None;
    }
    Some(line[start..end].to_string())
}

fn svr_is_vi_fn_alias(src: &str, ident: &str) -> bool {
    for line in src.lines() {
        let trimmed = line.trim_start();
        for kw in ["const ", "let ", "var "] {
            if let Some(rest) = trimmed.strip_prefix(kw) {
                let rest = rest.trim_start();
                if let Some(after_ident) = rest.strip_prefix(ident) {
                    let next = after_ident.as_bytes().first().copied();
                    if next.is_none_or(|b| !svr_is_ident_byte(b)) && after_ident.contains("vi.fn(") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn svr_is_render_hook_fp(src: &str, line_1based: usize) -> bool {
    if line_1based == 0 {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    if line_1based > lines.len() {
        return false;
    }
    let start = line_1based.saturating_sub(1).max(1);
    for i in (start..=line_1based).rev() {
        let line = lines[i - 1];
        if svr_contains_render_hook_call(line) {
            return true;
        }
    }
    false
}

fn svr_contains_render_hook_call(line: &str) -> bool {
    let needle = "renderHook(";
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(pos) = line[from..].find(needle) {
        let abs = from + pos;
        let ok = abs == 0 || !svr_is_ident_byte(bytes[abs - 1]);
        if ok {
            return true;
        }
        from = abs + needle.len();
    }
    false
}

fn svr_is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

fn svr_is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

// ── no-invalid-void-type post-filter ───────────────────────────────────────
//
// `no-invalid-void-type` flags `void` used outside the two positions the rule's
// own message names as valid: a return type or a generic type argument. The
// false positive: `void` as a union constituent in a *return-type* position —
// `(get) => Cleanup | void | Promise<Cleanup | void>` (pmndrs/valtio, RTK
// Query). `Cleanup | void` in a return type is the idiomatic "returns a cleanup
// function or nothing" callback contract, and TypeScript accepts it. The `void`
// nested in `Promise<…>` is a generic type argument, also valid.
//
// The delegated diagnostic arrives from oxlint with no AST, anchored on the
// `void` keyword. The filter re-parses the file, finds the `TSVoidKeyword`
// covering that position, and drops the diagnostic when an ancestor walk shows
// the `void` sits in a function/callback return type or a generic type-argument
// list. A `void` in a non-return position — `let x: string | void`, a parameter
// annotation, a `<T extends void>` constraint — is still flagged. Failing safe:
// an unreadable source or a position that does not resolve to a `void` keyword
// keeps the diagnostic. (Closes #5609)

struct NoInvalidVoidTypeFilter;

impl PostFilter for NoInvalidVoidTypeFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else {
            return true;
        };
        let Some(offset) = nivt_byte_offset(src, diag.line, diag.column) else {
            return true;
        };
        !nivt_void_at_offset_is_valid_position(src, &diag.path, offset)
    }
}

/// Byte offset of the `(line, column)` position (both 1-based) into `src`.
fn nivt_byte_offset(src: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }
    let mut offset = 0usize;
    for (idx, l) in src.lines().enumerate() {
        if idx + 1 == line {
            return Some(offset + (column - 1).min(l.len()));
        }
        offset += l.len() + 1;
    }
    None
}

/// Re-parse `src` and report whether the `void` keyword covering byte `offset`
/// sits in a position the rule explicitly permits: a function/callback return
/// type, or a generic type-argument list. Returns `false` when no
/// `TSVoidKeyword` covers the offset, so an unresolved position keeps the
/// diagnostic.
fn nivt_void_at_offset_is_valid_position(
    src: &str,
    path: &std::path::Path,
    offset: usize,
) -> bool {
    use crate::oxc_helpers::with_oxc_parse;
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;

    with_oxc_parse(src, path, |semantic| {
        let offset = offset as u32;
        // The smallest `TSVoidKeyword` whose span contains the offset is the one
        // oxlint flagged; `void` keywords never nest, so a single keyword resolves.
        let target = semantic
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind(), AstKind::TSVoidKeyword(_)))
            .find(|node| {
                let span = node.kind().span();
                span.start <= offset && offset < span.end
            });
        let Some(target) = target else {
            return false;
        };
        let void_start = target.kind().span().start;
        nivt_is_return_type_context(target, semantic, void_start)
            || nivt_is_generic_type_arg(target, semantic)
    })
}

/// True when `void` sits inside the return-type annotation of an enclosing
/// function, arrow function, function type, constructor type, method signature,
/// or call signature. Mirrors the native `ts-no-invalid-void-type` rule: the
/// nearest such ancestor is the boundary, and the `void` must fall within its
/// return-type span. A `void` in a parameter annotation reaches a
/// `FormalParameter`/function boundary outside the return-type span and is not
/// exempted.
fn nivt_is_return_type_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    void_start: u32,
) -> bool {
    use oxc_ast::AstKind;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(f) => {
                return f.return_type.as_ref().is_some_and(|ret| {
                    void_start >= ret.span.start && void_start < ret.span.end
                });
            }
            AstKind::ArrowFunctionExpression(f) => {
                return f.return_type.as_ref().is_some_and(|ret| {
                    void_start >= ret.span.start && void_start < ret.span.end
                });
            }
            AstKind::TSFunctionType(ft) => {
                let ret = ft.return_type.span;
                return void_start >= ret.start && void_start < ret.end;
            }
            AstKind::TSConstructorType(ct) => {
                let ret = ct.return_type.span;
                return void_start >= ret.start && void_start < ret.end;
            }
            AstKind::TSMethodSignature(ms) => {
                return ms.return_type.as_ref().is_some_and(|ret| {
                    void_start >= ret.span.start && void_start < ret.span.end
                });
            }
            AstKind::TSCallSignatureDeclaration(cs) => {
                return cs.return_type.as_ref().is_some_and(|ret| {
                    void_start >= ret.span.start && void_start < ret.span.end
                });
            }
            AstKind::TSTypeAliasDeclaration(_) | AstKind::TSInterfaceDeclaration(_) => {
                return false;
            }
            _ => continue,
        }
    }
    false
}

/// True when `void` is a member of a generic type-argument list
/// (`Promise<void>`, `Promise<Cleanup | void>`), stopping at the first
/// function/class boundary so a `void` parameter of an inner callback is not
/// mistaken for a type argument of an outer generic.
fn nivt_is_generic_type_arg(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
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

// ── no-implied-eval post-filter ────────────────────────────────────────────
//
// tsgolint reports two shapes under `no-implied-eval`:
//
//  1. `new Function("…")` — the Function-constructor variant. Always a true
//     positive; the post-filter never touches it.
//  2. A `setTimeout` / `setInterval` / `setImmediate` first argument — the
//     "Consider passing a function." variant. Only a *string* argument is
//     implied eval (the string is compiled and run like `eval`). A function
//     value is the safe, idiomatic usage.
//
// Without resolvable types tsgolint flags any first argument it cannot prove is
// a function, including an untyped callback parameter (`it('x', (done) => {
// setImmediate(done) })`). That misfires on a plain function reference. This
// filter re-parses the file and drops the timer-argument variant unless the
// first argument is provably string-like, mirroring eslint's `no-implied-eval`,
// which reports only string-typed first arguments.

struct NoImpliedEvalFilter;

impl PostFilter for NoImpliedEvalFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        // The Function-constructor variant is always a true positive.
        if diag.message.contains("Function constructor") {
            return true;
        }
        let Some(src) = source else {
            return true;
        };
        let Some(offset) = nivt_byte_offset(src, diag.line, diag.column) else {
            return true;
        };
        nie_timer_arg_is_string_like(src, &diag.path, offset)
    }
}

/// Re-parse `src` and report whether the timer first argument starting at byte
/// `offset` (where tsgolint anchors the diagnostic) is provably string-like.
/// Returns `true` (keep the diagnostic) when the argument is a string/template
/// literal, a binary expression (kept as a possible string concat — the safe,
/// no-suppression direction), or an identifier bound to a string value, and
/// also when nothing resolves at the offset. Only a directly-written function
/// value (arrow or function expression) or an identifier bound to a function is
/// dropped; a function obtained via member access or a call (`obj.cb`, `mk()`)
/// is not an admitted shape and is kept.
fn nie_timer_arg_is_string_like(src: &str, path: &std::path::Path, offset: usize) -> bool {
    use crate::oxc_helpers::with_oxc_parse;
    use oxc_span::GetSpan;

    with_oxc_parse(src, path, |semantic| {
        let offset = offset as u32;
        // The flagged argument is the smallest expression node whose span starts
        // at the diagnostic offset.
        let arg = semantic
            .nodes()
            .iter()
            .filter(|node| nie_is_expression(node.kind()))
            .filter(|node| node.kind().span().start == offset)
            .min_by_key(|node| node.kind().span().end);
        let Some(arg) = arg else {
            return true; // unresolved → keep tsgolint's verdict
        };
        nie_kind_is_string_like(arg.kind(), semantic)
    })
}

fn nie_is_expression(kind: oxc_ast::AstKind) -> bool {
    use oxc_ast::AstKind;
    matches!(
        kind,
        AstKind::StringLiteral(_)
            | AstKind::TemplateLiteral(_)
            | AstKind::BinaryExpression(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Function(_)
            | AstKind::IdentifierReference(_)
    )
}

/// Classify the flagged argument. String/template literals and `+` expressions
/// are string-like; arrow/function expressions are function values; an
/// identifier is resolved to its binding.
fn nie_kind_is_string_like(
    kind: oxc_ast::AstKind,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    match kind {
        AstKind::StringLiteral(_) | AstKind::TemplateLiteral(_) | AstKind::BinaryExpression(_) => {
            true
        }
        AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => false,
        AstKind::IdentifierReference(ident) => nie_binding_is_string(ident, semantic),
        _ => true,
    }
}

/// Resolve `ident` to its declaration and report whether it is provably a
/// string value. A `const`/`let` with a string/template-literal initializer or
/// an explicit string-ish type annotation is string-like (keep). A function
/// declaration, a function-valued initializer, or a parameter that is not
/// annotated as a string is a function reference (drop). Unresolved bindings
/// keep the diagnostic.
fn nie_binding_is_string(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::Expression;

    let Some(ref_id) = ident.reference_id.get() else {
        return true;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return true;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            AstKind::Function(_) => return false, // function declaration
            AstKind::VariableDeclarator(decl) => {
                if let Some(ann) = decl.type_annotation.as_ref() {
                    return nie_type_is_string(&ann.type_annotation);
                }
                return match &decl.init {
                    Some(Expression::StringLiteral(_) | Expression::TemplateLiteral(_)) => true,
                    Some(
                        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_),
                    ) => false,
                    // No annotation and a non-literal initializer: cannot prove
                    // it is a string, so treat it as a function reference.
                    _ => false,
                };
            }
            AstKind::FormalParameter(param) => {
                return param
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| nie_type_is_string(&ann.type_annotation));
            }
            _ => continue,
        }
    }
    true
}

/// True when a type annotation is `string` or a string-literal type.
fn nie_type_is_string(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::{TSLiteral, TSType};
    match ty {
        TSType::TSStringKeyword(_) => true,
        TSType::TSLiteralType(lit) => matches!(lit.literal, TSLiteral::StringLiteral(_)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{Diagnostic, Severity};
    use std::borrow::Cow;
    use std::path::Path;
    use std::sync::Arc;

    fn diag(path: &str) -> Diagnostic {
        Diagnostic {
            path: Arc::from(Path::new(path)),
            line: 1,
            column: 1,
            rule_id: Cow::Borrowed("await-thenable"),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        }
    }

    // Regression for #449: await renderWithProviders() in RTL test file must not fire.
    #[test]
    fn drops_await_thenable_in_test_file() {
        let f = AwaitThenableFilter;
        let d = diag("src/features/product/product-row-actions.test.tsx");
        assert!(!f.keep(&d, None), "await-thenable in .test.tsx must be suppressed");
    }

    #[test]
    fn drops_await_thenable_in_spec_file() {
        let f = AwaitThenableFilter;
        assert!(!f.keep(&diag("src/utils/format.spec.ts"), None));
    }

    #[test]
    fn drops_await_thenable_in_tests_dir() {
        let f = AwaitThenableFilter;
        assert!(!f.keep(&diag("src/__tests__/helpers.ts"), None));
    }

    #[test]
    fn keeps_await_thenable_in_production_file() {
        let f = AwaitThenableFilter;
        assert!(f.keep(&diag("src/features/product/product-row-actions.tsx"), None));
    }

    // ── unbound-method ──────────────────────────────────────────────────────

    // Regression for #4369: `expect(vi.mocked(log).error).not.toHaveBeenCalled()`
    // in a test file inspects a mock's call records, not a detached method —
    // unbound-method must not fire there.
    #[test]
    fn drops_unbound_method_in_test_file() {
        let f = UnboundMethodFilter;
        assert!(!f.keep(&diag("src/api/csv/csv-stream.test.ts"), None));
        assert!(!f.keep(&diag("src/api/csv/csv-stream.spec.ts"), None));
    }

    #[test]
    fn keeps_unbound_method_in_non_test_file() {
        let f = UnboundMethodFilter;
        assert!(f.keep(&diag("src/api/csv/csv-stream.ts"), None));
    }

    // ── ban-types ───────────────────────────────────────────────────────────

    fn ban_types_diag(path: &std::path::Path, line: usize) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed("ban-types"),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        }
    }

    fn write_temp(name: &str, src: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("comply-tsgolint-post-filter-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, src).unwrap();
        path
    }

    fn line_of(src: &str, needle: &str) -> usize {
        src.lines()
            .enumerate()
            .find(|(_, l)| l.contains(needle))
            .map(|(i, _)| i + 1)
            .expect("needle not in source")
    }

    fn source_for(path: &std::path::Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    // Regression for #748: `string & {}` intersection must be suppressed.
    #[test]
    fn drops_ban_types_on_intersection() {
        let src = "type Spec = Breakpoint | (string & {});\n";
        let path = write_temp("ban_types_intersection.ts", src);
        let line = line_of(src, "string & {}");
        let src_content = source_for(&path);
        let f = BanTypesFilter;
        assert!(!f.keep(&ban_types_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn keeps_ban_types_standalone_empty_object() {
        let src = "const x: {} = foo;\n";
        let path = write_temp("ban_types_standalone.ts", src);
        let src_content = source_for(&path);
        let f = BanTypesFilter;
        assert!(f.keep(&ban_types_diag(&path, 1), Some(&src_content)));
    }

    // ── no-empty-interface ──────────────────────────────────────────────────

    fn empty_interface_diag(path: &std::path::Path, line: usize) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed("no-empty-interface"),
            message: "an interface declaring no members is equivalent to its supertype".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    // Regression for #5293: direct self-recursion `interface Foo extends X<Foo> {}`
    // cannot be a type alias (circular) — must be suppressed.
    #[test]
    fn drops_empty_interface_direct_self_recursion() {
        let src = "interface Foo extends X<Foo> {}\n";
        let path = write_temp("nei_direct.ts", src);
        let line = line_of(src, "interface Foo");
        let src_content = source_for(&path);
        let f = NoEmptyInterfaceFilter;
        assert!(!f.keep(&empty_interface_diag(&path, line), Some(&src_content)));
    }

    // Nested-generic self-recursion `interface T extends A<B<T>> {}`.
    #[test]
    fn drops_empty_interface_nested_self_recursion() {
        let src = "interface T extends A<B<T>> {}\n";
        let path = write_temp("nei_nested.ts", src);
        let line = line_of(src, "interface T");
        let src_content = source_for(&path);
        let f = NoEmptyInterfaceFilter;
        assert!(!f.keep(&empty_interface_diag(&path, line), Some(&src_content)));
    }

    // Regression for #5293: gcanti/io-ts mutual recursion through a union alias.
    #[test]
    fn drops_empty_interface_mutual_recursion_via_union_alias() {
        let src = r#"interface GenerableRecord extends t.DictionaryType<Generable, Generable> {}
interface GenerableUnion extends t.UnionType<Array<Generable>> {}

type Generable =
  | t.StringC
  | GenerableRecord
  | GenerableUnion
"#;
        let path = write_temp("nei_io_ts.ts", src);
        let src_content = source_for(&path);
        let f = NoEmptyInterfaceFilter;
        let l1 = line_of(src, "interface GenerableRecord");
        let l2 = line_of(src, "interface GenerableUnion");
        assert!(!f.keep(&empty_interface_diag(&path, l1), Some(&src_content)));
        assert!(!f.keep(&empty_interface_diag(&path, l2), Some(&src_content)));
    }

    // A plain single-extends empty interface with no self/mutual recursion is
    // genuinely rewritable to a type alias — must still flag.
    #[test]
    fn keeps_empty_interface_non_recursive_single_extends() {
        let src = "interface Foo extends Bar {}\n";
        let path = write_temp("nei_plain.ts", src);
        let line = line_of(src, "interface Foo");
        let src_content = source_for(&path);
        let f = NoEmptyInterfaceFilter;
        assert!(f.keep(&empty_interface_diag(&path, line), Some(&src_content)));
    }

    // A parameterized extends whose args do NOT cycle back to the interface is
    // still rewritable — must still flag.
    #[test]
    fn keeps_empty_interface_generic_extends_no_cycle() {
        let src = "interface Foo extends X<Bar> {}\ntype Baz = Bar | Qux;\n";
        let path = write_temp("nei_no_cycle.ts", src);
        let line = line_of(src, "interface Foo");
        let src_content = source_for(&path);
        let f = NoEmptyInterfaceFilter;
        assert!(f.keep(&empty_interface_diag(&path, line), Some(&src_content)));
    }

    // A bodiless empty interface with no extends at all is still redundant.
    #[test]
    fn keeps_empty_interface_no_extends() {
        let src = "interface Foo {}\n";
        let path = write_temp("nei_no_extends.ts", src);
        let line = line_of(src, "interface Foo");
        let src_content = source_for(&path);
        let f = NoEmptyInterfaceFilter;
        assert!(f.keep(&empty_interface_diag(&path, line), Some(&src_content)));
    }

    // ── no-unsafe-assignment ────────────────────────────────────────────────

    fn unsafe_assign_diag(path: &std::path::Path, line: usize) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: 3,
            rule_id: Cow::Borrowed("no-unsafe-assignment"),
            message: "Unsafe assignment of an error typed value.".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    // Regression for #380: visualizer() as PluginOption
    #[test]
    fn drops_visualizer_as_plugin_option() {
        let src = r#"import visualizer from "rollup-plugin-visualizer";
import type { PluginOption } from "vite";
const plugins: PluginOption[] = [
  visualizer({ open: true }) as PluginOption,
];
"#;
        let path = write_temp("drops_visualizer_plugin_option.ts", src);
        let line = line_of(src, "as PluginOption");
        let src_content = source_for(&path);
        let f = NoUnsafeAssignmentFilter;
        assert!(!f.keep(&unsafe_assign_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn keeps_unsafe_assignment_without_plugin_option() {
        let src = r#"import { something } from "vite";
const x: string = unknownAny as string;
"#;
        let path = write_temp("no_plugin_option.ts", src);
        let line = line_of(src, "as string");
        let src_content = source_for(&path);
        let f = NoUnsafeAssignmentFilter;
        assert!(f.keep(&unsafe_assign_diag(&path, line), Some(&src_content)));
    }

    // Regression for #5770: a Vitest asymmetric matcher nested in
    // `objectContaining` returns `any` from its own typings — drop the FP.
    #[test]
    fn drops_vitest_asymmetric_matcher() {
        let src = r#"it("reports", () => {
  expect(errorReporter.captureException).toHaveBeenCalledWith(
    expect.any(Error),
    expect.objectContaining({ requestId: expect.stringMatching(uuidv7Regex) }),
  );
});
"#;
        let path = write_temp("error-handler.test.ts", src);
        let line = line_of(src, "expect.objectContaining(");
        let src_content = source_for(&path);
        let f = NoUnsafeAssignmentFilter;
        assert!(!f.keep(&unsafe_assign_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn keeps_unsafe_assignment_on_non_matcher_line_in_test() {
        // Negative-space guard: a genuine unsafe assignment in a test file whose
        // line carries no asymmetric matcher must still be reported.
        let src = r#"it("does work", () => {
  const value: string = readAny() as string;
  expect(value).toBe("x");
});
"#;
        let path = write_temp("genuine.test.ts", src);
        let line = line_of(src, "readAny()");
        let src_content = source_for(&path);
        let f = NoUnsafeAssignmentFilter;
        assert!(f.keep(&unsafe_assign_diag(&path, line), Some(&src_content)));
    }

    // ── only-throw-error ────────────────────────────────────────────────────

    fn throw_diag(path: &std::path::Path, line: usize) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed("only-throw-error"),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        }
    }

    // Regression: `throw notFound(...)` in TanStack Router file must be suppressed.
    #[test]
    fn drops_tanstack_not_found_throw() {
        let src = r#"import { notFound } from "@tanstack/react-router";
export function loader() {
  throw notFound({ routeId: "/x" });
}
"#;
        let path = write_temp("tanstack_throw.ts", src);
        let line = line_of(src, "throw notFound(");
        let src_content = source_for(&path);
        let f = OnlyThrowErrorFilter;
        assert!(!f.keep(&throw_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn keeps_throw_without_tanstack_import() {
        let src = r#"function notFound(opts) { return { type: "not-found", ...opts }; }
export function loader() {
  throw notFound({ routeId: "/x" });
}
"#;
        let path = write_temp("throw_no_tanstack.ts", src);
        let line = line_of(src, "throw notFound(");
        let src_content = source_for(&path);
        let f = OnlyThrowErrorFilter;
        assert!(f.keep(&throw_diag(&path, line), Some(&src_content)));
    }

    // ── promise-function-async ──────────────────────────────────────────────

    fn pfa_diag(path: &std::path::Path, line: usize, col: usize) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: col,
            rule_id: Cow::Borrowed("promise-function-async"),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        }
    }

    fn line_col_of(src: &str, needle: &str) -> (usize, usize) {
        for (i, l) in src.lines().enumerate() {
            if let Some(c) = l.find(needle) {
                return (i + 1, c + 1);
            }
        }
        panic!("needle not in source: {needle}");
    }

    // Regression for #273: Effect.Effect return type must be dropped.
    #[test]
    fn drops_effect_return_type() {
        let src = "function getUser(id: string): Effect.Effect<User, Err> {\n  return program;\n}\n";
        let path = write_temp("pfa_effect_fn.ts", src);
        let (line, col) = line_col_of(src, "function");
        let src_content = source_for(&path);
        let f = PromiseFunctionAsyncFilter;
        assert!(!f.keep(&pfa_diag(&path, line, col), Some(&src_content)));
    }

    #[test]
    fn keeps_promise_return_type() {
        let src = "function f(): Promise<void> {\n  return p;\n}\n";
        let path = write_temp("pfa_promise_fn.ts", src);
        let (line, col) = line_col_of(src, "function");
        let src_content = source_for(&path);
        let f = PromiseFunctionAsyncFilter;
        assert!(f.keep(&pfa_diag(&path, line, col), Some(&src_content)));
    }

    // Regression for #342: concise callback arrow is a pass-through.
    #[test]
    fn drops_concise_callback_arrow_passthrough() {
        let src = "apiCall((api) => api.get())\n";
        let path = write_temp("pfa_callback.ts", src);
        let (line, col) = line_col_of(src, "(api)");
        let src_content = source_for(&path);
        let f = PromiseFunctionAsyncFilter;
        assert!(!f.keep(&pfa_diag(&path, line, col), Some(&src_content)));
    }

    #[test]
    fn drops_single_return_block_callback_arrow() {
        let src = "apiCall((api) => { return api.get(); })\n";
        let path = write_temp("pfa_block_callback.ts", src);
        let (line, col) = line_col_of(src, "(api)");
        let src_content = source_for(&path);
        let f = PromiseFunctionAsyncFilter;
        assert!(!f.keep(&pfa_diag(&path, line, col), Some(&src_content)));
    }

    // ── no-unnecessary-condition ─────────────────────────────────────────

    fn nuc_diag(path: &std::path::Path, line: usize) -> Diagnostic {
        Diagnostic {
            path: Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed("no-unnecessary-condition"),
            message: String::new(),
            severity: crate::diagnostic::Severity::Error,
            span: None,
        }
    }

    #[test]
    fn nuc_drops_elysia_map_response_nullish() {
        let src = "import { Elysia } from \"elysia\";\nnew Elysia()\n  .mapResponse(({ requestId, set }) => {\n    set.headers[\"x-request-id\"] = requestId ?? \"unknown\";\n  });\n";
        let path = write_temp("nuc_elysia_map.ts", src);
        let src_content = source_for(&path);
        let line = line_of(src, "requestId ?? \"unknown\"");
        let f = NoUnnecessaryConditionFilter;
        assert!(!f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nuc_drops_exhaustiveness_gate() {
        let src = "function foo(props: { action: \"create\" }) {\n  if (props.action === \"create\") {\n    return 1;\n  }\n  const _exhaustive: never = props.action;\n}\n";
        let path = write_temp("nuc_exhaustiveness.ts", src);
        let src_content = source_for(&path);
        let line = line_of(src, "props.action === \"create\"");
        let f = NoUnnecessaryConditionFilter;
        assert!(!f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nuc_keeps_nullish_outside_elysia_hook() {
        let src = "import { Elysia } from \"elysia\";\nconst x: string = \"set\";\nconst y = x ?? \"fallback\";\n";
        let path = write_temp("nuc_outside_hook.ts", src);
        let src_content = source_for(&path);
        let line = line_of(src, "?? \"fallback\"");
        let f = NoUnnecessaryConditionFilter;
        assert!(f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nuc_keeps_always_true_without_gate() {
        let src = "function foo(props: { action: \"create\" }) {\n  if (props.action === \"create\") {\n    return 1;\n  }\n  return 0;\n}\n";
        let path = write_temp("nuc_no_gate.ts", src);
        let src_content = source_for(&path);
        let line = line_of(src, "props.action === \"create\"");
        let f = NoUnnecessaryConditionFilter;
        assert!(f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nuc_drops_optional_chain_after_jest_not_to_be_null() {
        let src = "const hint = cond ? null : document.querySelector(sel);\nexpect(hint).not.toBeNull();\nexpect(hint?.textContent?.trim().length ?? 0).toBeGreaterThan(0);\n";
        let path = write_temp("nuc_jest_not_null.test.tsx", src);
        let src_content = source_for(&path);
        let line = line_of(src, "hint?.textContent");
        let f = NoUnnecessaryConditionFilter;
        assert!(!f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nuc_drops_optional_chain_after_jest_to_be_defined() {
        let src = "const hint = maybe();\nexpect(hint).toBeDefined();\nconst len = hint?.value;\n";
        let path = write_temp("nuc_jest_defined.test.tsx", src);
        let src_content = source_for(&path);
        let line = line_of(src, "hint?.value");
        let f = NoUnnecessaryConditionFilter;
        assert!(!f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nuc_drops_optional_chain_after_jest_to_be_truthy() {
        let src = "const hint = maybe();\nexpect(hint).toBeTruthy();\nconst len = hint?.value;\n";
        let path = write_temp("nuc_jest_truthy.test.tsx", src);
        let src_content = source_for(&path);
        let line = line_of(src, "hint?.value");
        let f = NoUnnecessaryConditionFilter;
        assert!(!f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nuc_drops_chain_after_jest_assertion_with_whitespace() {
        let src = "const hint = maybe();\nexpect( hint ).not.toBeNull();\nconst len = hint?.value;\n";
        let path = write_temp("nuc_jest_ws.test.tsx", src);
        let src_content = source_for(&path);
        let line = line_of(src, "hint?.value");
        let f = NoUnnecessaryConditionFilter;
        assert!(!f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nuc_drops_optional_chain_after_jest_not_to_be_falsy() {
        let src = "const hint = maybe();\nexpect(hint).not.toBeFalsy();\nconst len = hint?.value;\n";
        let path = write_temp("nuc_jest_not_falsy.test.tsx", src);
        let src_content = source_for(&path);
        let line = line_of(src, "hint?.value");
        let f = NoUnnecessaryConditionFilter;
        assert!(!f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nuc_keeps_optional_chain_without_preceding_assertion() {
        let src = "const hint = maybe();\nconst len = hint?.value;\n";
        let path = write_temp("nuc_jest_no_assertion.test.tsx", src);
        let src_content = source_for(&path);
        let line = line_of(src, "hint?.value");
        let f = NoUnnecessaryConditionFilter;
        assert!(f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nuc_keeps_chain_when_assertion_is_on_different_var() {
        let src = "const hint = maybe();\nexpect(other).not.toBeNull();\nconst len = hint?.value;\n";
        let path = write_temp("nuc_jest_other_var.test.tsx", src);
        let src_content = source_for(&path);
        let line = line_of(src, "hint?.value");
        let f = NoUnnecessaryConditionFilter;
        assert!(f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nuc_keeps_chain_when_assertion_var_is_a_prefix() {
        let src = "const hint = maybe();\nexpect(hintFoo).not.toBeNull();\nconst len = hint?.value;\n";
        let path = write_temp("nuc_jest_prefix_var.test.tsx", src);
        let src_content = source_for(&path);
        let line = line_of(src, "hint?.value");
        let f = NoUnnecessaryConditionFilter;
        assert!(f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nuc_keeps_flagged_line_without_chain_or_coalesce() {
        let src = "const hint = maybe();\nexpect(hint).not.toBeNull();\nconst len = hint.value;\n";
        let path = write_temp("nuc_jest_no_chain.test.tsx", src);
        let src_content = source_for(&path);
        let line = line_of(src, "hint.value");
        let f = NoUnnecessaryConditionFilter;
        assert!(f.keep(&nuc_diag(&path, line), Some(&src_content)));
    }

    // ── no-misused-spread ────────────────────────────────────────────────

    fn nms_diag(path: &std::path::Path, line: usize) -> Diagnostic {
        Diagnostic {
            path: Arc::from(path),
            line,
            column: 5,
            rule_id: Cow::Borrowed("no-misused-spread"),
            message: "Using the spread operator on a class instance loses methods.".into(),
            severity: crate::diagnostic::Severity::Error,
            span: None,
        }
    }

    #[test]
    fn nms_drops_spread_into_error_constructor() {
        let src = "throw new APIError(\n  'FORBIDDEN',\n  { ...apiError },\n);\n";
        let path = write_temp("nms_error_interop.ts", src);
        let src_content = source_for(&path);
        let line = line_of(src, "{ ...apiError }");
        let f = NoMisusedSpreadFilter;
        assert!(!f.keep(&nms_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nms_keeps_spread_not_into_error_constructor() {
        let src = "const merged = { ...someClassInstance };\n";
        let path = write_temp("nms_plain_spread.ts", src);
        let src_content = source_for(&path);
        let line = line_of(src, "...someClassInstance");
        let f = NoMisusedSpreadFilter;
        assert!(f.keep(&nms_diag(&path, line), Some(&src_content)));
    }

    // ── no-redundant-type-constituents ──────────────────────────────────

    fn nrtc_diag(path: &std::path::Path, line: usize) -> Diagnostic {
        Diagnostic {
            path: Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed("no-redundant-type-constituents"),
            message: String::new(),
            severity: crate::diagnostic::Severity::Error,
            span: None,
        }
    }

    #[test]
    fn nrtc_drops_single_line_keyof_string_satisfies() {
        let src = "type User = { id: string; email: string };\nconst s = ['email'] as const satisfies readonly (keyof User & string)[];\n";
        let path = write_temp("nrtc_single_keyof.ts", src);
        let src_content = source_for(&path);
        let line = line_of(src, "satisfies");
        let f = NoRedundantTypeConstituentsFilter;
        assert!(!f.keep(&nrtc_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn nrtc_keeps_string_union_literal() {
        let src = "type T = string | 'foo';\n";
        let path = write_temp("nrtc_union_literal.ts", src);
        let src_content = source_for(&path);
        let f = NoRedundantTypeConstituentsFilter;
        assert!(f.keep(&nrtc_diag(&path, 1), Some(&src_content)));
    }

    #[test]
    fn nrtc_keeps_string_intersection_string() {
        let src = "type T = string & string;\n";
        let path = write_temp("nrtc_string_and_string.ts", src);
        let src_content = source_for(&path);
        let f = NoRedundantTypeConstituentsFilter;
        assert!(f.keep(&nrtc_diag(&path, 1), Some(&src_content)));
    }

    fn nrtc_diag_msg(path: &std::path::Path, line: usize, message: &str) -> Diagnostic {
        Diagnostic {
            path: Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed("no-redundant-type-constituents"),
            message: message.to_string(),
            severity: crate::diagnostic::Severity::Error,
            span: None,
        }
    }

    #[test]
    fn nrtc_drops_unresolved_imported_geojson_union() {
        // Issue #5282: `Polygon | MultiPolygon` from the `geojson` package.
        // The backend cannot resolve `geojson`, so `Polygon` degrades to the
        // `error` type — but it is an imported reference, not a genuine `any`.
        let src = "import { Feature, MultiPolygon, Polygon } from 'geojson';\n\
                   type T = Polygon | MultiPolygon;\n";
        let path = write_temp("nrtc_geojson_union.ts", src);
        let src_content = source_for(&path);
        let f = NoRedundantTypeConstituentsFilter;
        let msg = "'Polygon' is an 'error' type that acts as 'any' and overrides all other types in this union type.";
        assert!(!f.keep(&nrtc_diag_msg(&path, 2, msg), Some(&src_content)));
    }

    #[test]
    fn nrtc_drops_unresolved_imported_generic_instantiation() {
        // `Feature<any> | Geometry` — the flagged name is the generic
        // instantiation `Feature<any>`; the base identifier `Feature` is what
        // the import binds.
        let src = "import { Feature, Geometry } from 'geojson';\n\
                   type T = Feature<any> | Geometry;\n";
        let path = write_temp("nrtc_geojson_generic.ts", src);
        let src_content = source_for(&path);
        let f = NoRedundantTypeConstituentsFilter;
        let msg = "'Feature<any>' is an 'error' type that acts as 'any' and overrides all other types in this union type.";
        assert!(!f.keep(&nrtc_diag_msg(&path, 2, msg), Some(&src_content)));
    }

    #[test]
    fn nrtc_drops_unresolved_multiline_import() {
        // Multi-line named import — the binding lives on a continuation line,
        // so the import statement must be scanned as a block.
        let src = "import {\n  Polygon,\n  MultiPolygon,\n} from 'geojson';\n\
                   type T = Polygon | MultiPolygon;\n";
        let path = write_temp("nrtc_geojson_multiline.ts", src);
        let src_content = source_for(&path);
        let f = NoRedundantTypeConstituentsFilter;
        let msg = "'Polygon' is an 'error' type that acts as 'any' and overrides all other types in this union type.";
        assert!(!f.keep(&nrtc_diag_msg(&path, 5, msg), Some(&src_content)));
    }

    #[test]
    fn nrtc_keeps_type_name_only_in_module_specifier() {
        // The type name appears only inside the package path, never as a
        // binding — must NOT be treated as imported, so a genuine local error
        // type keeps flagging.
        let src = "import { foo } from './Polygon';\n\
                   type T = Polygon | string;\n";
        let path = write_temp("nrtc_specifier_only.ts", src);
        let src_content = source_for(&path);
        let f = NoRedundantTypeConstituentsFilter;
        let msg = "'Polygon' is an 'error' type that acts as 'any' and overrides all other types in this union type.";
        assert!(f.keep(&nrtc_diag_msg(&path, 2, msg), Some(&src_content)));
    }

    #[test]
    fn nrtc_keeps_genuine_any_union() {
        // `string | any` — genuine `any` uses the distinct "overrides all other
        // types" message (no "error type"), so it stays flagged.
        let src = "type T = string | any;\n";
        let path = write_temp("nrtc_genuine_any.ts", src);
        let src_content = source_for(&path);
        let f = NoRedundantTypeConstituentsFilter;
        let msg = "'any' overrides all other types in this union type.";
        assert!(f.keep(&nrtc_diag_msg(&path, 1, msg), Some(&src_content)));
    }

    #[test]
    fn nrtc_keeps_error_type_not_imported() {
        // A locally-declared type that is a genuine error (not imported) keeps
        // firing — the guard only excuses unresolved imports.
        let src = "type T = Broken | string;\n";
        let path = write_temp("nrtc_local_error.ts", src);
        let src_content = source_for(&path);
        let f = NoRedundantTypeConstituentsFilter;
        let msg = "'Broken' is an 'error' type that acts as 'any' and overrides all other types in this union type.";
        assert!(f.keep(&nrtc_diag_msg(&path, 1, msg), Some(&src_content)));
    }

    // ── no-unnecessary-type-parameters (equal probe) ────────────────────

    fn ep_diag(path: &std::path::Path, line: usize) -> Diagnostic {
        Diagnostic {
            path: Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed("no-unnecessary-type-parameters"),
            message: String::new(),
            severity: crate::diagnostic::Severity::Error,
            span: None,
        }
    }

    #[test]
    fn ep_drops_equal_probe_identity() {
        let src = "type IdentityProbe<X> = <T>() => T extends X ? 1 : 2;\n";
        let path = write_temp("ep_identity_probe.ts", src);
        let src_content = source_for(&path);
        let f = EqualProbeFilter;
        assert!(!f.keep(&ep_diag(&path, 1), Some(&src_content)));
    }

    #[test]
    fn ep_drops_equal_probe_with_boolean_units() {
        let src = "type P<X> = <T>() => T extends X ? true : false;\n";
        let path = write_temp("ep_bool_units.ts", src);
        let src_content = source_for(&path);
        let f = EqualProbeFilter;
        assert!(!f.keep(&ep_diag(&path, 1), Some(&src_content)));
    }

    #[test]
    fn ep_drops_probe_with_constrained_type_param() {
        let src = "type P<X> = <T extends unknown>() => T extends X ? 1 : 2;\n";
        let path = write_temp("ep_constrained_probe.ts", src);
        let src_content = source_for(&path);
        let f = EqualProbeFilter;
        assert!(!f.keep(&ep_diag(&path, 1), Some(&src_content)));
    }

    #[test]
    fn ep_drops_multiline_signature_fp() {
        let src = concat!(
            "export function useListSearchSync<TSearch extends ListRouteSearch>(\n",
            "  routeApi: ListRouteApi<TSearch>,\n",
            "  { filterKeys }: UseListSearchSyncOptions<TSearch>,\n",
            "): void {}\n",
        );
        let path = write_temp("ep_multiline_sig.ts", src);
        let line = line_of(src, "useListSearchSync");
        let src_content = source_for(&path);
        let f = EqualProbeFilter;
        assert!(!f.keep(&ep_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn ep_keeps_real_unused_type_parameter() {
        let src = "function f<T>(x: number): string { return ''; }\n";
        let path = write_temp("ep_real_unused.ts", src);
        let src_content = source_for(&path);
        let f = EqualProbeFilter;
        assert!(f.keep(&ep_diag(&path, 1), Some(&src_content)));
    }

    #[test]
    fn ep_drops_overload_union_param_fp() {
        let src = concat!(
            "public on<T = JsonRpcResult>(\n",
            "    type: 'message',\n",
            "    listener:\n",
            "        | Web3Eip1193ProviderEventCallback<ProviderMessage>\n",
            "        | Web3ProviderMessageEventCallback<T>,\n",
            "): void;\n",
        );
        let path = write_temp("ep_overload_union.ts", src);
        let line = line_of(src, "public on<T");
        let src_content = source_for(&path);
        let f = EqualProbeFilter;
        assert!(!f.keep(&ep_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn ep_keeps_unused_param_when_only_body_uses_it() {
        let src = concat!(
            "function f<T>(\n",
            "  x: number,\n",
            "): string {\n",
            "  return useT<T>();\n",
            "}\n",
        );
        let path = write_temp("ep_body_only_use.ts", src);
        let line = line_of(src, "function f<T>");
        let src_content = source_for(&path);
        let f = EqualProbeFilter;
        assert!(f.keep(&ep_diag(&path, line), Some(&src_content)));
    }

    #[test]
    fn ep_does_not_drop_nested_conditional_without_function_generic() {
        let src = "type A<T> = T extends (U extends V ? 1 : 2) ? 3 : 4;\n";
        let path = write_temp("ep_nested_conditional.ts", src);
        let line = line_of(src, "T extends");
        let src_content = source_for(&path);
        let f = EqualProbeFilter;
        assert!(f.keep(&ep_diag(&path, line), Some(&src_content)));
    }

    // ── strict-void-return ───────────────────────────────────────────────

    fn svr_diag(path: &std::path::Path, line: usize, column: usize) -> Diagnostic {
        Diagnostic {
            path: Arc::from(path),
            line,
            column,
            rule_id: Cow::Borrowed("strict-void-return"),
            message: String::new(),
            severity: crate::diagnostic::Severity::Error,
            span: None,
        }
    }

    #[test]
    fn svr_drops_inline_vi_fn_jsx_prop() {
        let src = "import { vi } from 'vitest';\nfunction Test() { return <Dialog onClose={vi.fn()} />; }\n";
        let path = write_temp("svr_inline_vi_fn.tsx", src);
        let (line, _) = line_col_of(src, "vi.fn()");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(!f.keep(&svr_diag(&path, line, 1), Some(&src_content)));
    }

    #[test]
    fn svr_drops_aliased_vi_fn_via_const_declaration() {
        let src = "import { vi } from 'vitest';\nconst onClose = vi.fn();\nfunction Test() { return <Dialog onClose={onClose} />; }\n";
        let path = write_temp("svr_aliased_vi_fn.tsx", src);
        let (line, col) = line_col_of(src, "onClose={onClose}");
        let value_col = col + "onClose={".len();
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(!f.keep(&svr_diag(&path, line, value_col), Some(&src_content)));
    }

    #[test]
    fn svr_drops_render_hook_callback() {
        let src = "import { renderHook } from '@testing-library/react';\ntest('x', () => {\n  renderHook(() => useUser());\n});\n";
        let path = write_temp("svr_render_hook.tsx", src);
        let (line, _) = line_col_of(src, "renderHook(() =>");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(!f.keep(&svr_diag(&path, line, 15), Some(&src_content)));
    }

    #[test]
    fn svr_keeps_genuine_void_misuse() {
        let src = "function setup(cb: () => void) { cb(); }\nsetup(() => 42);\n";
        let path = write_temp("svr_genuine_misuse.ts", src);
        let (line, _) = line_col_of(src, "() => 42");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(f.keep(&svr_diag(&path, line, 7), Some(&src_content)));
    }

    #[test]
    fn svr_keeps_lookalike_my_render_hook() {
        let src = "function myRenderHook(cb: () => void) { cb(); }\nmyRenderHook(() => 42);\n";
        let path = write_temp("svr_my_render_hook.ts", src);
        let (line, _) = line_col_of(src, "myRenderHook(() => 42)");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(f.keep(&svr_diag(&path, line, 14), Some(&src_content)));
    }

    #[test]
    fn svr_render_hook_no_bleed_into_adjacent_call() {
        let src = "\
import { renderHook } from '@testing-library/react';
test('a', () => {
  renderHook(() => useUser());

  // unrelated
  otherCall(() => getValue());
});
";
        let path = write_temp("svr_render_hook_no_bleed.tsx", src);
        let (rh_line, _) = line_col_of(src, "renderHook(() =>");
        let (oc_line, _) = line_col_of(src, "otherCall(() =>");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(!f.keep(&svr_diag(&path, rh_line, 15), Some(&src_content)));
        assert!(f.keep(&svr_diag(&path, oc_line, 13), Some(&src_content)));
    }

    #[test]
    fn svr_ident_prefix_does_not_match_alias() {
        let src = "import { vi } from 'vitest';\nconst mockable = vi.fn();\nfunction T() { return <D onClose={mock} />; }\n";
        let path = write_temp("svr_prefix_no_match.tsx", src);
        let (line, col) = line_col_of(src, "{mock}");
        let value_col = col + 1;
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(f.keep(&svr_diag(&path, line, value_col), Some(&src_content)));
    }

    // Regression for #4397: `new Promise(r => setTimeout(r, ms))` is the
    // canonical sleep idiom — the executor's `void` return type discards the
    // timer handle. Must not fire.
    #[test]
    fn svr_drops_promise_executor_set_timeout_with_delay() {
        let src = "await new Promise((resolve) => setTimeout(resolve, 10))\n";
        let path = write_temp("svr_promise_set_timeout_delay.ts", src);
        let (line, _) = line_col_of(src, "new Promise(");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(!f.keep(&svr_diag(&path, line, 1), Some(&src_content)));
    }

    #[test]
    fn svr_drops_promise_executor_set_timeout_no_delay() {
        let src = "await new Promise((resolve) => setTimeout(resolve))\n";
        let path = write_temp("svr_promise_set_timeout_no_delay.ts", src);
        let (line, _) = line_col_of(src, "new Promise(");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(!f.keep(&svr_diag(&path, line, 1), Some(&src_content)));
    }

    #[test]
    fn svr_drops_promise_executor_set_interval() {
        let src = "const sleep = (ms: number) => new Promise(resolve => setInterval(resolve, ms))\n";
        let path = write_temp("svr_promise_set_interval.ts", src);
        let (line, _) = line_col_of(src, "new Promise(");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(!f.keep(&svr_diag(&path, line, 1), Some(&src_content)));
    }

    #[test]
    fn svr_keeps_set_timeout_outside_promise() {
        let src = "setTimeout(() => doReturn(), 10)\n";
        let path = write_temp("svr_set_timeout_no_promise.ts", src);
        let (line, _) = line_col_of(src, "setTimeout(");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(f.keep(&svr_diag(&path, line, 1), Some(&src_content)));
    }

    #[test]
    fn svr_keeps_promise_executor_returning_non_timer_value() {
        let src = "new Promise((resolve) => resolve(computeValue()))\n";
        let path = write_temp("svr_promise_non_timer.ts", src);
        let (line, _) = line_col_of(src, "new Promise(");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(f.keep(&svr_diag(&path, line, 1), Some(&src_content)));
    }

    #[test]
    fn svr_keeps_promise_executor_timer_when_source_missing() {
        let f = StrictVoidReturnFilter;
        let d = svr_diag(Path::new("src/foo.ts"), 1, 1);
        assert!(f.keep(&d, None));
    }

    // Regression for #4821: a concise arrow whose body is a collection mutator
    // (`(data) => blocks.push(data)`) in a void-callback slot is a discarded
    // side effect, not a leaked value. Must not fire. The diagnostic column
    // points at the arrow-function start.
    #[test]
    fn svr_drops_concise_arrow_push() {
        let src = "transport.subscribe({\n  onData: (data) => blocks.push(data),\n})\n";
        let path = write_temp("svr_concise_push.ts", src);
        let (line, col) = line_col_of(src, "(data) => blocks.push(data)");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(!f.keep(&svr_diag(&path, line, col), Some(&src_content)));
    }

    #[test]
    fn svr_drops_concise_arrow_error_push() {
        let src = "transport.subscribe({\n  onError: (err) => errors.push(err),\n})\n";
        let path = write_temp("svr_concise_error_push.ts", src);
        let (line, col) = line_col_of(src, "(err) => errors.push(err)");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(!f.keep(&svr_diag(&path, line, col), Some(&src_content)));
    }

    #[test]
    fn svr_drops_concise_arrow_set_add() {
        let src = "items.forEach((x) => set.add(x))\n";
        let path = write_temp("svr_concise_set_add.ts", src);
        let (line, col) = line_col_of(src, "(x) => set.add(x)");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(!f.keep(&svr_diag(&path, line, col), Some(&src_content)));
    }

    // Member-chain receiver with optional chaining: `this.items?.push(x)`.
    #[test]
    fn svr_drops_concise_arrow_optional_chain_push() {
        let src = "list.forEach((x) => this.items.push(x))\n";
        let path = write_temp("svr_concise_member_push.ts", src);
        let (line, col) = line_col_of(src, "(x) => this.items.push(x)");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(!f.keep(&svr_diag(&path, line, col), Some(&src_content)));
    }

    // A block body returning a real value is still a genuine misuse.
    #[test]
    fn svr_keeps_block_body_returning_value() {
        let src = "setup((data) => { return computeValue(data); })\n";
        let path = write_temp("svr_block_return.ts", src);
        let (line, col) = line_col_of(src, "(data) =>");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(f.keep(&svr_diag(&path, line, col), Some(&src_content)));
    }

    // A concise arrow returning a real value (non-mutator call) still fires.
    #[test]
    fn svr_keeps_concise_arrow_returning_value() {
        let src = "setup((data) => transform(data))\n";
        let path = write_temp("svr_concise_value.ts", src);
        let (line, col) = line_col_of(src, "(data) =>");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(f.keep(&svr_diag(&path, line, col), Some(&src_content)));
    }

    // A mutator call that is only part of a larger value expression still fires:
    // `a.push(x) + 1` returns the new length, a genuine value leak.
    #[test]
    fn svr_keeps_mutator_call_in_larger_expression() {
        let src = "setup((x) => arr.push(x) + 1)\n";
        let path = write_temp("svr_mutator_plus.ts", src);
        let (line, col) = line_col_of(src, "(x) =>");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(f.keep(&svr_diag(&path, line, col), Some(&src_content)));
    }

    // `push` as a bare identifier (not a method call) must not trigger the exemption.
    #[test]
    fn svr_keeps_bare_push_identifier() {
        let src = "setup((push) => push)\n";
        let path = write_temp("svr_bare_push.ts", src);
        let (line, col) = line_col_of(src, "(push) =>");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(f.keep(&svr_diag(&path, line, col), Some(&src_content)));
    }

    // Multi-callback line: a genuine value-returning arrow before a mutator arrow
    // must still fire (column anchors to the offending arrow, no bleed).
    #[test]
    fn svr_multi_arrow_no_bleed_from_mutator() {
        let src = "obj({ onA: () => compute(), onB: (x) => acc.push(x) })\n";
        let path = write_temp("svr_multi_arrow.ts", src);
        let (line, col_a) = line_col_of(src, "() => compute()");
        let (_, col_b) = line_col_of(src, "(x) => acc.push(x)");
        let src_content = source_for(&path);
        let f = StrictVoidReturnFilter;
        assert!(f.keep(&svr_diag(&path, line, col_a), Some(&src_content)));
        assert!(!f.keep(&svr_diag(&path, line, col_b), Some(&src_content)));
    }

    // ── no-deprecated ─────────────────────────────────────────────────────────

    fn nd_diag(path: &std::path::Path, line: usize, column: usize) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column,
            rule_id: Cow::Borrowed("no-deprecated"),
            message: "'Line' is deprecated.".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    // Regression for #5325: re-exporting a deprecated class from a barrel file is
    // backward-compat forwarding, not a use of the deprecated API.
    #[test]
    fn nd_drops_named_reexport() {
        let src = "export { Line } from './src/shapes/Line';\n";
        let path = write_temp("nd_named_reexport.ts", src);
        let (line, col) = line_col_of(src, "Line }");
        let src_content = source_for(&path);
        let f = NoDeprecatedFilter;
        assert!(!f.keep(&nd_diag(&path, line, col), Some(&src_content)));
    }

    #[test]
    fn nd_drops_aliased_reexport() {
        let src = "export { Line as FabricLine } from './src/shapes/Line';\n";
        let path = write_temp("nd_aliased_reexport.ts", src);
        let (line, col) = line_col_of(src, "Line as");
        let src_content = source_for(&path);
        let f = NoDeprecatedFilter;
        assert!(!f.keep(&nd_diag(&path, line, col), Some(&src_content)));
    }

    #[test]
    fn nd_drops_multiline_named_reexport() {
        let src =
            "export {\n  Circle,\n  Line,\n  Rect,\n} from './src/shapes';\n";
        let path = write_temp("nd_multiline_reexport.ts", src);
        let (line, col) = line_col_of(src, "  Line,");
        let src_content = source_for(&path);
        let f = NoDeprecatedFilter;
        assert!(!f.keep(&nd_diag(&path, line, col + 2), Some(&src_content)));
    }

    #[test]
    fn nd_drops_namespace_reexport() {
        let src = "export * from './src/shapes/Line';\n";
        let path = write_temp("nd_star_reexport.ts", src);
        let (line, col) = line_col_of(src, "export *");
        let src_content = source_for(&path);
        let f = NoDeprecatedFilter;
        assert!(!f.keep(&nd_diag(&path, line, col), Some(&src_content)));
    }

    #[test]
    fn nd_drops_named_namespace_reexport() {
        let src = "export * as Shapes from './src/shapes';\n";
        let path = write_temp("nd_named_star_reexport.ts", src);
        let (line, col) = line_col_of(src, "export *");
        let src_content = source_for(&path);
        let f = NoDeprecatedFilter;
        assert!(!f.keep(&nd_diag(&path, line, col), Some(&src_content)));
    }

    #[test]
    fn nd_drops_type_reexport() {
        let src = "export type { Line } from './src/shapes/Line';\n";
        let path = write_temp("nd_type_reexport.ts", src);
        let (line, col) = line_col_of(src, "Line }");
        let src_content = source_for(&path);
        let f = NoDeprecatedFilter;
        assert!(!f.keep(&nd_diag(&path, line, col), Some(&src_content)));
    }

    // A genuine use of the deprecated symbol must still fire.
    #[test]
    fn nd_keeps_instantiation() {
        let src = "import { Line } from './shapes/Line';\nconst l = new Line(0, 0, 1, 1);\n";
        let path = write_temp("nd_instantiation.ts", src);
        let (line, col) = line_col_of(src, "new Line(");
        let src_content = source_for(&path);
        let f = NoDeprecatedFilter;
        assert!(f.keep(&nd_diag(&path, line, col + "new ".len()), Some(&src_content)));
    }

    // Regression for the review's MAJOR: a genuine use sitting BELOW a re-export
    // line (the barrel-file scenario) must still fire — the re-export exemption
    // must not bleed onto later statements.
    #[test]
    fn nd_keeps_use_after_reexport() {
        let src = "export { Foo } from './a';\nconst l = new Line();\n";
        let path = write_temp("nd_use_after_reexport.ts", src);
        let (line, col) = line_col_of(src, "new Line(");
        let src_content = source_for(&path);
        let f = NoDeprecatedFilter;
        assert!(f.keep(&nd_diag(&path, line, col + "new ".len()), Some(&src_content)));
    }

    // A use several lines below a multi-line re-export must still fire.
    #[test]
    fn nd_keeps_use_after_multiline_reexport() {
        let src =
            "export {\n  Foo,\n  Bar,\n} from './a';\nconst l = new Line();\n";
        let path = write_temp("nd_use_after_multiline_reexport.ts", src);
        let (line, col) = line_col_of(src, "new Line(");
        let src_content = source_for(&path);
        let f = NoDeprecatedFilter;
        assert!(f.keep(&nd_diag(&path, line, col + "new ".len()), Some(&src_content)));
    }

    // A local `export { X }` WITHOUT a `from` source is the deprecated
    // declaration's own export, not forwarding — it must still fire.
    #[test]
    fn nd_keeps_local_export_without_source() {
        let src = "class Line {}\nexport { Line };\n";
        let path = write_temp("nd_local_export.ts", src);
        let (line, col) = line_col_of(src, "export { Line }");
        let src_content = source_for(&path);
        let f = NoDeprecatedFilter;
        assert!(f.keep(&nd_diag(&path, line, col + "export { ".len()), Some(&src_content)));
    }

    // `from` appearing as an identifier in a real expression must not be
    // mistaken for a re-export `from` clause.
    #[test]
    fn nd_keeps_use_with_from_identifier() {
        let src = "const from = makeLine();\nconst l = new Line(from);\n";
        let path = write_temp("nd_from_identifier.ts", src);
        let (line, col) = line_col_of(src, "new Line(");
        let src_content = source_for(&path);
        let f = NoDeprecatedFilter;
        assert!(f.keep(&nd_diag(&path, line, col + "new ".len()), Some(&src_content)));
    }

    #[test]
    fn nd_keeps_when_source_missing() {
        let f = NoDeprecatedFilter;
        let d = nd_diag(Path::new("src/foo.ts"), 1, 1);
        assert!(f.keep(&d, None));
    }

    // Regression for #5326: a backward-compat test that calls a deprecated API
    // to verify it still works must not be flagged. The call is the test's
    // entire purpose.
    #[test]
    fn nd_drops_use_in_spec_file() {
        let src = "it('addEquals', () => {\n  const returned = point.addEquals(point2);\n});\n";
        let (line, col) = line_col_of(src, "point.addEquals(");
        let f = NoDeprecatedFilter;
        let d = nd_diag(Path::new("src/Point.spec.ts"), line, col);
        assert!(!f.keep(&d, Some(src)));
    }

    #[test]
    fn nd_drops_use_in_test_dir() {
        let f = NoDeprecatedFilter;
        let d = nd_diag(Path::new("src/__tests__/Point.ts"), 1, 1);
        assert!(!f.keep(&d, Some("const r = point.addEquals(point2);\n")));
    }

    // A genuine use in production code is unaffected by the test-file gate.
    #[test]
    fn nd_keeps_use_in_production_file() {
        let src = "import { Line } from './shapes/Line';\nconst l = new Line(0, 0, 1, 1);\n";
        let (line, col) = line_col_of(src, "new Line(");
        let f = NoDeprecatedFilter;
        let d = nd_diag(Path::new("src/Point.ts"), line, col + "new ".len());
        assert!(f.keep(&d, Some(src)));
    }

    // ── unified-signatures ───────────────────────────────────────────────────

    fn us_diag(path: &std::path::Path, line: usize) -> Diagnostic {
        Diagnostic {
            path: Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed("unified-signatures"),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        }
    }

    // Regression for #5506: vobyjs/voby `useEventListener` — each overload pairs
    // a target constraint (`T extends Window`) with the matching event map
    // (`U extends keyof WindowEventMap`). The correlated constraints differ per
    // overload, so no single union-parameter signature expresses them. Drop.
    #[test]
    fn us_drops_target_type_discriminated_dom_overloads() {
        let src = "\
function useEventListener<T extends Window, U extends keyof WindowEventMap>(target: T, event: U): Disposer;
function useEventListener<T extends Document, U extends keyof DocumentEventMap>(target: T, event: U): Disposer;
function useEventListener<T extends HTMLElement, U extends keyof HTMLElementEventMap>(target: T, event: U): Disposer;
function useEventListener(target: unknown, event: string): Disposer {
  return () => {};
}
";
        let path = write_temp("us_dom_overloads.ts", src);
        let line = line_of(src, "T extends Document");
        let src_content = source_for(&path);
        let f = UnifiedSignaturesFilter;
        assert!(!f.keep(&us_diag(&path, line), Some(&src_content)));
    }

    // Guard: overloads with no generics that genuinely collapse into one
    // union-parameter signature still fire.
    #[test]
    fn us_keeps_plain_unifiable_overloads() {
        let src = "\
function foo(x: string): void;
function foo(x: number): void;
function foo(x: string | number): void {}
";
        let path = write_temp("us_plain_overloads.ts", src);
        let line = line_of(src, "x: number");
        let src_content = source_for(&path);
        let f = UnifiedSignaturesFilter;
        assert!(f.keep(&us_diag(&path, line), Some(&src_content)));
    }

    // Guard: overloads whose generic constraints are identical (differing only
    // in a unionizable value parameter) remain genuinely mergeable, so fire.
    #[test]
    fn us_keeps_overloads_with_identical_generic_constraints() {
        let src = "\
function wrap<T extends object>(x: T, k: string): T;
function wrap<T extends object>(x: T, k: number): T;
function wrap<T extends object>(x: T, k: string | number): T {
  return x;
}
";
        let path = write_temp("us_identical_generics.ts", src);
        let line = line_of(src, "k: number");
        let src_content = source_for(&path);
        let f = UnifiedSignaturesFilter;
        assert!(f.keep(&us_diag(&path, line), Some(&src_content)));
    }

    // An overload group mixing a constrained-generic member with a plain one
    // carries divergent constraint fingerprints (one non-empty, one empty), so
    // it is dropped: a meaningful generic constraint and its absence are not
    // unifiable into a single signature.
    #[test]
    fn us_drops_mixed_generic_and_plain_overloads() {
        let src = "\
function pick<T extends object>(x: T): T;
function pick(x: string): string;
function pick(x: unknown): unknown {
  return x;
}
";
        let path = write_temp("us_mixed_generic_plain.ts", src);
        let line = line_of(src, "x: string");
        let src_content = source_for(&path);
        let f = UnifiedSignaturesFilter;
        assert!(!f.keep(&us_diag(&path, line), Some(&src_content)));
    }

    // When the source cannot be read the filter keeps the diagnostic.
    #[test]
    fn us_keeps_when_source_missing() {
        let f = UnifiedSignaturesFilter;
        assert!(f.keep(&us_diag(Path::new("src/foo.ts"), 1), None));
    }

    // ── no-invalid-void-type ─────────────────────────────────────────────────

    fn nivt_diag(path: &std::path::Path, line: usize, col: usize) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: col,
            rule_id: Cow::Borrowed("no-invalid-void-type"),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        }
    }

    // Regression for #5609: pmndrs/valtio WatchCallback — `void` as a union
    // constituent in a callback return type must not fire.
    #[test]
    fn nivt_drops_void_in_callback_return_union() {
        let src = "type WatchCallback = (get: WatchGet) => Cleanup | void;\n";
        let path = write_temp("nivt_callback_return_union.ts", src);
        let (line, col) = line_col_of(src, "void");
        let src_content = source_for(&path);
        let f = NoInvalidVoidTypeFilter;
        assert!(!f.keep(&nivt_diag(&path, line, col), Some(&src_content)));
    }

    // The `void` nested in `Promise<Cleanup | void>` is a generic type argument.
    #[test]
    fn nivt_drops_void_in_promise_generic_arg() {
        let src = "type WatchCallback = (get: WatchGet) => Promise<Cleanup | void>;\n";
        let path = write_temp("nivt_promise_generic_arg.ts", src);
        let (line, col) = line_col_of(src, "void");
        let src_content = source_for(&path);
        let f = NoInvalidVoidTypeFilter;
        assert!(!f.keep(&nivt_diag(&path, line, col), Some(&src_content)));
    }

    #[test]
    fn nivt_drops_void_in_arrow_return_union() {
        let src = "const cb = (): Cleanup | void => undefined;\n";
        let path = write_temp("nivt_arrow_return_union.ts", src);
        let (line, col) = line_col_of(src, "void");
        let src_content = source_for(&path);
        let f = NoInvalidVoidTypeFilter;
        assert!(!f.keep(&nivt_diag(&path, line, col), Some(&src_content)));
    }

    // Negative space: `void` in a non-return union (variable annotation) stays.
    #[test]
    fn nivt_keeps_void_in_variable_union() {
        let src = "let x: string | void;\n";
        let path = write_temp("nivt_variable_union.ts", src);
        let (line, col) = line_col_of(src, "void");
        let src_content = source_for(&path);
        let f = NoInvalidVoidTypeFilter;
        assert!(f.keep(&nivt_diag(&path, line, col), Some(&src_content)));
    }

    // Negative space: `void` in a parameter annotation of a callback type stays.
    #[test]
    fn nivt_keeps_void_in_callback_param() {
        let src = "type Cb = (x: string | void) => number;\n";
        let path = write_temp("nivt_callback_param.ts", src);
        let (line, col) = line_col_of(src, "void");
        let src_content = source_for(&path);
        let f = NoInvalidVoidTypeFilter;
        assert!(f.keep(&nivt_diag(&path, line, col), Some(&src_content)));
    }

    // Negative space: `void` as a generic constraint (`<T extends void>`) is not
    // a generic type argument and stays flagged.
    #[test]
    fn nivt_keeps_void_as_generic_constraint() {
        let src = "type Fn<T extends void> = () => T;\n";
        let path = write_temp("nivt_generic_constraint.ts", src);
        let (line, col) = line_col_of(src, "void");
        let src_content = source_for(&path);
        let f = NoInvalidVoidTypeFilter;
        assert!(f.keep(&nivt_diag(&path, line, col), Some(&src_content)));
    }

    // Negative space: a bare `void` variable annotation stays flagged.
    #[test]
    fn nivt_keeps_bare_void_variable() {
        let src = "let x: void;\n";
        let path = write_temp("nivt_bare_void_variable.ts", src);
        let (line, col) = line_col_of(src, "void");
        let src_content = source_for(&path);
        let f = NoInvalidVoidTypeFilter;
        assert!(f.keep(&nivt_diag(&path, line, col), Some(&src_content)));
    }

    // When the source cannot be read the filter keeps the diagnostic.
    #[test]
    fn nivt_keeps_when_source_missing() {
        let f = NoInvalidVoidTypeFilter;
        assert!(f.keep(&nivt_diag(Path::new("src/foo.ts"), 1, 1), None));
    }

    // ── tsd type-test file (no-unsafe-* family) ─────────────────────────────

    fn unsafe_diag(path: &str, rule_id: &'static str) -> Diagnostic {
        Diagnostic {
            path: Arc::from(Path::new(path)),
            line: 8,
            column: 21,
            rule_id: Cow::Borrowed(rule_id),
            message: "Unsafe call of a(n) error type typed value.".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    // Regression for #5741: tsd `.test-d.ts` files assert types at compile time;
    // `expectType<string>(api.method())` over an unresolved `error` type must not
    // trip the no-unsafe-* family. The filter is path-based, so source is unused.
    #[test]
    fn drops_unsafe_family_in_tsd_test_d_file() {
        let f = TypeTestFileFilter;
        for rule in ["no-unsafe-call", "no-unsafe-argument", "no-unsafe-member-access"] {
            assert!(
                !f.keep(&unsafe_diag("index.test-d.ts", rule), None),
                "{rule} in a .test-d.ts type-test file must be suppressed"
            );
        }
    }

    // `.test-d.tsx` and a `test-d/` directory are the same tsd convention.
    #[test]
    fn drops_unsafe_call_in_test_d_dir_and_tsx() {
        let f = TypeTestFileFilter;
        assert!(!f.keep(&unsafe_diag("test-d/components.ts", "no-unsafe-call"), None));
        assert!(!f.keep(&unsafe_diag("src/Component.test-d.tsx", "no-unsafe-call"), None));
    }

    // Negative space: a plain `.test.ts` runtime unit test can carry a genuine
    // unsafe-`any` bug, so the family must still flag there.
    #[test]
    fn keeps_unsafe_family_in_runtime_test_file() {
        let f = TypeTestFileFilter;
        for rule in ["no-unsafe-call", "no-unsafe-argument", "no-unsafe-member-access"] {
            assert!(
                f.keep(&unsafe_diag("src/api.test.ts", rule), None),
                "{rule} in a runtime .test.ts unit test must still flag"
            );
        }
    }

    // Negative space: ordinary production source still flags.
    #[test]
    fn keeps_unsafe_call_in_production_file() {
        let f = TypeTestFileFilter;
        assert!(f.keep(&unsafe_diag("src/index.ts", "no-unsafe-call"), None));
    }

    // ── no-implied-eval ─────────────────────────────────────────────────────

    fn implied_eval_diag(
        path: &std::path::Path,
        line: usize,
        col: usize,
        message: &str,
    ) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: col,
            rule_id: Cow::Borrowed("no-implied-eval"),
            message: message.to_string(),
            severity: Severity::Error,
            span: None,
        }
    }

    const TIMER_MSG: &str = "Implied eval. Consider passing a function.";

    // Regression for #5888: a function-reference argument is not implied eval.
    #[test]
    fn drops_set_immediate_with_function_ref() {
        let src = "it('x', (done) => {\n  setImmediate(done)\n})\n";
        let path = write_temp("ie_set_immediate_ref.ts", src);
        let (line, col) = line_col_of(src, "setImmediate(done)");
        let col = col + "setImmediate(".len();
        let src_content = source_for(&path);
        let f = NoImpliedEvalFilter;
        assert!(!f.keep(&implied_eval_diag(&path, line, col, TIMER_MSG), Some(&src_content)));
    }

    #[test]
    fn drops_set_immediate_with_arrow() {
        let src = "setImmediate(() => doStuff())\n";
        let path = write_temp("ie_set_immediate_arrow.ts", src);
        let (line, col) = line_col_of(src, "() =>");
        let src_content = source_for(&path);
        let f = NoImpliedEvalFilter;
        assert!(!f.keep(&implied_eval_diag(&path, line, col, TIMER_MSG), Some(&src_content)));
    }

    #[test]
    fn drops_set_timeout_with_named_function_decl() {
        let src = "function tick() {}\nsetTimeout(tick, 0)\n";
        let path = write_temp("ie_set_timeout_fn_decl.ts", src);
        let (line, col) = line_col_of(src, "tick, 0");
        let src_content = source_for(&path);
        let f = NoImpliedEvalFilter;
        assert!(!f.keep(&implied_eval_diag(&path, line, col, TIMER_MSG), Some(&src_content)));
    }

    // Positive space: a string first argument is still implied eval.
    #[test]
    fn keeps_set_immediate_with_string_literal() {
        let src = "setImmediate(\"doStuff()\")\n";
        let path = write_temp("ie_set_immediate_str.ts", src);
        let (line, col) = line_col_of(src, "\"doStuff()\"");
        let src_content = source_for(&path);
        let f = NoImpliedEvalFilter;
        assert!(f.keep(&implied_eval_diag(&path, line, col, TIMER_MSG), Some(&src_content)));
    }

    #[test]
    fn keeps_set_timeout_with_string_literal() {
        let src = "setTimeout(\"code\", 0)\n";
        let path = write_temp("ie_set_timeout_str.ts", src);
        let (line, col) = line_col_of(src, "\"code\"");
        let src_content = source_for(&path);
        let f = NoImpliedEvalFilter;
        assert!(f.keep(&implied_eval_diag(&path, line, col, TIMER_MSG), Some(&src_content)));
    }

    #[test]
    fn keeps_set_timeout_with_string_typed_variable() {
        let src = "const codeStr: string = build()\nsetTimeout(codeStr, 0)\n";
        let path = write_temp("ie_set_timeout_str_var.ts", src);
        let (line, col) = line_col_of(src, "codeStr, 0");
        let src_content = source_for(&path);
        let f = NoImpliedEvalFilter;
        assert!(f.keep(&implied_eval_diag(&path, line, col, TIMER_MSG), Some(&src_content)));
    }

    #[test]
    fn keeps_set_timeout_with_template_literal() {
        let src = "setTimeout(`alert(1)`, 0)\n";
        let path = write_temp("ie_set_timeout_tmpl.ts", src);
        let (line, col) = line_col_of(src, "`alert(1)`");
        let src_content = source_for(&path);
        let f = NoImpliedEvalFilter;
        assert!(f.keep(&implied_eval_diag(&path, line, col, TIMER_MSG), Some(&src_content)));
    }

    // Positive space: the Function-constructor variant is always a true positive.
    #[test]
    fn keeps_new_function_constructor() {
        let src = "const f = new Function(\"return 1\")\n";
        let path = write_temp("ie_new_function.ts", src);
        let (line, col) = line_col_of(src, "new Function");
        let src_content = source_for(&path);
        let f = NoImpliedEvalFilter;
        let msg = "Implied eval. Do not use the Function constructor to create functions.";
        assert!(f.keep(&implied_eval_diag(&path, line, col, msg), Some(&src_content)));
    }
}
