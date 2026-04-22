# Hickey-inspired Rules — Coverage Audit

Audit of 10 rules inspired by Rich Hickey's *Simple Made Easy* talk
against the current comply rule set.

Legend:

- DONE — covered 1:1 by an existing rule.
- PARTIAL — covered by a narrower or adjacent rule; a dedicated rule would add value.
- TODO — not covered, needs implementation.

---

## 1. Immutability

### 1.1 `no-let-var` — autoriser uniquement `const`

- Status: PARTIAL
- Existing coverage:
  - `no-var` (delegated to oxlint, Error) — forbids `var`. See `src/rules/delegated/eslint.rs:27`.
  - `prefer-const` (delegated to oxlint, Error) — forces `const` when the
    binding is never reassigned. See `src/rules/delegated/eslint.rs:35`.
- Gap: comply has no blanket "ban `let` even when reassigned" rule. A
  strict Hickey mode would flag every `let`, pushing the user towards
  derived values or reducers instead of rebinding.
- Feasibility: easy. An `AstCheck` on TypeScript that flags every
  `lexical_declaration` whose first child is `let` would suffice. Needs
  an opt-out for `for`/`for-of`/`for-in` headers (tree-sitter places
  those `let` tokens inside `for_statement`, so filtering by parent kind
  is straightforward). High FP risk on real codebases — likely ship as
  severity `Warning` under category `functional`.

### 1.2 `no-param-reassign` — interdire modification des arguments

- Status: DONE
- Existing coverage:
  - `no-param-reassign` (delegated to oxlint, Error). See
    `src/rules/delegated/eslint.rs:94`.
- Note: delegation covers the ESLint semantics (reassignment only, not
  deep property mutation). If deep-mutation detection is wanted, that is
  the scope of `no-property-mutation` below — keep them separate.

### 1.3 `no-mutation-methods` — interdire `push`, `splice`, etc.

- Status: PARTIAL
- Existing coverage:
  - `no-array-sort-mutation` — only `.sort()` → `.toSorted()`.
    See `src/rules/no_array_sort_mutation/mod.rs`.
  - `no-array-reverse` — only `.reverse()` → `.toReversed()`.
    See `src/rules/no_array_reverse/`.
  - `no-array-delete` — flags `delete arr[i]`.
  - `no-immediate-mutation` — flags mutation right after declaration.
    See `src/rules/no_immediate_mutation/mod.rs`.
  - `vue-no-mutate-prop` — Vue-specific, template scope only.
- Gap: no single rule that bans the full mutating-method set
  (`push`, `pop`, `shift`, `unshift`, `splice`, `copyWithin`, `fill`,
  `sort`, `reverse`) across the codebase. Existing rules are surgical
  (each method has a preferred immutable replacement); a blanket rule
  would overlap with them but enforce the Hickey "no in-place mutation"
  stance even when an immutable replacement doesn't exist (e.g.
  `push` → spread).
