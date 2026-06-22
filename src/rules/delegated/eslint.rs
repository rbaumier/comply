//! ESLint core rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::backend::{Backend, PostFilter};
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY, oxlint_and_clippy, oxlint_delegate};
use std::sync::Arc;

// comply-ignore: max-function-lines — this is a flat data table, not logic; splitting it would scatter related rule entries across files for no readability gain.
pub fn register_all() -> Vec<RuleDef> {
    vec![
        entry(
            "eqeqeq",
            "eqeqeq",
            Severity::Error,
            "Use === over == to avoid type coercion surprises.",
            "Replace `==` with `===` (and `!=` with `!==`). Loose equality \
             triggers implicit coercion rules that hide bugs.",
        ),
        entry(
            "no-var",
            "no-var",
            Severity::Error,
            "Never declare variables with `var`.",
            "Replace `var` with `const` (or `let` only when the binding \
             actually needs to be reassigned).",
        ),
        entry(
            "prefer-const",
            "prefer-const",
            Severity::Error,
            "Prefer `const` over `let` when the binding is never reassigned.",
            "Change `let` to `const` for bindings that are assigned once. \
             The intent becomes explicit and accidental reassignment becomes \
             a compile error.",
        ),
        entry_with_clippy(
            "no-else-return",
            "no-else-return",
            "clippy::redundant_else",
            Severity::Error,
            "Prefer guard clauses over else-after-return.",
            "Remove the `else` after a `return` and de-indent the trailing \
             block. Early returns keep the happy path at the leftmost level.",
        ),
        // `max-params` is handled natively — see `src/rules/max_params/`.
        // The native version exempts fixed-signature library callbacks
        // (TanStack Query `onError`/`queryFn`/etc.) and keeps the same
        // clippy delegation for Rust.
        entry_with_clippy(
            "max-depth",
            "max-depth",
            "clippy::excessive_nesting",
            Severity::Error,
            "Nesting beyond 2 levels is a smell.",
            "Flatten via early return, extract a helper, or invert the \
             condition. Deep nesting hides the happy path.",
        ),
        entry(
            "no-useless-catch",
            "no-useless-catch",
            Severity::Error,
            "A catch that only rethrows is pointless.",
            "If the catch block just rethrows the original error, remove it \
             — the error propagates identically without the ceremony.",
        ),
        // --- v1.1 additions ---
        // `id-length` is handled natively — see `src/rules/id_length/`.
        entry(
            "no-param-reassign",
            "no-param-reassign",
            Severity::Error,
            "Reassigning function parameters mutates the caller's data.",
            "Copy the argument into a local `let` if you need to mutate it. \
             Mutating params silently surprises callers.",
        ),
        entry(
            "no-empty",
            "no-empty",
            Severity::Error,
            "Empty blocks — including empty `catch` — must be justified.",
            "Either handle the case or add a comment naming why the block \
             is intentionally empty. Silent empty blocks rot into bugs.",
        ),
        // --- Biome-parity additions: rules Biome flags that comply lacked. ---
        entry(
            "no-self-assign",
            "no-self-assign",
            Severity::Error,
            "Assigning a variable to itself does nothing.",
            "Remove the `x = x` assignment or fix the typo — you likely \
             meant a different value or property.",
        ),
        entry(
            "max-nested-callbacks",
            "max-nested-callbacks",
            Severity::Warning,
            "Deeply nested callbacks are unreadable.",
            "Flatten nested callbacks by extracting named functions or \
             switching to async/await. Past three levels the control flow \
             is hard to follow.",
        ),
        entry(
            "no-dupe-keys",
            "no-dupe-keys",
            Severity::Error,
            "Duplicate object keys silently overwrite each other.",
            "Remove or rename the duplicate key. Later keys win, so the \
             earlier value is silently discarded.",
        ),
        entry(
            "no-import-assign",
            "no-import-assign",
            Severity::Error,
            "Imported bindings are read-only.",
            "Don't assign to an imported binding or namespace. Imports are \
             immutable live bindings; assigning to them throws in modules.",
        ),
        entry_with_filter(
            "no-template-curly-in-string",
            "no-template-curly-in-string",
            Severity::Error,
            "`${...}` in a regular string is not interpolated.",
            "Switch the quotes to backticks to make it a template literal, \
             or remove the `${}` if it was meant literally. In a normal \
             string it stays verbatim text.",
            Some(Arc::new(NoTemplateCurlyInStringFilter)),
        ),
        entry(
            "no-sequences",
            "no-sequences",
            Severity::Warning,
            "The comma operator hides multiple expressions in one.",
            "Split comma-separated expressions into separate statements. \
             The comma operator evaluates each and returns the last, which \
             surprises most readers.",
        ),
        entry(
            "no-extra-label",
            "no-extra-label",
            Severity::Warning,
            "A label on a loop with no nested target is useless.",
            "Remove the label when `break`/`continue` would target the same \
             loop anyway. Labels only earn their keep when breaking out of \
             an outer loop.",
        ),
        entry(
            "constructor-super",
            "constructor-super",
            Severity::Error,
            "Derived classes must call `super()`; base classes must not.",
            "Add a `super(...)` call in any class that `extends` another, \
             and remove `super()` from classes with no superclass. The \
             wrong shape throws at runtime.",
        ),
        entry(
            "no-setter-return",
            "no-setter-return",
            Severity::Error,
            "A setter's return value is discarded.",
            "Remove the `return value` from the setter — it has no effect. \
             Use a plain method if you need to return something.",
        ),
        entry(
            "no-async-promise-executor",
            "no-async-promise-executor",
            Severity::Error,
            "The `Promise` executor must not be `async`.",
            "Remove `async` from the `new Promise(async (resolve) => ...)` \
             executor. Errors thrown inside an async executor are swallowed \
             and never reject the promise.",
        ),
        entry(
            "no-duplicate-case",
            "no-duplicate-case",
            Severity::Error,
            "Duplicate `case` labels make later branches dead.",
            "Remove or correct the duplicate `case` value. The second \
             identical label can never be reached.",
        ),
        entry(
            "no-div-regex",
            "no-div-regex",
            Severity::Warning,
            "A regex starting with `/=` reads like a division.",
            "Escape the leading equals sign as `/\\=.../`. A bare `/=` at \
             the start of a regex literal is easy to misread as the `/=` \
             operator.",
        ),
        entry(
            "no-lone-blocks",
            "no-lone-blocks",
            Severity::Warning,
            "A standalone block with no block-scoped declarations is dead \
             structure.",
            "Remove the redundant `{ }`. A bare block only matters when it \
             scopes `let`/`const`/`class`; otherwise it just adds \
             indentation.",
        ),
        entry(
            "no-const-assign",
            "no-const-assign",
            Severity::Error,
            "A `const` binding cannot be reassigned.",
            "Reassigning a `const` throws a TypeError at runtime. Declare \
             the variable with `let` if it genuinely needs to change.",
        ),
        entry(
            "no-multi-assign",
            "no-multi-assign",
            Severity::Warning,
            "Chained assignments hide what is being set.",
            "Split `a = b = c` into separate statements. Chained assignment \
             obscures which variables are declared and which are mutated.",
        ),
        entry_with_filter(
            "no-console",
            "no-console",
            Severity::Error,
            "`console` calls leak into production.",
            "Remove the `console.*` call or route it through a real logger. \
             Stray console output clutters production logs and can leak \
             data.",
            Some(Arc::new(NoConsoleFilter)),
        ),
        entry(
            "no-fallthrough",
            "no-fallthrough",
            Severity::Error,
            "A `switch` case must not silently fall through.",
            "Add a `break`/`return`, or an explicit `// fallthrough` \
             comment when it is intentional. Accidental fallthrough runs \
             the next case's code.",
        ),
        entry(
            "no-self-compare",
            "no-self-compare",
            Severity::Error,
            "Comparing a value with itself is trivially true or false.",
            "Replace `x === x` with the intended operands. The only real \
             use, NaN detection, is clearer as `Number.isNaN(x)`.",
        ),
        entry(
            "no-useless-rename",
            "no-useless-rename",
            Severity::Warning,
            "Renaming a binding to the same name is redundant.",
            "Simplify `{ foo: foo }` to `{ foo }` in imports, exports, and \
             destructuring. Renaming a binding to itself is pure noise.",
        ),
        entry(
            "vars-on-top",
            "vars-on-top",
            Severity::Warning,
            "`var` declarations should sit at the top of their scope.",
            "Move `var` declarations to the top of the function so the code \
             matches their hoisted semantics — or better, switch to \
             `let`/`const`.",
        ),
        entry(
            "no-multi-str",
            "no-multi-str",
            Severity::Warning,
            "Escaping a newline to span a string is error-prone.",
            "Use a template literal or string concatenation instead of a \
             `\\` line continuation. A trailing space after the backslash \
             silently breaks it.",
        ),
        entry(
            "operator-assignment",
            "operator-assignment",
            Severity::Warning,
            "Prefer compound assignment operators.",
            "Replace `x = x + 1` with `x += 1`, and likewise for the other \
             operators. Shorthand states the intent and avoids repeating \
             the target.",
        ),
        entry(
            "no-constant-binary-expression",
            "no-constant-binary-expression",
            Severity::Error,
            "A binary expression with a constant operand has no effect.",
            "Fix the expression — a constant side means a comparison or \
             short-circuit always resolves the same way, which is usually a \
             typo such as `a === b === c`.",
        ),
        entry(
            "no-func-assign",
            "no-func-assign",
            Severity::Error,
            "A function declaration must not be reassigned.",
            "Don't assign to the name of a `function` declaration. \
             Overwriting it is almost always a bug; use a separate variable \
             instead.",
        ),
        entry(
            "no-irregular-whitespace",
            "no-irregular-whitespace",
            Severity::Error,
            "Irregular whitespace characters break tooling.",
            "Replace non-standard whitespace such as a non-breaking or \
             zero-width space with a regular space. Invisible characters \
             cause confusing parse and diff errors.",
        ),
        entry_with_filter(
            "no-unassigned-vars",
            "no-unassigned-vars",
            Severity::Error,
            "A variable that is read but never assigned is always \
             undefined.",
            "Assign the variable a value or remove it. A `let`/`var` that \
             is only ever read holds `undefined`, usually a forgotten \
             assignment.",
            Some(Arc::new(NoUnassignedVarsFilter)),
        ),
        entry(
            "no-empty-pattern",
            "no-empty-pattern",
            Severity::Error,
            "An empty destructuring pattern binds nothing.",
            "Replace the empty `{}`/`[]` with the names you meant to \
             extract, or drop the destructuring. `const {} = x` is almost \
             always a mistake.",
        ),
        entry_with_filter(
            "no-new",
            "no-new",
            Severity::Error,
            "`new` used only for side effects throws the object away.",
            "Assign the `new X()` result to a variable, or call a plain \
             function if you only want side effects. A bare `new` signals a \
             misused constructor.",
            Some(Arc::new(NoNewFilter)),
        ),
        entry(
            "no-ex-assign",
            "no-ex-assign",
            Severity::Error,
            "Don't reassign the caught exception.",
            "Use a new local variable instead of assigning to the `catch \
             (e)` binding. Overwriting it loses the original error and \
             confuses debugging.",
        ),
        entry(
            "no-label-var",
            "no-label-var",
            Severity::Error,
            "A label must not share a name with an in-scope variable.",
            "Rename the label or the variable. Reusing the same name makes \
             `break name` ambiguous to readers.",
        ),
        entry(
            "no-unreachable",
            "no-unreachable",
            Severity::Error,
            "Code after a terminating statement never runs.",
            "Remove the unreachable statements after a \
             `return`/`throw`/`break`/`continue`, or fix the control flow \
             that strands them.",
        ),
        entry(
            "no-unused-labels",
            "no-unused-labels",
            Severity::Error,
            "A label that is never referenced is dead.",
            "Remove the unused label. If you meant to jump to it, add the \
             matching `break`/`continue`.",
        ),
        entry(
            "no-class-assign",
            "no-class-assign",
            Severity::Error,
            "A class binding must not be reassigned.",
            "Don't assign to a `class` declaration's name. Reassigning it \
             shadows the class and is almost always a mistake.",
        ),
        entry(
            "no-debugger",
            "no-debugger",
            Severity::Error,
            "`debugger` statements must not be committed.",
            "Remove the `debugger` statement. It halts execution wherever \
             devtools are open and never belongs in shipped code.",
        ),
        entry(
            "no-obj-calls",
            "no-obj-calls",
            Severity::Error,
            "Global namespace objects are not callable.",
            "Don't call `Math()`, `JSON()`, `Reflect()`, and the like — \
             they are namespaces, not functions, so calling them throws a \
             TypeError.",
        ),
        entry(
            "no-this-before-super",
            "no-this-before-super",
            Severity::Error,
            "`this` must not be used before `super()` in a derived \
             constructor.",
            "Move every `this` access after the `super()` call. Touching \
             `this` before the base constructor runs throws a \
             ReferenceError.",
        ),
        entry(
            "yoda",
            "yoda",
            Severity::Warning,
            "Yoda conditions read backwards.",
            "Write `value === 'literal'`, not `'literal' === value`. \
             Putting the variable first reads naturally.",
        ),
        entry(
            "object-shorthand",
            "object-shorthand",
            Severity::Warning,
            "Use shorthand for object properties and methods.",
            "Write `{ foo }` instead of `{ foo: foo }` and `{ method() {} \
             }` instead of `{ method: function () {} }`. Shorthand is the \
             consistent, modern form.",
        ),
        entry(
            "no-alert",
            "no-alert",
            Severity::Error,
            "`alert`, `confirm`, and `prompt` block the UI thread.",
            "Replace `alert`/`confirm`/`prompt` with a non-blocking UI \
             component. These browser dialogs freeze the page and rarely \
             belong in shipped code.",
        ),
        entry(
            "no-extra-boolean-cast",
            "no-extra-boolean-cast",
            Severity::Warning,
            "Redundant boolean casts add noise.",
            "Drop the extra `!!` or `Boolean()` in a context that already \
             coerces to boolean, such as an `if` condition. It is pure \
             noise.",
        ),
        entry(
            "symbol-description",
            "symbol-description",
            Severity::Warning,
            "Every `Symbol()` should carry a description.",
            "Pass a description to `Symbol('...')`. The description is the \
             only thing that identifies a symbol when debugging.",
        ),
        entry(
            "no-compare-neg-zero",
            "no-compare-neg-zero",
            Severity::Error,
            "Comparing against `-0` is misleading.",
            "Replace `x === -0` with `Object.is(x, -0)`. The `===` operator \
             treats `-0` and `0` as equal, so the comparison never does \
             what it looks like.",
        ),
        entry(
            "no-eq-null",
            "no-eq-null",
            Severity::Error,
            "Use `===`/`!==` when comparing with `null`.",
            "Replace `== null` with `=== null`. Loose `== null` quietly \
             matches both `null` and `undefined`; make that intent explicit \
             instead.",
        ),
    ]
}

