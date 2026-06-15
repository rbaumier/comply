use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    // Files under a `__fixtures__/` directory are input data (e.g. Babel transform
    // input fixtures), not test specs, even when nested inside `__tests__/`.
    if path.components().any(|c| c.as_os_str() == "__fixtures__") {
        return false;
    }
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__")
}

/// `true` when the filename declares the file a test spec (`*.test.*` /
/// `*.spec.*`). Such a file claims to hold test cases, so an empty one is still
/// flagged. Files that merely live under `__tests__/` without this naming are
/// candidates for the fixture/setup exemption below.
fn is_named_test_spec(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.")
}

/// Lifecycle hooks of the major runners. A file built only from these is a
/// setup file (e.g. a Vitest `setupFiles` entry that resets state before each
/// test) rather than an empty spec.
const LIFECYCLE_HOOK_MARKERS: &[&str] =
    &["beforeEach(", "afterEach(", "beforeAll(", "afterAll("];

/// `true` when the file is shared test infrastructure rather than a spec:
/// either a setup file (only lifecycle hooks, no assertions) or a fixture/helper
/// module (exports values, no test cases). Files explicitly named `*.test.*` /
/// `*.spec.*` are never treated as infrastructure — they claim to be specs.
fn is_test_infrastructure(path: &std::path::Path, source: &str) -> bool {
    if is_named_test_spec(path) {
        return false;
    }
    has_lifecycle_hook(source) || exports_value(source)
}

fn has_lifecycle_hook(source: &str) -> bool {
    LIFECYCLE_HOOK_MARKERS
        .iter()
        .any(|marker| source.contains(marker))
}

/// `true` when the file exports a value (`export const`, `export function`,
/// `export default`, `export class`, `export { ... }`), marking it a module
/// imported by spec files as a fixture rather than a test itself.
fn exports_value(source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("export ") || trimmed.starts_with("export{")
    })
}

/// Runtime test markers: standard runner/assertion calls (Jest, Mocha, Vitest,
/// Node `assert`).
const RUNTIME_TEST_MARKERS: &[&str] =
    &["test(", "it(", "describe(", "expect(", "assert(", "assert."];

/// Runner functions whose calls may be chained through modifier properties
/// (`it.concurrent(`, `test.each([...])(`, `describe.skip(`, `it.concurrent.skip(`).
/// The bare `name(` form is already covered by [`RUNTIME_TEST_MARKERS`].
const CHAINABLE_RUNNERS: &[&str] = &["it", "test", "describe"];

/// Compile-time type-assertion markers. Type-testing spec files (tsd,
/// `@mui/types`) verify TypeScript inference with these instead of runtime
/// `expect()`; they are real assertions, so a file using only them is not empty.
/// Both the call form (`expectError(...)`) and the generic form
/// (`expectType<T>(value)`) are recognized.
const TYPE_TEST_MARKERS: &[&str] = &[
    "expectType<",
    "expectError(",
    "expectError<",
    "expectAssignable<",
    "expectNotAssignable<",
    "expectDeprecated(",
    "expectDeprecated<",
];

/// The `@ts-expect-error` directive asserts that the following line fails to
/// type-check — a compile-time test on its own.
const TS_EXPECT_ERROR_DIRECTIVE: &str = "@ts-expect-error";

/// Performance-test framework packages whose `.spec.ts` files are scenario
/// classes (a `PerfTest<T>` subclass with an `async run()` entry point), not
/// unit-test files. The framework discovers them by the `*.spec.ts` pattern, so
/// importing from such a package marks the file as a real perf-test scenario
/// rather than an empty test file.
const PERF_TEST_FRAMEWORKS: &[&str] = &["@azure-tools/test-perf"];

fn has_test_content(source: &str) -> bool {
    for marker in RUNTIME_TEST_MARKERS.iter().chain(TYPE_TEST_MARKERS) {
        if source.contains(marker) {
            return true;
        }
    }
    if source.contains(TS_EXPECT_ERROR_DIRECTIVE) {
        return true;
    }
    if has_chained_runner_call(source) {
        return true;
    }
    if is_perf_test_scenario(source) {
        return true;
    }
    drives_imported_runner(source) || drives_required_runner(source)
}

