# Skill-driven rules expansion

**Date**: 2026-04-21
**Status**: Draft

## Problem Statement

Comply has ~648 rules covering TypeScript, JavaScript, Rust, and Vue. These rules were
accumulated organically — from Sonar, unicorn, oxlint, and hand-crafted patterns. The
developer's skill library (Claude Code skills) encodes a separate body of best practices
for ~15 technology domains. These two corpora have never been compared systematically.

The result: whole technology domains have zero or minimal static analysis coverage —
i18n (0 rules), TanStack Query (2 rules despite a rich v5 rule set), Better Auth (0 rules),
TanStack Start (0 rules), and many others. Developers using comply on a modern TypeScript
stack get no feedback on patterns their skills explicitly call out as bugs or anti-patterns.

## Solution

Implement ~118 new native comply rules derived directly from the skill library, prioritized
by implementation order agreed with the developer. Each new rule maps a skill "do/don't"
to a detectable AST or text pattern, keeping the same architecture (tree-sitter AstCheck
or TextCheck, one directory per rule, 2+ tests per rule).

The implementation is batched into 15 domain groups. Each group ships as a coherent unit
that can be reviewed and merged independently.

## User Stories

1. As a comply user on a Next.js project, I want a warning when I write `export default function`,
   so that my codebase enforces named exports consistently without a separate ESLint rule.
2. As a comply user, I want a warning when I `JSON.parse()` a value without any type guard,
   so that I don't silently introduce `unknown`-typed data into typed code.
3. As a comply user, I want a warning when I cast with `as unknown as T`,
   so that I'm forced to use a proper type guard instead of bypassing the type system.
4. As a comply user, I want a warning when I write sequential `await` calls for independent
   async operations, so that I'm reminded to use `Promise.all` and avoid unnecessary waterfalls.
5. As a comply user working with React, I want a warning when a Server Action performs a
   mutation without calling `auth()`/`getSession()`, so that I don't accidentally expose
   unauthenticated mutation endpoints.
6. As a comply user, I want a warning when I write conditional JSX with `&&` on a potentially
   falsy value (0, ""), so that I avoid rendering `0` or empty strings to the DOM.
7. As a comply user, I want a warning when I use `dangerouslySetInnerHTML` without adjacent
   sanitization, so that I catch XSS vectors at analysis time.
8. As a comply user working with Tailwind, I want a warning when I use the `!` important
   modifier, so that I know I'm fighting specificity rather than fixing it.
9. As a comply user, I want a warning when I use arbitrary z-index values `z-[n]` instead
   of design tokens, so that z-index is always managed systematically.
10. As a comply user, I want a warning when two `w-` and `h-` utilities share the same value,
    so that I'm reminded to use the `size-*` shorthand.
11. As a comply user running comply on SQL migration files, I want a warning when I write
    `CREATE INDEX` without `CONCURRENTLY`, so that I avoid table locks in production.
12. As a comply user, I want a warning when a column is declared nullable without a comment,
    so that every nullable column has an explicit rationale.
13. As a comply user writing Rust, I want a warning when I use `lazy_static!` or
    `once_cell::sync::Lazy` instead of `std::sync::OnceLock`/`LazyLock`, so that I'm
    on the modern stdlib equivalent.
14. As a comply user writing Rust, I want a warning when a `pub fn` returns `Result<T, E>`
    in a library crate without `#[must_use]`, so that callers can't silently discard errors.
15. As a comply user writing Rust, I want a warning when `Arc<Mutex<Vec<` is used as the
    main pattern for collecting task results, so that I'm steered toward `mpsc::channel`.
16. As a comply user on TanStack Start, I want a warning when a `createServerFn` doesn't
    call `.safeParse`/`.parse` on its arguments, so that server function inputs are always
    validated at the RPC boundary.
17. As a comply user on TanStack Start, I want a warning when a `createServerFn` file
    doesn't follow the `.functions.ts` naming convention, so that server/client code
    separation is enforced structurally.
18. As a comply user on TanStack Query v5, I want warnings on removed or renamed APIs
    (`isLoading`, `cacheTime`, `keepPreviousData: true`, `onSuccess` on `useQuery`), so
    that migration to v5 is guided by the linter.
19. As a comply user, I want a warning when `QueryClient` is constructed without a default
    `staleTime`, so that I don't accidentally get refetch-on-every-mount behavior.
20. As a comply user on an API-designed project, I want a warning when a list endpoint
    handler returns a root-level array, so that I design extensible response envelopes from
    the start.
21. As a comply user, I want a warning when a GET handler returns results without any
    pagination mechanism, so that I don't forget pagination until I have 10,000 rows.
22. As a comply user working with Zod, I want a warning when I call `.parse()` directly
    in a route handler or middleware (instead of `.safeParse()`), so that unhandled
    `ZodError` throws don't leak schema internals to clients.