// Entry-builder helpers used by `register_all` above.

fn entry(
    id: &'static str,
    oxlint_key: &'static str,
    severity: Severity,
    description: &'static str,
    remediation: &'static str,
) -> RuleDef {
    oxlint_delegate(
        RuleMeta {
            id,
            description,
            remediation,
            severity,
            doc_url: None,
            categories: &["typescript"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        oxlint_key,
        TS_FAMILY,
    )
}

/// Same shape as `entry()` but also binds the rule to a clippy lint on Rust.
fn entry_with_clippy(
    id: &'static str,
    oxlint_key: &'static str,
    clippy_lint: &'static str,
    severity: Severity,
    description: &'static str,
    remediation: &'static str,
) -> RuleDef {
    oxlint_and_clippy(
        RuleMeta {
            id,
            description,
            remediation,
            severity,
            doc_url: None,
            categories: &["typescript"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        oxlint_key,
        clippy_lint,
    )
}

/// Same shape as `entry()` but attaches a `PostFilter` to the oxlint backend.
fn entry_with_filter(
    id: &'static str,
    oxlint_key: &'static str,
    severity: Severity,
    description: &'static str,
    remediation: &'static str,
    post_filter: Option<Arc<dyn PostFilter>>,
) -> RuleDef {
    RuleDef {
        meta: RuleMeta {
            id,
            description,
            remediation,
            severity,
            doc_url: None,
            categories: &["typescript"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        backends: TS_FAMILY
            .iter()
            .map(|&lang| {
                (lang, Backend::Oxlint { rule: oxlint_key, post_filter: post_filter.as_ref().map(Arc::clone) })
            })
            .collect(),
    }
}

// ── no-console post-filter ─────────────────────────────────────────────────
//
// `console` is the wrong tool on the server (a structured logger should be
// used) but the correct, standard output primitive in two cases this filter
// drops the diagnostic for:
//
// 1. The file is browser-targeted, where no server logger exists. A file is
//    browser-targeted when any of these hold:
//    a. Its extension is `tsx`/`jsx` (JSX renders into the DOM).
//    b. It carries a `"use client"` / `'use client'` directive.
//    c. It imports a browser-only frontend package (see `BROWSER_PACKAGES`).
//       `react-dom/server` is server-side rendering and is not a browser signal.
//
// 2. The nearest `package.json` declares a `bin` entry — the file belongs to a
//    CLI-tool package whose product *is* terminal output, so `console.log` /
//    `console.error` are the deliberate stdout/stderr API, not stray debug
//    logging. This matches how teams disable `no-console` for CLI packages.
//
// Stray `console.*` is still flagged everywhere stdout is not the product
// (web bundles, libraries, server code without a `bin`).

struct NoConsoleFilter;

impl PostFilter for NoConsoleFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        !is_browser_targeted(&diag.path, source)
    }

    fn keep_with_project(
        &self,
        diag: &crate::diagnostic::Diagnostic,
        source: Option<&str>,
        project: &crate::project::ProjectCtx,
    ) -> bool {
        if is_cli_tool_package(&diag.path, project) {
            return false;
        }
        self.keep(diag, source)
    }
}

/// True when the nearest `package.json` to `path` declares a `bin` field — the
/// file is part of a CLI-tool package whose stdout/stderr is its product, so
/// `console.*` is the intended output API.
fn is_cli_tool_package(path: &std::path::Path, project: &crate::project::ProjectCtx) -> bool {
    project
        .nearest_package_json(path)
        .is_some_and(|pkg| pkg.has_bin)
}

/// Browser-only frontend packages: importing one means the file runs in a
/// browser, where `console` is the standard logging primitive.
const BROWSER_PACKAGES: &[&str] = &[
    "react",
    "react-dom",
    "preact",
    "solid-js",
    "vue",
    "svelte",
    "@sentry/react",
    "@sentry/browser",
    "@sentry/vue",
    "@sentry/svelte",
    "@angular/core",
    "@tanstack/react-router",
];

fn is_browser_targeted(path: &std::path::Path, source: Option<&str>) -> bool {
    if matches!(path.extension().and_then(|e| e.to_str()), Some("tsx") | Some("jsx")) {
        return true;
    }
    let Some(src) = source else {
        return false;
    };
    if src.contains("\"use client\"") || src.contains("'use client'") {
        return true;
    }
    import_specifiers(src).any(is_browser_package)
}

fn is_browser_package(spec: &str) -> bool {
    if spec.starts_with("react-dom/server") {
        return false;
    }
    BROWSER_PACKAGES
        .iter()
        .any(|&pkg| spec == pkg || spec.starts_with(&format!("{pkg}/")))
}

/// Yields each module specifier referenced by `from "..."`, bare
/// `import "..."`, `require("...")`, and dynamic `import("...")`.
///
/// Only lines that look like a module-loading statement are scanned, so a
/// `from "vue"` that merely appears inside a string or template literal is not
/// mistaken for an import.
fn import_specifiers(src: &str) -> impl Iterator<Item = &str> {
    src.lines().filter_map(line_specifier)
}

/// Extracts the module specifier from a single import-like line, if any.
fn line_specifier(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let looks_like_import =
        trimmed.starts_with("import") || line.contains("import(") || line.contains("require(");
    if !looks_like_import {
        return None;
    }
    const MARKERS: &[&str] = &["from ", "import ", "import(", "require("];
    let mut rest = line;
    loop {
        let (marker_pos, marker) = MARKERS
            .iter()
            .filter_map(|&m| rest.find(m).map(|p| (p, m)))
            .min_by_key(|&(p, _)| p)?;
        let after_marker = &rest[marker_pos + marker.len()..];
        let candidate = after_marker.trim_start_matches(['(', ' ']);
        rest = after_marker;
        if let Some(spec) = quoted_specifier(candidate) {
            return Some(spec);
        }
    }
}

/// Extracts the contents of a leading `"..."` / `'...'` string literal.
fn quoted_specifier(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let quote = *bytes.first()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let end = s[1..].find(quote as char)?;
    Some(&s[1..1 + end])
}

// ── no-template-curly-in-string post-filter ────────────────────────────────
//
// The rule flags `${...}` inside a regular (non-template) string, where the
// real bug is a single/double-quoted string that should have been a backtick
// template literal: `"Hi ${name}"`.
//
// The false positive: component registries (e.g. shadcn-vue's
// `registry-examples.ts`) store source code as JSON string data. That source
// frequently contains a *backtick* template literal — `` `$ ${expr}` `` — and
// the `${...}` there is intentional code-as-data, not a non-interpolated
// placeholder. This filter drops the diagnostic when the flagged string value
// holds a backtick before its `${`, i.e. the placeholder is bracketed by an
// embedded template literal. A genuine `"Hi ${name}"` bug has no backtick and
// is still flagged.
//
// Load-bearing oxlint contract: the diagnostic's `(line, column)` points at the
// opening quote of the string literal, and `column` is a 1-based UTF-8 byte
// offset. A wrong assumption fails safe — the position lands off the quote,
// `string_literal_at` returns `None`, and the diagnostic is kept, never masked.

struct NoTemplateCurlyInStringFilter;

impl PostFilter for NoTemplateCurlyInStringFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else {
            return true;
        };
        let Some(content) = string_literal_at(src, diag.line, diag.column) else {
            return true;
        };
        !has_backtick_before_placeholder(content)
    }
}

/// Byte offset of the `(line, column)` position (both 1-based) into `src`.
fn byte_offset(src: &str, line: usize, column: usize) -> Option<usize> {
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

/// Extracts the raw content of the string literal whose opening quote sits at
/// `(line, column)` — the position oxlint reports for this rule. Scans forward
/// from the opening quote to its matching close, honoring `\`-escapes (so an
/// escaped quote inside the value does not end it). Returns `None` if the
/// position is not on a `"`/`'` quote or the string is unterminated.
fn string_literal_at(src: &str, line: usize, column: usize) -> Option<&str> {
    let start = byte_offset(src, line, column)?;
    let bytes = src.as_bytes();
    let quote = *bytes.get(start)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == quote => return Some(&src[start + 1..i]),
            _ => i += 1,
        }
    }
    None
}

/// True when a backtick appears before the first `${` placeholder in `content`,
/// i.e. the placeholder is inside an embedded template literal (code-as-data),
/// not an accidentally non-interpolated regular string.
fn has_backtick_before_placeholder(content: &str) -> bool {
    let Some(placeholder) = content.find("${") else {
        return false;
    };
    content[..placeholder].contains('`')
}

// ── no-new post-filter ──────────────────────────────────────────────────────
//
// `no-new` flags any `new X()` used as a statement (result discarded), reading
// it as a constructor misused for side effects. Two discards are legitimate and
// dropped here:
//
//   1. Throw-assertion callback: the test asserts that *constructing* `X`
//      throws, so the new value is meant to be thrown away
//      (`expect(() => { new Foo() }).to.throw()`,
//      `assert.throws(() => { new Foo() })`, Jest `.toThrow()`).
//   2. Try/catch feature-detection probe: the `new X()` statement sits directly
//      in the `try` block of a `try`/`catch`, where the constructor's throw is
//      the probe signal and the instance is intentionally discarded
//      (`try { new WebAssembly.Module(bytes); } catch { /* unsupported */ }`).
//      The surrounding `catch` is what proves the author relies on the throw.
//
// Unlike the native `no-constructor-side-effects` rule, the delegated diagnostic
// arrives from oxlint with no AST, so the filter re-parses the file and locates
// the `NewExpression` at the diagnostic position, then runs the structural
// checks. A genuine side-effect-only `new Foo();` outside any throw assertion or
// try/catch is still flagged, as is a `new Foo();` in a try block with no catch
// handler (a `finally`-only try is resource cleanup, not a throw probe). Failing
// safe: if the source is unreadable or the position does not resolve to a
// `NewExpression`, the diagnostic is kept.

struct NoNewFilter;

impl PostFilter for NoNewFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else {
            return true;
        };
        let Some(offset) = byte_offset(src, diag.line, diag.column) else {
            return true;
        };
        !new_at_offset_is_exempt(src, &diag.path, offset)
    }
}