/// `true` when the source declares a test via a modifier-chained runner call:
/// `it`/`test`/`describe` followed by one or more `.<ident>` segments and then
/// `(` — e.g. `it.concurrent(`, `it.concurrent.skip(`, `test.each([...])(`,
/// `describe.only(`. The bare `name(` form is handled by [`RUNTIME_TEST_MARKERS`];
/// this covers only the chained shape that those substrings miss.
///
/// The runner name must stand alone (not be the tail of a larger identifier like
/// `submit` or `wait`), so prose such as `// it.would be nice` is not matched.
fn has_chained_runner_call(source: &str) -> bool {
    CHAINABLE_RUNNERS
        .iter()
        .any(|runner| has_chained_call_for(source, runner))
}

fn has_chained_call_for(source: &str, runner: &str) -> bool {
    let bytes = source.as_bytes();
    let mut search_from = 0;
    while let Some(offset) = source[search_from..].find(runner) {
        let start = search_from + offset;
        let after = start + runner.len();
        search_from = after;
        // The runner must be preceded by a non-identifier character (or be at the
        // start of the file) so we don't match the tail of `submit`, `await`, etc.
        if start > 0 && is_ident_byte(bytes[start - 1]) {
            continue;
        }
        // A `.<ident>` chain must follow the runner name to form the chained call.
        if matches_chained_call(&source[after..]) {
            return true;
        }
    }
    false
}

/// `true` when `rest` (the text right after a runner name) is one or more
/// `.<ident>` segments followed by a `(` — i.e. the modifier-chained call form.
fn matches_chained_call(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    let mut i = 0;
    let mut saw_modifier = false;
    while bytes.get(i) == Some(&b'.') {
        i += 1;
        let ident_start = i;
        while i < bytes.len() && is_ident_byte(bytes[i]) {
            i += 1;
        }
        if i == ident_start {
            // A `.` not followed by an identifier (e.g. `it..` or `it.(`) is not a
            // modifier chain.
            return false;
        }
        saw_modifier = true;
    }
    saw_modifier && bytes.get(i) == Some(&b'(')
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// True when the file imports from a perf-test framework package, marking it as
/// a scenario class rather than an empty unit-test file.
fn is_perf_test_scenario(source: &str) -> bool {
    PERF_TEST_FRAMEWORKS
        .iter()
        .any(|pkg| source.contains(pkg))
}

/// True when the file imports a symbol and then invokes it as a top-level
/// statement, e.g. `import { testTokenization } from '...'; testTokenization(...)`.
///
/// This captures project-local custom test runners that wrap `it`/`test`
/// internally, so the file carries real test logic without any standard marker.
/// A genuinely empty test file (lone import, comment, or bare declaration) has
/// no such top-level invocation and is still flagged.
fn drives_imported_runner(source: &str) -> bool {
    let imported = imported_bindings(source);
    if imported.is_empty() {
        return false;
    }
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        imported.iter().any(|name| is_call_statement(trimmed, name))
    })
}

/// `true` when `line` begins with `name(`, i.e. `name` is being called at the
/// start of a statement rather than referenced as part of a larger identifier.
fn is_call_statement(line: &str, name: &str) -> bool {
    let Some(rest) = line.strip_prefix(name) else {
        return false;
    };
    rest.starts_with('(')
}

/// Collects the binding names introduced by `import` statements: default,
/// namespace (`* as ns`), and named bindings (honoring `as` aliases).
fn imported_bindings(source: &str) -> Vec<String> {
    let mut bindings = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(after_import) = trimmed.strip_prefix("import ") else {
            continue;
        };
        // Side-effect import (`import './x'`) has no bindings.
        let Some(clause) = after_import.split(" from ").next() else {
            continue;
        };
        collect_clause_bindings(clause, &mut bindings);
    }
    bindings
}

fn collect_clause_bindings(clause: &str, bindings: &mut Vec<String>) {
    if let Some(open) = clause.find('{') {
        let head = &clause[..open];
        for name in head.split(',') {
            push_binding(name, bindings);
        }
        if let Some(close) = clause[open + 1..].find('}') {
            let inner = &clause[open + 1..open + 1 + close];
            for spec in inner.split(',') {
                let alias = spec.split(" as ").last().unwrap_or(spec);
                push_binding(alias, bindings);
            }
        }
    } else {
        for name in clause.split(',') {
            push_binding(name, bindings);
        }
    }
}

