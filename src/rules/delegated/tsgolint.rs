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
        entry(
            "promise-function-async",
            "promise-function-async",
            "Function returns a Promise but is not marked `async`.",
            "Add the `async` keyword to the function declaration.",
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
        entry(
            "no-unnecessary-condition",
            "no-unnecessary-condition",
            "Condition is always truthy or always falsy based on types.",
            "Remove the condition or fix the type.",
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
        entry(
            "unbound-method",
            "unbound-method",
            "Method passed as callback loses its `this` binding.",
            "Bind the method: `.bind(this)` or use an arrow function.",
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
        entry(
            "no-unnecessary-type-parameters",
            "no-unnecessary-type-parameters",
            "Type parameter is never used or could be `unknown`.",
            "Remove the unused type parameter.",
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
        entry(
            "no-redundant-type-constituents",
            "no-redundant-type-constituents",
            "`string | \"foo\"` is redundant — the literal is subsumed.",
            "Remove the redundant type constituent.",
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
        entry(
            "strict-void-return",
            "strict-void-return",
            "Function declared void but caller expects a value.",
            "Fix the return type or don't use the return value.",
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
        entry(
            "no-misused-spread",
            "no-misused-spread",
            "Spread `...x` on incompatible type loses data.",
            "Ensure spread is used on the correct type.",
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
}