/// Re-parse `src` and report whether the `NewExpression` covering byte `offset`
/// (the position oxlint reports for a `no-new` diagnostic — the `new` keyword)
/// is a legitimate discard: the subject of a throw assertion or a statement of a
/// try/catch feature-detection probe. Returns `false` when no
/// `NewExpression` covers the offset, so an unresolved position keeps the
/// diagnostic.
fn new_at_offset_is_exempt(src: &str, path: &std::path::Path, offset: usize) -> bool {
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::GetSpan;

    let allocator = Allocator::default();
    let source_type = crate::oxc_helpers::source_type_for_path(path);
    let parse_ret = Parser::new(&allocator, src, source_type).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;

    // The smallest `NewExpression` whose span contains the diagnostic offset is
    // the one oxlint flagged; nested `new` arguments would have wider-or-equal
    // outer spans, so the minimal span is the precise match.
    let offset = offset as u32;
    let target = semantic
        .nodes()
        .iter()
        .filter(|node| matches!(node.kind(), crate::rules::backend::AstKind::NewExpression(_)))
        .filter(|node| {
            let span = node.kind().span();
            span.start <= offset && offset < span.end
        })
        .min_by_key(|node| node.kind().span().size());
    let Some(target) = target else {
        return false;
    };
    crate::rules::throw_assertion::new_is_throw_assertion_subject(target.id(), &semantic)
        || new_is_try_catch_probe(target.id(), &semantic)
}