/// Normalizes a raw import token (default name or `* as ns`) and records it if
/// it is a plain identifier.
fn push_binding(raw: &str, bindings: &mut Vec<String>) {
    let token = raw.trim();
    let name = token.rsplit(" as ").next().unwrap_or(token).trim();
    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
        bindings.push(name.to_string());
    }
}

/// True when the file delegates its tests through a CommonJS `require`, the CJS
/// counterpart of [`drives_imported_runner`]. Two shapes count:
///
/// * an inline call on the require result, e.g.
///   `require('./helper').payloadMethod(t)` — the module's exported runner is
///   invoked directly; and
/// * a `const`/`let`/`var` binding of a require result that is then called as a
///   top-level statement, e.g. `const run = require('./helper').run; run(t)` or
///   `const { runner } = require('./helper'); runner(t)`.
///
/// A bare side-effect `require('./setup')` (no method call, no binding usage) is
/// not test delegation, mirroring how a lone ESM `import './setup'` is ignored.
fn drives_required_runner(source: &str) -> bool {
    if source.lines().any(|line| has_inline_require_call(line.trim_start())) {
        return true;
    }
    let bound = required_bindings(source);
    if bound.is_empty() {
        return false;
    }
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        bound.iter().any(|name| invokes_binding(trimmed, name))
    })
}

/// `true` when `line` invokes `name` at statement start, either directly
/// (`name(...)`) or through a method on it (`name.method(...)`) — the latter is
/// the common CJS shape where the whole module object is bound and one of its
/// exported runners is called.
fn invokes_binding(line: &str, name: &str) -> bool {
    let Some(rest) = line.strip_prefix(name) else {
        return false;
    };
    // Reject a longer identifier (`helperFn` when `name` is `helper`).
    match rest.as_bytes().first() {
        Some(&b'(') => true,
        Some(&b'.') => matches_chained_call(rest),
        _ => false,
    }
}

/// `true` when `line` invokes a method on the result of a `require(...)` call,
/// e.g. `require('./helper').payloadMethod(t)`. The `require(...)` may sit at the
/// statement start or after an assignment (`const x = require('./helper').run()`).
fn has_inline_require_call(line: &str) -> bool {
    let mut search_from = 0;
    while let Some(offset) = line[search_from..].find("require(") {
        let after_require = search_from + offset + "require(".len();
        search_from = after_require;
        // Skip past the `require(...)` argument list to find what follows the
        // closing paren.
        let Some(close) = matching_paren(&line[after_require..]) else {
            continue;
        };
        let rest = line[after_require + close + 1..].trim_start();
        // A `.method(` chain on the require result is a delegated runner call.
        if matches_chained_call(rest) {
            return true;
        }
    }
    false
}

/// Returns the byte offset, within `rest`, of the `)` that closes an already-open
/// `require(` argument list, accounting for nested parens and quoted strings.
fn matching_paren(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'\'' | b'"' | b'`' => quote = Some(b),
                b'(' => depth += 1,
                b')' if depth == 0 => return Some(i),
                b')' => depth -= 1,
                _ => {}
            },
        }
    }
    None
}

/// Collects identifiers bound to a `require(...)` result: both the default form
/// (`const x = require('./m')`) and destructured names
/// (`const { a, b: c } = require('./m')`, recording `a` and the `c` alias).
fn required_bindings(source: &str) -> Vec<String> {
    let mut bindings = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(after_kw) = strip_declaration_keyword(trimmed) else {
            continue;
        };
        let Some(eq) = after_kw.find('=') else {
            continue;
        };
        let (target, value) = after_kw.split_at(eq);
        if !value.contains("require(") {
            continue;
        }
        collect_require_targets(target.trim(), &mut bindings);
    }
    bindings
}

/// Records the binding names in a `require` declarator target: a plain
/// identifier (`helper`), or destructured names with optional CJS `:` renames
/// (`{ runner }`, `{ runner: r }` records `r`).
fn collect_require_targets(target: &str, bindings: &mut Vec<String>) {
    let inner = match (target.find('{'), target.rfind('}')) {
        (Some(open), Some(close)) if close > open => &target[open + 1..close],
        _ => target,
    };
    for spec in inner.split(',') {
        // A `key: local` rename binds the right-hand name; a bare `key` binds itself.
        let name = spec.rsplit(':').next().unwrap_or(spec).trim();
        if !name.is_empty() && name.chars().all(is_binding_char) {
            bindings.push(name.to_string());
        }
    }
}

