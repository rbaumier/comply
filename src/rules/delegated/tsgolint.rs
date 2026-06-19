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
        entry(
            "no-explicit-any",
            "no-explicit-any",
            "Explicit `any` defeats TypeScript's type safety.",
            "Use `unknown`, a specific type, or a generic.",
        ),
        entry(
            "no-unsafe-argument",
            "no-unsafe-argument",
            "Passing `any` to a typed parameter defeats type safety.",
            "Add a type assertion or fix the source of `any`.",
        ),
        entry_with_filter(
            "no-unsafe-assignment",
            "no-unsafe-assignment",
            "Assigning `any` to a typed variable defeats type safety.",
            "Add a type assertion or fix the source of `any`.",
            Some(Arc::new(NoUnsafeAssignmentFilter)),
        ),
        entry(
            "no-unsafe-call",
            "no-unsafe-call",
            "Calling a value typed as `any` is unsafe.",
            "Add proper types or use a type guard.",
        ),
        entry(
            "no-unsafe-member-access",
            "no-unsafe-member-access",
            "Accessing a member on `any` is unsafe.",
            "Add proper types or use a type guard.",
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
        entry(
            "no-inferrable-types",
            "no-inferrable-types",
            "`const x: number = 5` — the type is inferred from the value.",
            "Remove the type annotation.",
        ),
        // ══════════════════════════════════════════════════════════════════
        // TYPES — BAD PATTERNS
        // ══════════════════════════════════════════════════════════════════
        entry(
            "no-wrapper-object-types",
            "no-wrapper-object-types",
            "`String` should be `string` — use primitive types.",
            "Use lowercase primitive: `string`, `number`, `boolean`.",
        ),
        entry(
            "no-invalid-void-type",
            "no-invalid-void-type",
            "`void` is only valid as a return type, not a variable type.",
            "Use `undefined` for variables, `void` only for returns.",
        ),
        entry(
            "no-misused-new",
            "no-misused-new",
            "Interface with `new()` or class with `constructor` type is wrong.",
            "Use proper constructor signature.",
        ),
        entry(
            "no-empty-interface",
            "no-empty-interface",
            "Empty interface has no members — use `type` or remove it.",
            "Add members, use `type = {}`, or remove the interface.",
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
        entry(
            "consistent-type-imports",
            "consistent-type-imports",
            "Type-only imports should use `import type`.",
            "Add the `type` keyword: `import type { Foo }`.",
        ),
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
        entry(
            "unified-signatures",
            "unified-signatures",
            "Overloads can be unified into a single signature.",
            "Use union type in single signature instead of overloads.",
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
        entry(
            "no-deprecated",
            "no-deprecated",
            "Using deprecated API that may be removed in future.",
            "Replace with the recommended alternative.",
        ),
        entry(
            "no-base-to-string",
            "no-base-to-string",
            "`.toString()` on object without override returns `[object Object]`.",
            "Implement custom `.toString()` or use JSON.stringify.",
        ),
        entry(
            "no-implied-eval",
            "no-implied-eval",
            "`setTimeout(\"code\")` executes string as code like eval.",
            "Pass a function instead of a string.",
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
    }
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

// ── no-redundant-type-constituents post-filter ────────────────────────────
//
// Drops false positives on the `keyof T & string` narrowing idiom.
// The `& string` constituent is intentional — it filters out numeric/symbol
// keys that `keyof T` can include. Checked in a ±5-line window around the
// diagnostic to handle multi-line satisfies clauses.

const NRTC_WINDOW: usize = 5;

struct NoRedundantTypeConstituentsFilter;

impl PostFilter for NoRedundantTypeConstituentsFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else {
            return true;
        };
        !nrtc_is_keyof_string_narrowing_fp(src, diag.line)
    }
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
// 2. Multi-line function signatures where tsgolint only sees the first
//    occurrence of a type parameter.

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
    let mut lines_with_param: usize = 0;
    for next_line in lines.iter().skip(line_1based).take(15) {
        if ep_contains_word(next_line, &param_name) {
            lines_with_param += 1;
            if lines_with_param >= 2 {
                return true;
            }
        }
        let mut hit_body = false;
        for b in next_line.bytes() {
            match b {
                b'(' => paren_depth += 1,
                b')' => {
                    paren_depth -= 1;
                    if paren_depth < 0 {
                        break;
                    }
                }
                b'{' if paren_depth <= 0 => {
                    hit_body = true;
                    break;
                }
                _ => {}
            }
        }
        if hit_body || paren_depth < 0 {
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
// Two FP shapes are dropped:
// 1. `vi.fn()` mocks — inline or aliased via const/let/var. (Closes #…)
// 2. `renderHook(() => …)` callbacks — the callback must return the hook
//    value. A 2-line window is used to avoid bleeding into adjacent calls.

struct StrictVoidReturnFilter;

impl PostFilter for StrictVoidReturnFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else {
            return true;
        };
        !svr_is_vi_fn_fp(src, diag.line, diag.column) && !svr_is_render_hook_fp(src, diag.line)
    }
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
}