/// True when the `NewExpression` identified by `new_node_id` is the expression
/// of an `ExpressionStatement` that is a statement of a `try` block whose
/// `TryStatement` has a `catch` handler. The constructor's throw-or-not is the
/// probe signal there, so discarding the instance is intentional.
///
/// Restricted to the `try` block itself: a `new` in the `catch` body sits under
/// a `CatchClause` (not the `TryStatement`), and a `finally`-only try has no
/// handler, so neither is exempted.
fn new_is_try_catch_probe(
    new_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use crate::rules::backend::AstKind;

    let nodes = semantic.nodes();
    let stmt = nodes.parent_node(new_node_id);
    if !matches!(stmt.kind(), AstKind::ExpressionStatement(_)) {
        return false;
    }
    let block_node = nodes.parent_node(stmt.id());
    let AstKind::BlockStatement(block) = block_node.kind() else {
        return false;
    };
    let AstKind::TryStatement(try_stmt) = nodes.parent_node(block_node.id()).kind() else {
        return false;
    };
    // The try block and the `finally` block are both direct `BlockStatement`
    // children of the `TryStatement` (the catch body hangs off a `CatchClause`),
    // so confirm by identity that this is the `try` block, not the finalizer.
    try_stmt.handler.is_some() && std::ptr::eq(try_stmt.block.as_ref(), block)
}

