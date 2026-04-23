//! tsgolint type-aware rules delegated to tsgolint subprocess.
//!
//! These rules require the TypeScript type checker and only run with --with-types.
//! tsgolint uses typescript-go for ~10x faster type checking than tsc.

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{Language, RuleDef, TS_FAMILY};

pub fn register_all() -> Vec<RuleDef> {
    vec![
        // Async / Promises
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
        entry(
            "await-thenable",
            "await-thenable",
            "`await` on a non-thenable value has no effect.",
            "Remove the `await` or ensure the value is a Promise.",
        ),
        entry(
            "require-await",
            "require-await",
            "Async function has no `await` — either add one or remove `async`.",
            "Add an `await` expression or convert to a regular function.",
        ),
        entry(
            "promise-function-async",
            "promise-function-async",
            "Function returns a Promise but is not marked `async`.",
            "Add the `async` keyword to the function declaration.",
        ),
        // Type Safety - any leaks
        entry(
            "no-unsafe-argument",
            "no-unsafe-argument",
            "Passing `any` to a typed parameter defeats type safety.",
            "Add a type assertion or fix the source of `any`.",
        ),
        entry(
            "no-unsafe-assignment",
            "no-unsafe-assignment",
            "Assigning `any` to a typed variable defeats type safety.",
            "Add a type assertion or fix the source of `any`.",
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
        // Boolean / Conditions
        entry(
            "strict-boolean-expressions",
            "strict-boolean-expressions",
            "Condition is not explicitly boolean — implicit coercion is error-prone.",
            "Use an explicit comparison: `!== undefined`, `> 0`, `Boolean(x)`.",
        ),
        entry(
            "no-unnecessary-condition",
            "no-unnecessary-condition",
            "Condition is always truthy or always falsy based on types.",
            "Remove the condition or fix the type.",
        ),
        // Nullish
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
        // Arrays
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
        // Errors
        entry(
            "only-throw-error",
            "only-throw-error",
            "Throwing a non-Error value loses stack trace information.",
            "Throw an Error instance: `throw new Error(message)`.",
        ),
        // Methods
        entry(
            "unbound-method",
            "unbound-method",
            "Method passed as callback loses its `this` binding.",
            "Bind the method: `.bind(this)` or use an arrow function.",
        ),
        // Operators
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
        // Unions / Enums
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
        // Type assertions
        entry(
            "no-unnecessary-type-assertion",
            "no-unnecessary-type-assertion",
            "Type assertion `as T` is unnecessary — value is already that type.",
            "Remove the type assertion.",
        ),
        // Misc
        entry(
            "consistent-type-exports",
            "consistent-type-exports",
            "Type-only exports should use `export type`.",
            "Add the `type` keyword: `export type { Foo }`.",
        ),
        entry(
            "return-await",
            "return-await",
            "`return await` is unnecessary outside of try/catch.",
            "Remove `await` from the return statement.",
        ),
    ]
}

fn entry(
    id: &'static str,
    rule_name: &'static str,
    description: &'static str,
    remediation: &'static str,
) -> RuleDef {
    // Leak the prefixed string to get a 'static lifetime.
    // This is intentional — rule definitions live for the entire process.
    let oxlint_key: &'static str = Box::leak(format!("typescript/{rule_name}").into_boxed_str());

    let backends: Vec<(Language, Backend)> = TS_FAMILY
        .iter()
        .map(|&lang| (lang, Backend::Tsgolint { rule: oxlint_key }))
        .collect();

    RuleDef {
        meta: RuleMeta {
            id,
            description,
            remediation,
            severity: Severity::Error,
            doc_url: Some("https://typescript-eslint.io/rules/"),
            categories: &["typescript", "type-aware"],
        },
        backends,
    }
}