fn is_binding_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Strips a leading `const`/`let`/`var` keyword (with its trailing space) from a
/// statement, returning the declarator text after it.
fn strip_declaration_keyword(line: &str) -> Option<&str> {
    ["const ", "let ", "var "]
        .iter()
        .find_map(|kw| line.strip_prefix(kw))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }
        if has_test_content(ctx.source) {
            return Vec::new();
        }
        if is_test_infrastructure(ctx.path, ctx.source) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "no-empty-test-file".into(),
            message:
                "Test file contains no test assertions (`test(`, `it(`, `describe(`, `expect(`)."
                    .into(),
            severity: Severity::Error,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_empty_test_file() {
        assert_eq!(
            run("utils.test.ts", "import { foo } from './foo';").len(),
            1
        );
    }

    #[test]
    fn flags_empty_spec_file() {
        assert_eq!(run("utils.spec.ts", "// TODO: add tests").len(), 1); // comply-ignore: todo-needs-issue-link — test content, not a real marker.
    }

    #[test]
    fn flags_tests_dir() {
        assert_eq!(
            run("__tests__/utils.ts", "import { foo } from './foo';").len(),
            1
        );
    }

    #[test]
    fn allows_vitest_setup_file() {
        let source = "import { beforeEach } from 'vitest';\n\
                      import { setActivePinia } from '../src';\n\
                      \n\
                      beforeEach(() => {\n\
                      \tsetActivePinia(undefined);\n\
                      });\n";
        assert!(run("packages/pinia/__tests__/vitest-setup.ts", source).is_empty());
    }

    #[test]
    fn allows_fixture_store_module() {
        let source = "import { defineStore } from '../../../src';\n\
                      import { useUserStore } from './user';\n\
                      \n\
                      export const useCartStore = defineStore('cart', {\n\
                      \tstate: () => ({ id: 2, rawItems: [] as string[] }),\n\
                      });\n";
        assert!(run("packages/pinia/__tests__/pinia/stores/cart.ts", source).is_empty());
    }

    #[test]
    fn flags_empty_spec_named_file_even_with_export() {
        // A file explicitly named `.spec.` claims to be a test: an `export`
        // does not turn it into exempt fixture infrastructure.
        assert_eq!(
            run("__tests__/cart.spec.ts", "export const fixture = 1;").len(),
            1
        );
    }

    #[test]
    fn allows_test_file_with_tests() {
        assert!(
            run(
                "utils.test.ts",
                "test('adds 1+1', () => { expect(1+1).toBe(2); });"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_non_test_file() {
        assert!(run("utils.ts", "export const foo = 1;").is_empty());
    }

    #[test]
    fn allows_fixtures_input_file() {
        assert!(
            run(
                "packages/babel-plugin/__tests__/__fixtures__/dynamic.js",
                "const x = <div/>"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_nested_fixtures_input_file() {
        assert!(run("__tests__/dynamic/__fixtures__/foo.js", "import './bar';").is_empty());
    }

    #[test]
    fn allows_assert_style_tests() {
        assert!(
            run(
                "plugin.test.js",
                "import assert from 'assert';\nassert.equal(result, expected);"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_custom_imported_runner() {
        let source = "import { testTokenization } from '../test/testRunner';\n\
                      \n\
                      testTokenization('m3', [\n\
                      \t[{ line: '(**)', tokens: [{ startIndex: 0, type: 'comment.m3' }] }]\n\
                      ]);\n";
        assert!(run("src/languages/definitions/m3/m3.test.ts", source).is_empty());
    }

    #[test]
    fn allows_aliased_imported_runner() {
        let source = "import { testTokenization as runTokens } from './testRunner';\n\
                      runTokens('lang', []);\n";
        assert!(run("lang.test.ts", source).is_empty());
    }

    #[test]
    fn flags_imported_but_uncalled_binding() {
        assert_eq!(
            run("utils.test.ts", "import { helper } from './helper';").len(),
            1
        );
    }

    #[test]
    fn allows_cjs_require_inline_runner() {
        // fastify/fastify test/put.test.js: tests are delegated to a factory via
        // an inline call on the `require(...)` result.
        let source = "'use strict'\n\
                      \n\
                      const t = require('node:test')\n\
                      require('./helper').payloadMethod('put', t)\n\
                      require('./input-validation').payloadMethod('put', t)\n";
        assert!(run("test/put.test.js", source).is_empty());
    }

    #[test]
    fn allows_cjs_require_bound_runner() {
        let source = "'use strict'\n\
                      const helper = require('./helper')\n\
                      const t = require('node:test')\n\
                      helper.payloadMethod('put', t)\n";
        assert!(run("test/put.test.js", source).is_empty());
    }

    #[test]
    fn allows_cjs_require_destructured_runner() {
        let source = "const { payloadMethod } = require('./helper')\n\
                      payloadMethod('put')\n";
        assert!(run("test/patch.test.js", source).is_empty());
    }

    #[test]
    fn flags_cjs_side_effect_require_only() {
        // A bare side-effect require with no method call and no called binding is
        // not test delegation — the file stays empty.
        assert_eq!(
            run("test/put.test.js", "require('./setup')\n").len(),
            1
        );
    }

    #[test]
    fn allows_type_assertion_spec_with_expect_type() {
        let source = "import * as React from 'react';\n\
                      import { expectType } from '@mui/types';\n\
                      \n\
                      const elem: HTMLDivElement | null = null;\n\
                      expectType<HTMLDivElement | null, typeof elem>(elem);\n\
                      expectType<HTMLDivElement | null, typeof elem>(elem);\n";
        assert!(run("packages/mui-material/test/typescript/styles.spec.tsx", source).is_empty());
    }

    #[test]
    fn allows_ts_expect_error_only_spec() {
        let source = "import { Button } from './Button';\n\
                      \n\
                      // @ts-expect-error color must be a known palette key\n\
                      <Button color=\"not-a-color\" />;\n";
        assert!(run("components.spec.tsx", source).is_empty());
    }

    #[test]
    fn flags_empty_spec_with_imports_and_comments_only() {
        let source = "import * as React from 'react';\n\
                      import { Button } from './Button';\n\
                      // setup only, no assertions\n";
        assert_eq!(run("components.spec.tsx", source).len(), 1);
    }

    #[test]
    fn allows_it_concurrent_only_spec() {
        // `it.concurrent(...)` is a real Jest test declaration; the chained
        // modifier must not hide it from the marker scan. No `expect(` here, so
        // detection relies solely on recognizing the chained-call shape.
        let source = "it.concurrent('one', () => { return Promise.resolve(); });\n";
        assert!(run("e2e/concurrent.test.js", source).is_empty());
    }

    #[test]
    fn allows_chained_modifier_only_spec() {
        // `it.concurrent.skip`, `test.each([...])`, `it.todo()` are all test
        // declarations on the `it`/`test` namespace.
        let source = "it.concurrent.skip('two', () => {});\n\
                      test.each([1, 2])('y %i', n => {});\n\
                      it.todo();\n";
        assert!(run("e2e/modifiers.test.js", source).is_empty());
    }

    #[test]
    fn flags_empty_spec_with_no_chained_call() {
        // A genuinely empty spec (imports/comments only, no test call of any
        // form) is still flagged even when the substring `it.` appears in prose.
        let source = "import { Button } from './Button';\n\
                      // it.would be nice to test this\n";
        assert_eq!(run("components.spec.tsx", source).len(), 1);
    }

    #[test]
    fn allows_azure_perf_test_scenario_spec() {
        let source = "// Copyright (c) Microsoft Corporation.\n\
                      import { AvroSerializerTest } from './avroSerializerTest.spec.js';\n\
                      import type { PerfOptionDictionary } from '@azure-tools/test-perf';\n\
                      \n\
                      export class SerializeTest extends AvroSerializerTest<SerializePerfTestOptions> {\n\
                      \toptions: PerfOptionDictionary<SerializePerfTestOptions> = {};\n\
                      \tarray: number[];\n\
                      \n\
                      \tasync run(): Promise<void> {\n\
                      \t\tawait this.serializer.serialize(data, AvroSerializerTest.schema);\n\
                      \t}\n\
                      }\n";
        assert!(run("sdk/schemaregistry/perf-tests/src/serialize.spec.ts", source).is_empty());
    }
}