// ── no-unassigned-vars post-filter ──────────────────────────────────────────
//
// `no-unassigned-vars` flags a `let`/`var` that is read but never assigned. The
// false positive: a TypeScript definite-assignment assertion (`let x!: T`). The
// `!` is the developer's explicit promise to the type checker that the binding
// is assigned before it is read through a channel the analyzer cannot see —
// e.g. a JSX/Solid `ref={el => (x = el)}` callback that populates a DOM
// reference at mount time. The `!` is the canonical opt-out signal, so a
// declarator carrying it must not be reported.
//
// The delegated diagnostic arrives from oxlint with no AST, anchored on the
// binding identifier. The filter re-parses the file, finds the
// `VariableDeclarator` covering that position, and drops the diagnostic when
// its `definite` flag (the `!`) is set. A plain `let x: T;` with no `!` and
// never assigned is still flagged. Failing safe: if the source is unreadable or
// the position does not resolve to a declarator, the diagnostic is kept.

struct NoUnassignedVarsFilter;

impl PostFilter for NoUnassignedVarsFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        let Some(src) = source else {
            return true;
        };
        let Some(offset) = byte_offset(src, diag.line, diag.column) else {
            return true;
        };
        !declarator_at_offset_is_definite(src, &diag.path, offset)
    }
}