23. As a comply user, I want a warning when I chain `.optional().nullable()` instead of
    `.nullish()`, so that I don't accidentally create a harder-to-read type.
24. As a comply user, I want a warning when a `.refine()` cross-field validation has no
    `path:` option, so that form error messages attach to the right field.
25. As a comply user writing Vue, I want a warning when a SFC uses `setup()` inside
    `<script>` without the `setup` attribute, so that Options API is not sneaking back in.
26. As a comply user writing Vue, I want a warning when the SFC section order is wrong
    (`<template>` before `<script>`), so that the canonical order is enforced.
27. As a comply user writing Vue, I want a warning when `v-html` is used without adjacent
    sanitization evidence, so that XSS vectors are flagged.
28. As a comply user writing Vue, I want a warning when a `onMounted` adds a global
    listener without a corresponding `onUnmounted` cleanup, so that memory leaks from
    listener accumulation are caught early.
29. As a comply user writing Vue, I want a warning when a Pinia store is destructured
    without `storeToRefs()`, so that reactivity is preserved.
30. As a comply user on an i18n project, I want a warning on string literals that appear
    directly as JSX text content, so that I don't ship hardcoded user-visible strings.
31. As a comply user on an i18n project, I want a warning when a translation key is built
    by string concatenation inside `t()`, so that i18next can't statically extract it.
32. As a comply user, I want a warning when `.toLocaleDateString()` is called without
    an explicit locale, so that dates always render in the correct locale.
33. As a comply user, I want a warning when pluralization is handled with `count === 1 ?`
    instead of `t(key, { count })`, so that CLDR plural rules are always respected.
34. As a comply user, I want a warning when `req.body` (or equivalent) is spread directly
    into a database operation, so that mass assignment privilege escalation is prevented.
35. As a comply user, I want a warning when `err.message` or `err.stack` appears in a
    response body, so that internal error details are never leaked to clients.
36. As a comply user, I want a warning when `exec()` is called with a variable argument
    (as opposed to `execFile()` with an args array), so that command injection vectors
    are flagged.
37. As a comply user writing Rust, I want a warning when I use `.context()` or
    `.with_context()` is missing on a `?` operator in an application crate, so that
    errors carry actionable context.
38. As a comply user, I want a warning when a Better Auth config sets
    `disableCSRFCheck: true` or `disableOriginCheck: true`, so that security settings
    are never silently disabled.
39. As a comply user, I want a warning when a Better Auth plugin is imported from the
    generic `better-auth/plugins` path instead of its dedicated path, so that tree-shaking
    works correctly.
40. As a comply user, I want a warning when `vi.mock()` factories reference variables
    declared outside `vi.hoisted()`, so that hoisting-related undefined errors are caught
    at analysis time rather than at runtime.
41. As a comply user, I want a warning when a test file mocks `fetch` or `axios` globally
    instead of using MSW, so that network-level mocking is consistent.
42. As a comply user working with Drizzle ORM, I want a warning when an `insert` or
    `update` is not chained with `.returning()`, so that I don't waste a second round-trip
    on a follow-up SELECT.
43. As a comply user, I want a warning when `sql.raw()` is called with a non-literal
    argument, so that SQL injection via raw interpolation is flagged.

## Implementation Decisions

### Architecture: no new concepts, same pattern

Every rule follows the existing comply architecture:
- One directory per rule under `src/rules/`
- `mod.rs` with `RuleMeta` (id, description, remediation, severity, categories, doc_url)
- `typescript.rs` (AstCheck) or `text.rs` (TextCheck) for the implementation
- Tests inline in the backend file, using `run_ts()`, `run_tsx()`, `run_rust()` helpers
- Registration in `src/rules/mod.rs` (`pub mod` + `all_rule_defs()` entry)

No new helpers, no new backend types, no new macros — unless two or more rules in the
same batch share a non-trivial detection pattern, in which case a helper in
`rust_helpers.rs` or a new `*_helpers.rs` may be extracted.

### Batch ordering and categories

Rules are implemented in 15 domain batches, in the order specified:

1. **TypeScript/Architecture** — categories: `typescript`, `architecture`, `code-quality`
2. **React** — categories: `react`
3. **Tailwind** — categories: `tailwind`
4. **Database SQL** — categories: `database`, `sql`, `migrations`
5. **Rust** — categories: `rust`
6. **TanStack Start** — categories: `tanstack-start`
7. **TanStack Query** — categories: `tanstack`
8. **API Design** — categories: `api`
9. **Zod** — categories: `zod`
10. **Vue** — categories: `vue`
11. **i18n** — categories: `i18n`
12. **Security** — categories: `security`
13. **Better Auth** — categories: `better-auth`
14. **Testing** — categories: `testing`
15. **Drizzle ORM** — categories: `drizzle`

### Feasibility tiers

Before implementing each rule, determine the feasibility tier:

- **Facile** (~40% of rules): single-node AST pattern or text regex. Implement directly.
- **Moyen** (~45%): multi-node pattern or cross-node relationship. Requires careful walker
  traversal; may need helper extraction.
- **Difficile** (~15%): data-flow, cross-file analysis, or heuristic. Implement a
  conservative approximation (high precision, lower recall) or defer.

Difficult rules are never skipped — they ship with a narrow, safe heuristic and a comment
in `mod.rs` explaining what the rule cannot catch.

### Severity policy

- Rules catching potential runtime errors or security issues → `Severity::Error`
- Rules enforcing best practices with clear remediation → `Severity::Warning`
- Stylistic rules (ordering, naming conventions) → `Severity::Warning`
- Rules that require human judgment to verify → `Severity::Warning` (never Error)

### doc_url policy

- Rules derived from a skill that references an external doc → `doc_url: Some("...")` 
- Rules with no obvious upstream doc → `doc_url: None`

### Deduplication check

Before implementing any rule, grep for the candidate id and for synonyms in `src/rules/`.
Rules already covered by existing logic under a different name are not reimplemented.
Known candidates requiring verification before implementation:
- `no-common-grab-bag` (check `no-common-grab-bag` in existing rules)
- `no-new-regex-with-variable` (check if already catches handler context)
- `sql-no-between-timestamp` (check if text rule covers Drizzle DSL too)
- `no-raw-db-entity-in-handler` (check if covers `req.body` spread)

### TanStack Query v5 batch specifics

The six v5 API-rename rules (`no-is-loading`, `no-cache-time`, `no-use-error-boundary`,
`no-keep-previous-data-prop`, `no-query-callbacks`, `no-enabled-true`) are pure text or
simple property-access patterns. They ship as a single PR since they are trivially fast to
implement and form a coherent migration guide.

### i18n batch specifics

`i18n-no-hardcoded-string-in-jsx` requires distinguishing JSX text content from
attribute values. The rule must not flag: `className`, `href`, `src`, `id`, `name`,
`data-*`, `aria-*`, placeholder values with no user-visible text. It only flags string
literals that are direct children of JSX elements (rendered text).

### Security batch specifics

`no-mass-assignment` must distinguish `req.body` (Express/Hono context) from other
spread patterns. It fires only when the spread target is a known DB operation call
(`db.update(`, `db.insert(`, `.set(`, `.values(`).

`no-error-details-in-response` must not fire on test files or intentional debug routes.
A `// comply-ignore` escape hatch is already available.

### Rust batch specifics

`rust-anyhow-context-on-question-mark` is only enforced in binary/application crates
(detected by `[lib]` absence in Cargo.toml or `src/main.rs` presence). Library crates
may legitimately bubble errors without context — adding context is the caller's job.

## Testing Decisions

Every rule ships with at minimum:
1. One test asserting the violation is flagged (with the exact number of diagnostics)
2. One test asserting the correct pattern produces zero diagnostics
3. For rules with nuanced conditions (context-dependent, file-type-dependent), one test
   per significant condition variant

Tests use the existing `run_ts()`, `run_tsx()`, `run_rust()` helpers from `test_helpers.rs`.
For Vue rules, `run_vue_template()` or the text backend equivalent.

Regression tests: for any rule where a false positive was discovered during development,
the false-positive input is added as a passing test case before the fix is committed.

The full test suite (`cargo nextest run`) must remain green and complete in under 6 seconds
after each batch. Clippy must pass with `-D warnings`.

## Out of Scope

- Rules requiring cross-file data-flow analysis (e.g., "this variable came from user input
  three files ago"). These remain as `Difficile` approximations or are deferred.
- Changes to the rule registry format, RuleMeta struct, or test helper API.
- Oxlint/Clippy delegated rules — new rules in this PRD are native comply rules only.
- LLM-backed rules for the new domains — only static AST/text rules.
- Automatic fixes (--fix) — comply does not yet support auto-fix; this PRD does not change that.
- Docker, CI-CD, Kubernetes domains — these skills operate on config files outside comply's
  language targets (TypeScript, JavaScript, Rust, Vue).
- Swift — comply does not yet support Swift.
- Rules requiring runtime information (e.g., "this endpoint is publicly exposed").

## Further Notes

- The `RULES_TO_ADD.md` file at repo root is the authoritative candidate list. It will be
  updated as rules are implemented (crossing off completed entries) and as new false
  positives or missed cases are discovered.
- The implementation order (TypeScript/Architecture first) was chosen by the developer
  based on: widest applicability across the codebase, lowest domain-specific knowledge
  required, and highest likelihood of catching bugs in comply's own source.
- Total estimated new rules: ~118. Some will be merged (near-identical detection), some
  deferred (infeasible heuristic). Realistic shipped count: 90–105.
- Each batch is a self-contained PR. Batch size target: 6–12 rules per PR, keeping
  review surface manageable.