- Feasibility: medium. `AstCheck` on TypeScript call expressions whose
  `member_expression` callee name is in the banned set. FP risk on
  non-array receivers (`Map#set` has the same shape but is not an array
  method — need some narrowing, e.g. "only flag when we can prove the
  receiver is an array literal / typed `Array<T>` / assignment from an
  array method"). Practical compromise: flag high-confidence cases only
  (receiver is an array literal, `Array.from`, `.map`, `.filter`, etc.).

### 1.4 `no-property-mutation` — interdire `obj.prop = value`

- Status: TODO
- Existing coverage:
  - None directly. `no-param-reassign` catches parameter reassignment
    but not `param.x = y`. `vue-no-mutate-prop` is template-only.
- Feasibility: medium/hard. Detect `assignment_expression` whose left
  side is a `member_expression` or `subscript_expression`. Very high FP
  rate out of the box (class constructors, imperative builders, React
  refs `.current`, `this.state` in old code, DOM nodes). Needs
  exceptions for:
  - `this.* = …` inside a constructor body;
  - `*.current = …` on refs (`useRef`, `createRef`);
  - class field initialization in methods marked as builders;
  - known DOM-style APIs (`style.*`, `dataset.*`, `innerText`, etc.).
  Better shipped as `category: "functional"` opt-in with a narrow
  allow-list.

---

## 2. Déclaratif

### 2.1 `no-reduce` — bannir `Array.reduce()`

- Status: DONE
- Existing coverage:
  - `no-array-reduce` — bans `.reduce()` and `.reduceRight()`.
    See `src/rules/no_array_reduce/mod.rs`.
  - Companion rule `reduce-initial-value` — ensures the seed is
    explicit when `.reduce()` is used despite the above.

### 2.2 `no-imperative-loops` — bannir `for(let i=0)`, `while`, `do-while`

- Status: PARTIAL
- Existing coverage:
  - `no-for-loop` — flags classic indexed `for (let i = 0; i < n; i++)`
    and recommends `for-of` / `.entries()`. Active on TS + Rust.
    See `src/rules/no_for_loop/mod.rs`.
  - `prefer-while` — narrows degenerate `for (;;)` / `for (;cond;)` to
    `while`. See `src/rules/prefer_while/mod.rs`.
  - `no-for-in-iterable` — bans `for-in` over iterables.
  - `ts-no-loop-func` — forbids closures inside loops.
- Gap: no rule bans `while` / `do-while` / `for-of` outright in favour
  of `map` / `filter` / recursion. The Hickey stance ("loops are
  complected iteration + mutation + control") would demand banning all
  three. A dedicated `no-imperative-loops` rule would:
  1. flag `while_statement`, `do_statement`;
  2. optionally flag `for_of_statement` when the body mutates an
     external accumulator (very hard to detect reliably);
  3. steer users to `Array#map/filter/flatMap/forEach` or recursion.
- Feasibility: easy for (1) and (2); the accumulator-detection
  heuristic is noisy and probably best left out. Ship as opt-in
  `category: "functional"` because `while (true)` event loops and
  stream pumps are legitimate.

---

## 3. Simplification objets

### 3.1 `no-class-inheritance` — interdire `extends`

- Status: DONE
- Existing coverage:
  - `no-class-inheritance` — flags every `extends` on a class
    declaration/expression, category `functional`.
    See `src/rules/no_class_inheritance/mod.rs`.

### 3.2 `no-this-mutation` — interdire mutation de `this` hors constructeur

- Status: PARTIAL
- Existing coverage:
  - `no-this-assignment` — flags `const self = this` aliases.
    See `src/rules/no_this_assignment/mod.rs`.
  - `ts-no-invalid-this` — `this` outside a class is a bug.
  - `react-no-this-in-sfc` — React function-component scope.
- Gap: no rule that specifically allows `this.x = y` inside a
  constructor but forbids it in every other method. This is the Hickey
  "objects should be values; their state is fixed at construction"
  stance.
- Feasibility: medium. `AstCheck` on `assignment_expression` where the
  LHS is `this.*` or `this[…]`, walking up to the nearest
  `method_definition` and checking whether its name is `constructor`.
  Needs to cover `Object.assign(this, …)` and destructuring writes too
  if we want to be thorough. Low FP rate once the constructor exception
  is implemented correctly.

---

## 4. Contrôle de flux

### 4.1 `require-exhaustive-switch` — switch sur unions discriminées uniquement

- Status: TODO
- Existing coverage:
  - Adjacent but not equivalent: `prefer-switch-over-chained-if`,
    `no-small-switch`, `no-useless-switch-case`, `no-case-label-in-switch`,
    `switch-case-braces`, `switch-case-break-position`, `no-nested-switch`,
    `comma-or-logical-or-case`.
  - None of these verify that the union of discriminator values is
    covered.
- Feasibility: hard without type information. Tree-sitter alone cannot
  resolve the discriminator's union type; comply would have to:
  - either lean on `tsc` via the `Tsc` backend (the `Backend` enum in
    `src/rules/backend.rs` already allows this) and run something
    equivalent to `@typescript-eslint/switch-exhaustiveness-check`;
  - or ship a weaker heuristic: flag any `switch` on a discriminator
    that lacks a `default` branch *and* lacks an `assertNever(x)` /
    `satisfies never` fallthrough, regardless of whether the union is
    actually exhausted.
  The pragmatic path is the heuristic first (tree-sitter only), then a
  Tsc-backed strict variant if needed. Ship the heuristic under
  `category: "typescript"`.

### 4.2 `max-nested-conditionals` — limiter l'imbrication des `if`

- Status: DONE
- Existing coverage:
  - `nested-control-flow` — flags control-flow nesting deeper than 3
    (if / for / while / match / switch / try / loop / do). Resets at
    function boundaries. `else if` cascades handled correctly. Applies
    to TS, JS, TSX and Rust.
    See `src/rules/nested_control_flow/mod.rs`.
  - `max-depth` (delegated to oxlint, Error) — ESLint equivalent, threshold 2.
  - `cognitive-complexity` / `cyclomatic-complexity` — broader cousins.
- Note: if a stricter "only `if`, ignore loops/switches" variant is
  desired, `nested-control-flow` would have to gain a config toggle.
  No new rule needed.

---

## Summary

| # | Rule                         | Status   | Existing comply rule(s)                                         |
|---|------------------------------|----------|-----------------------------------------------------------------|
| 1 | no-let-var                   | PARTIAL  | `no-var`, `prefer-const` (both delegated)                       |
| 2 | no-param-reassign            | DONE     | `no-param-reassign` (delegated)                                 |
| 3 | no-mutation-methods          | PARTIAL  | `no-array-sort-mutation`, `no-array-reverse`, `no-array-delete` |
| 4 | no-property-mutation         | TODO     | —                                                               |
| 5 | no-reduce                    | DONE     | `no-array-reduce` (+ `reduce-initial-value`)                    |
| 6 | no-imperative-loops          | PARTIAL  | `no-for-loop`, `prefer-while`, `no-for-in-iterable`             |
| 7 | no-class-inheritance         | DONE     | `no-class-inheritance`                                          |
| 8 | no-this-mutation             | PARTIAL  | `no-this-assignment`, `ts-no-invalid-this`                      |
| 9 | require-exhaustive-switch    | TODO     | — (adjacent switch-hygiene rules only)                          |
| 10| max-nested-conditionals      | DONE     | `nested-control-flow` (+ delegated `max-depth`)                 |

### To implement (ranked by value/effort)

1. **`no-let-var`** — trivial AST check; biggest Hickey-style stance
   bump for the least code. Ship as `category: "functional"`,
   `Severity::Warning`.
2. **`no-imperative-loops` (while/do-while half)** — extend the
   existing `prefer-while` or add a sibling rule that flags any
   `while_statement` / `do_statement`. Trivial AST check.
3. **`no-this-mutation`** — medium effort, low FP once the constructor
   exception is implemented. Pairs naturally with `no-class-inheritance`
   under `category: "functional"`.
4. **`no-mutation-methods`** — medium effort; keep the receiver
   heuristic narrow to avoid Map/Set FPs.
5. **`require-exhaustive-switch`** — heuristic variant first (no
   `default` and no `assertNever` fallthrough on a `switch` with 2+
   cases). A proper implementation needs the `Tsc` backend.
6. **`no-property-mutation`** — ship last; highest FP surface, needs a
   robust exception list before it is usable on real TS codebases.

### Shared notes

- All new rules should be `category: "functional"` (new or existing)
  to make the Hickey stance opt-in via category filtering rather than
  forcing the whole codebase to adopt it.
- Tests: two per rule minimum (violation + pass), following
  `src/rules/test_helpers.rs` (`run_ts`, `run_tsx`, `run_rust`), as
  required by `CLAUDE.md`.
- `doc_url` is not required for rules original to comply (per the
  convention only imported rules need it).