/// Re-parse `src` and report whether the `VariableDeclarator` covering byte
/// `offset` (the binding-identifier position oxlint reports for a
/// `no-unassigned-vars` diagnostic) carries a definite-assignment assertion
/// (`let x!: T`). Returns `false` when no declarator covers the offset, so an
/// unresolved position keeps the diagnostic.
fn declarator_at_offset_is_definite(src: &str, path: &std::path::Path, offset: usize) -> bool {
    use oxc_allocator::Allocator;
    use oxc_ast::AstKind;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::GetSpan;

    let allocator = Allocator::default();
    let source_type = crate::oxc_helpers::source_type_for_path(path);
    let parse_ret = Parser::new(&allocator, src, source_type).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;

    // The smallest `VariableDeclarator` whose span contains the offset is the
    // one oxlint flagged; with `let a!: T, b: U` the comma-separated declarators
    // have disjoint spans, so the minimal span pins the exact binding.
    let offset = offset as u32;
    semantic
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            AstKind::VariableDeclarator(decl) => Some(decl),
            _ => None,
        })
        .filter(|decl| {
            let span = decl.span();
            span.start <= offset && offset < span.end
        })
        .min_by_key(|decl| decl.span().size())
        .is_some_and(|decl| decl.definite)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{Diagnostic, Severity};
    use std::borrow::Cow;
    use std::path::Path;

    fn diag(path: &str) -> Diagnostic {
        Diagnostic {
            path: Arc::from(Path::new(path)),
            line: 1,
            column: 1,
            rule_id: Cow::Borrowed("no-console"),
            message: "Unexpected console statement.".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    const FILTER: NoConsoleFilter = NoConsoleFilter;

    // ── frontend (dropped) — exact findings from issue #4040 ─────────────────

    #[test]
    fn drops_tsx_component_importing_react() {
        let src = "import { useState } from \"react\";\nconsole.error(\"Async filter search failed\", searchError);\n";
        let d = diag("src/app/components/data-table/async-multi-select.tsx");
        assert!(!FILTER.keep(&d, Some(src)));
    }

    #[test]
    fn drops_tsx_component_by_extension() {
        // `.tsx` alone is a browser signal even without a known import.
        let src = "export const Grouped = () => null;\nconsole.error(\"failed\");\n";
        let d = diag("src/app/components/data-table/async-multi-select-grouped.tsx");
        assert!(!FILTER.keep(&d, Some(src)));
    }

    #[test]
    fn drops_ts_file_importing_sentry_react() {
        let src = "import type { Breadcrumb } from \"@sentry/react\";\nconsole.debug(\"Sentry replay integration failed to load\", error);\n";
        let d = diag("src/app/lib/sentry.ts");
        assert!(!FILTER.keep(&d, Some(src)));
    }

    #[test]
    fn drops_tsx_route_importing_tanstack_router() {
        let src = "import { createRootRoute } from \"@tanstack/react-router\";\nconsole.warn(\"Failed to load toast chunk, notifications disabled\", error);\n";
        let d = diag("src/app/routes/__root.tsx");
        assert!(!FILTER.keep(&d, Some(src)));
    }

    // ── server (kept) — exact finding from issue #4040 ───────────────────────

    #[test]
    fn keeps_ts_server_file_without_browser_signal() {
        let src = "import process from \"node:process\";\nimport * as Sentry from \"@sentry/bun\";\nimport { z } from \"zod\";\nconsole.warn(\"[ErrorReporter] No Sentry DSN configured\");\n";
        let d = diag("src/api/sentry.ts");
        assert!(FILTER.keep(&d, Some(src)));
    }

    // ── CLI output abstraction (dropped) — issue #5013 (oclif/core) ──────────

    /// Write a `package.json` at `dir` and a source file under `dir/src`, then
    /// run the project-aware filter so `nearest_package_json` resolves the real
    /// manifest. Returns whether the diagnostic is kept.
    fn keep_in_project(pkg_json: &str, rel_file: &str, source: &str) -> bool {
        use std::fs;
        let dir = tempfile::TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file = dir.path().join(rel_file);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        crate::oxc_helpers::reset_file_caches();
        let project = crate::project::ProjectCtx::empty();
        let d = Diagnostic {
            path: Arc::from(file.as_path()),
            line: 1,
            column: 1,
            rule_id: Cow::Borrowed("no-console"),
            message: "Unexpected console statement.".into(),
            severity: Severity::Error,
            span: None,
        };
        FILTER.keep_with_project(&d, Some(source), &project)
    }

    #[test]
    fn drops_console_in_cli_output_abstraction_with_bin() {
        // Issue #5013 — oclif/core's `src/ux/write.ts` wraps `console.log` as the
        // CLI's stdout primitive. The package declares `bin`, so it's a CLI tool
        // whose terminal output is the product.
        let pkg = r#"{"name":"@oclif/core","bin":{"oclif":"./bin/run.js"}}"#;
        let source = "export const stdout = (str) => {\n  console.log(str)\n}\n";
        assert!(!keep_in_project(pkg, "src/ux/write.ts", source));
    }

    #[test]
    fn drops_console_error_in_cli_error_handler_with_bin() {
        // Issue #5013 — `src/errors/handle.ts` prints the error to stderr before
        // exiting, the standard CLI error-output path.
        let pkg = r#"{"name":"@oclif/core","bin":"./bin/run.js"}"#;
        let source = "if (shouldPrint) {\n  console.error(pretty ?? stack)\n}\n";
        assert!(!keep_in_project(pkg, "src/errors/handle.ts", source));
    }

    #[test]
    fn keeps_console_in_library_without_bin() {
        // Negative space: the SAME `console.log` in a published library (no `bin`)
        // is still flagged — stdout is not the product there.
        let pkg = r#"{"name":"some-lib","main":"./dist/index.js"}"#;
        let source = "export function helper() {\n  console.log(\"debug\")\n}\n";
        assert!(keep_in_project(pkg, "src/helper.ts", source));
    }

    // ── focused unit tests ───────────────────────────────────────────────────

    #[test]
    fn drops_ts_file_with_use_client_directive() {
        let src = "\"use client\";\nconsole.log(\"hi\");\n";
        let d = diag("src/app/widget.ts");
        assert!(!FILTER.keep(&d, Some(src)));
    }

    #[test]
    fn keeps_ts_file_importing_react_dom_server() {
        // SSR entry — server-side, not a browser signal.
        let src = "import { renderToString } from \"react-dom/server\";\nconsole.log(\"render\");\n";
        let d = diag("src/server/render.ts");
        assert!(FILTER.keep(&d, Some(src)));
    }

    #[test]
    fn keeps_ts_file_importing_react_redux() {
        // `react-redux` must not match the `react` prefix.
        let src = "import { useSelector } from \"react-redux\";\nconsole.log(\"state\");\n";
        let d = diag("src/store/hooks.ts");
        assert!(FILTER.keep(&d, Some(src)));
    }

    #[test]
    fn extracts_require_and_dynamic_import_specifiers() {
        let src = "const v = require('vue');\nconst lazy = () => import(\"solid-js\");\n";
        assert!(import_specifiers(src).any(|s| s == "vue"));
        assert!(import_specifiers(src).any(|s| s == "solid-js"));
    }

    #[test]
    fn keeps_server_file_mentioning_package_in_string_literal() {
        // `from "vue"` inside a string is not an import — must not flip the file
        // to browser-targeted.
        let src = "const msg = `migrated from \"vue\" to bun`;\nconsole.log(msg);\n";
        let d = diag("src/api/migrate.ts");
        assert!(FILTER.keep(&d, Some(src)));
    }

    // ── no-template-curly-in-string post-filter ──────────────────────────────

    fn tcs_diag(path: &str, line: usize, column: usize) -> Diagnostic {
        Diagnostic {
            path: Arc::from(Path::new(path)),
            line,
            column,
            rule_id: Cow::Borrowed("no-template-curly-in-string"),
            message: "Template placeholders will not interpolate in regular strings".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    #[test]
    fn tcs_drops_code_as_data_string_with_embedded_template_literal() {
        // shadcn-vue registry: component source stored as a JSON string value;
        // the `${...}` belongs to a backtick template literal inside that code.
        // oxlint points at the opening quote of the `content` string (col 14).
        let src = "    content: \"<script setup lang=\\\"ts\\\">\\n:y-formatter=\\\"(tick) => `$ ${new Intl.NumberFormat('us').format(tick)}`\\\"\\n</script>\",\n";
        let d = tcs_diag("deprecated/www/src/registry/registry-examples.ts", 1, 14);
        assert!(!NoTemplateCurlyInStringFilter.keep(&d, Some(src)));
    }

    #[test]
    fn tcs_keeps_genuine_meant_a_template_literal_bug() {
        // The real bug: a double-quoted string that should be a backtick
        // template literal. No backtick precedes the `${name}` placeholder.
        let src = "const s = \"Hi ${name}\";\n";
        let d = tcs_diag("src/greet.ts", 1, 11);
        assert!(NoTemplateCurlyInStringFilter.keep(&d, Some(src)));
    }

    #[test]
    fn tcs_keeps_single_quoted_genuine_bug() {
        let src = "const s = 'total ${count} items';\n";
        let d = tcs_diag("src/cart.ts", 1, 11);
        assert!(NoTemplateCurlyInStringFilter.keep(&d, Some(src)));
    }

    #[test]
    fn tcs_keeps_when_no_source_available() {
        let d = tcs_diag("src/x.ts", 1, 1);
        assert!(NoTemplateCurlyInStringFilter.keep(&d, None));
    }

    #[test]
    fn tcs_keeps_backtick_after_placeholder_only() {
        // A trailing backtick that does not bracket the placeholder is not the
        // code-as-data shape — keep the diagnostic.
        let src = "const s = \"x ${y} `\";\n";
        let d = tcs_diag("src/x.ts", 1, 11);
        assert!(NoTemplateCurlyInStringFilter.keep(&d, Some(src)));
    }

    // ── no-new filter tests ──────────────────────────────────────────────────

    /// Build a `no-new` diagnostic pointing at the (single) `new` keyword in
    /// `src`, mirroring oxlint's reported position (1-based line/column on the
    /// `new` token).
    fn no_new_diag(path: &str, src: &str) -> Diagnostic {
        let idx = src.find("new ").expect("source must contain a `new` token");
        let line = src[..idx].bytes().filter(|&b| b == b'\n').count() + 1;
        let line_start = src[..idx].rfind('\n').map_or(0, |p| p + 1);
        let column = idx - line_start + 1;
        Diagnostic {
            path: Arc::from(Path::new(path)),
            line,
            column,
            rule_id: Cow::Borrowed("no-new"),
            message: "Do not use 'new' for side effects.".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    #[test]
    fn no_new_dropped_in_chai_expect_to_throw() {
        // Regression for #5100 — node-oidc-provider Chai `.to.throw()` idiom.
        let src =
            "expect(() => {\n  new Configuration({ features: { foo: {} } });\n}).to.throw('x');\n";
        let d = no_new_diag("test/configuration.test.js", src);
        assert!(!NoNewFilter.keep(&d, Some(src)));
    }

    #[test]
    fn no_new_dropped_in_assert_throws() {
        let src = "assert.throws(() => {\n  new Foo();\n});\n";
        let d = no_new_diag("test/foo.test.js", src);
        assert!(!NoNewFilter.keep(&d, Some(src)));
    }

    #[test]
    fn no_new_dropped_in_jest_to_throw() {
        let src = "expect(() => {\n  new Foo();\n}).toThrow();\n";
        let d = no_new_diag("test/foo.test.ts", src);
        assert!(!NoNewFilter.keep(&d, Some(src)));
    }

    #[test]
    fn no_new_kept_for_genuine_side_effect_statement() {
        // A side-effect-only `new` outside any throw assertion still flags.
        let src = "function f() {\n  new Logger('global');\n}\n";
        let d = no_new_diag("src/log.ts", src);
        assert!(NoNewFilter.keep(&d, Some(src)));
    }

    #[test]
    fn no_new_kept_in_plain_callback() {
        // A `new` in a non-throw-assertion callback is a genuine discard.
        let src = "arr.forEach(() => {\n  new X();\n});\n";
        let d = no_new_diag("src/each.ts", src);
        assert!(NoNewFilter.keep(&d, Some(src)));
    }

    #[test]
    fn no_new_kept_when_source_unavailable() {
        let d = no_new_diag("src/x.ts", "function f() {\n  new X();\n}\n");
        assert!(NoNewFilter.keep(&d, None));
    }

    #[test]
    fn no_new_dropped_in_try_catch_wasm_probe() {
        // Regression for #5461 — wasm-feature-detect's canonical probe: the
        // constructor's throw is the feature signal, the instance is discarded.
        let src = "export default () => {\n  try {\n    new WebAssembly.Module(bytes);\n    return true;\n  } catch (e) {\n    return false;\n  }\n};\n";
        let d = no_new_diag("src/detectors/multi-memory/index.js", src);
        assert!(!NoNewFilter.keep(&d, Some(src)));
    }

    #[test]
    fn no_new_dropped_in_try_catch_with_empty_catch() {
        // A bare `catch {}` still proves reliance on the constructor's throw.
        let src = "try {\n  new Foo();\n} catch {}\n";
        let d = no_new_diag("src/probe.ts", src);
        assert!(!NoNewFilter.keep(&d, Some(src)));
    }

    #[test]
    fn no_new_kept_in_try_with_finally_only() {
        // A `finally`-only try is resource cleanup, not a throw probe — no catch
        // handler means the discard is not justified.
        let src = "try {\n  new Foo();\n} finally {\n  cleanup();\n}\n";
        let d = no_new_diag("src/cleanup.ts", src);
        assert!(NoNewFilter.keep(&d, Some(src)));
    }

    #[test]
    fn no_new_kept_in_catch_block() {
        // A `new` in the catch body is a genuine side-effect discard, not a probe.
        let src = "try {\n  doWork();\n} catch (e) {\n  new Logger('err');\n}\n";
        let d = no_new_diag("src/log.ts", src);
        assert!(NoNewFilter.keep(&d, Some(src)));
    }

    // ── no-unassigned-vars filter tests ──────────────────────────────────────

    /// Build a `no-unassigned-vars` diagnostic pointing at the binding
    /// identifier `name` in `src`, mirroring oxlint's reported position (1-based
    /// line/column on the variable name).
    fn no_unassigned_diag(path: &str, src: &str, name: &str) -> Diagnostic {
        let idx = src.find(name).expect("source must contain the binding name");
        let line = src[..idx].bytes().filter(|&b| b == b'\n').count() + 1;
        let line_start = src[..idx].rfind('\n').map_or(0, |p| p + 1);
        let column = idx - line_start + 1;
        Diagnostic {
            path: Arc::from(Path::new(path)),
            line,
            column,
            rule_id: Cow::Borrowed("no-unassigned-vars"),
            message: format!("'{name}' is always 'undefined' because it's never assigned."),
            severity: Severity::Error,
            span: None,
        }
    }

    #[test]
    fn no_unassigned_dropped_for_definite_assertion() {
        // Regression for #5492 — solidjs/solid: `let div!: T` populated via a
        // JSX `ref={div}` callback the analyzer cannot see.
        let src = "function f() {\n  let div!: HTMLDivElement;\n  return div.innerHTML;\n}\n";
        let d = no_unassigned_diag("test/dynamic.spec.tsx", src, "div");
        assert!(!NoUnassignedVarsFilter.keep(&d, Some(src)));
    }

    #[test]
    fn no_unassigned_dropped_for_definite_assertion_first_in_list() {
        // `let div!: T, disposer: () => void` — only the `div` declarator carries
        // the `!`; the minimal-span declarator pins the right one.
        let src =
            "function f() {\n  let div!: HTMLDivElement, disposer: () => void;\n  return div;\n}\n";
        let d = no_unassigned_diag("test/dynamic.spec.tsx", src, "div");
        assert!(!NoUnassignedVarsFilter.keep(&d, Some(src)));
    }

    #[test]
    fn no_unassigned_kept_for_plain_never_assigned_let() {
        // A plain `let x: T;` with no `!` and never assigned still flags.
        let src = "function g() {\n  let plain: string;\n  return plain.length;\n}\n";
        let d = no_unassigned_diag("src/g.ts", src, "plain");
        assert!(NoUnassignedVarsFilter.keep(&d, Some(src)));
    }

    #[test]
    fn no_unassigned_kept_for_plain_let_after_definite_in_list() {
        // In `let div!: T, plain: U`, the non-definite `plain` declarator still
        // flags even though its sibling carries the assertion.
        let src = "function f() {\n  let div!: HTMLDivElement, plain: string;\n  return plain;\n}\n";
        let d = no_unassigned_diag("test/dynamic.spec.tsx", src, "plain");
        assert!(NoUnassignedVarsFilter.keep(&d, Some(src)));
    }

    #[test]
    fn no_unassigned_kept_when_source_unavailable() {
        let src = "function f() {\n  let div!: HTMLDivElement;\n  return div;\n}\n";
        let d = no_unassigned_diag("src/x.tsx", src, "div");
        assert!(NoUnassignedVarsFilter.keep(&d, None));
    }
}
