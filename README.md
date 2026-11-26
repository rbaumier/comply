# comply

> Your code will comply.

**AI writes your code now, and it ships slop.**

You can improve the prompt, use skills, but it won't catch everything. A linter
will, on every run. I gathered every linter rule I could find and judged worth
keeping, **~2000 of them**, into one fast binary.

**Supported languages & frameworks:** TypeScript, JavaScript, Rust, Vue, CSS, SQL, JSON, YAML, Docker, TOML, React, Next.js, Nuxt, Node, Express, Hono, Elysia, Drizzle, Prisma, Zod, TanStack Query, TanStack Router, React Router, Vue Router, XState, shadcn/ui, Tailwind, React Email, React Native, Better Auth, better-result, Jest, Vitest, Playwright, Vite, Webpack, i18n, Kubernetes.

> [!WARNING]
> **comply is in early alpha and very much a work in progress.** Many rules are aggressively opinionated, and **you should expect a fair number of false positives.** Treat its output as suggestions to review, not gospel — and please [open an issue](https://github.com/rbaumier/comply/issues) when a rule fires where it shouldn't. Rule IDs, defaults, and behavior are all still subject to change.

## See it in action

```ts
// TODO: handle partial refunds
// ▲
// └── todo-needs-issue-link
/** Charges the order and emails the receipt. */
// ▲
// └── jsdoc-missing-example
export async function processOrder(id, items, discount, isGift = false, retry = false) {
//     ▲              ▲                ▲
//     │              │                └── no-generic-names -> 'items'
//     │              └── no-generic-names -> 'process...'
//     ├── max-params (5 params, max 4)
//     └── no-async-without-await
  let done = false;
//▲   ▲
//│   └── boolean-naming -> isDone
//└── no-let
  const cart = JSON.parse(items) as Cart;
//             ▲          ▲
//             │          └── no-unsafe-argument
//             ├── no-type-assertion
//             ├── no-json-parse-cast
//             ├── no-unchecked-json-parse
//             ├── try-catch-json-parse
//             └── ts-no-as-narrowing
  if (cart.lines.indexOf(id) === -1) return 0;
//    ▲
//    └── no-indexof-equality
  let fee = cart.total > 100 ? 5 : cart.vip ? 0 : 2;
//▲                      ▲     ▲   ▲
//│                      │     │   └── no-nested-ternary
//│                      │     └── no-magic-numbers -> 5
//│                      └── no-magic-numbers -> 100
//└── no-let
  if (!isGift) {
//▲   ▲
//│   └── no-negated-condition
//└── prefer-ternary
    fee = fee - discount;
//  ▲
//  └── operator-assignment
  } else {
    fee = 0;
  }
  try {
//▲
//└── no-try-statements
    sendReceipt(id);
    done = true;
  } catch (e) {
//         ▲
//         ├── catch-error-name -> error
//         └── ts-no-implicit-any-catch
    throw new Error(e.message);
//        ▲
//        ├── error-without-cause
//        └── exception-use-error-cause
  }
  return done == true ? fee : 0;
//       ▲    ▲
//       │    └── eqeqeq
//       ├── no-unnecessary-boolean-literal-compare
//       └── no-unnecessary-condition
}
```

Here's the version comply signs off on (`comply: all clear`): the untrusted
order validated by a schema, named constants, early guards, and a `Result`
returned instead of a thrown error, so failures come back to the caller as values.

```ts
import { ok, err } from "better-result";
import type { Result } from "better-result";

const FREE_SHIPPING_THRESHOLD = 100;
const STANDARD_SHIPPING_FEE = 5;

/**
 * Validates a raw order, computes the shipping fee, and emits a receipt.
 *
 * @example
 *   const result = await fulfillOrder(req.body);
 *   // => ok({ receipt, shippingFee })
 */
export async function fulfillOrder(rawOrder: unknown): Promise<Result<Fulfillment, FulfillmentError>> {
  const parsed = orderSchema.safeParse(rawOrder);
  if (!parsed.success) {
    return err(new InvalidOrderError(parsed.error));
  }
  const order = parsed.data;

  const shippingFee = shippingFeeFor(order.cart);
  const receipt = await sendReceipt(order.id, shippingFee);
  if (receipt.isErr()) {
    return err(receipt.error);
  }

  return ok({ receipt: receipt.value, shippingFee });
}

function shippingFeeFor(cart: Cart): number {
  if (cart.total >= FREE_SHIPPING_THRESHOLD) {
    return 0;
  }
  return STANDARD_SHIPPING_FEE;
}
```

Run `comply explain <rule-id>` for the full rationale behind any of these, or browse every rule in [`docs/rules.md`](docs/rules.md).

## How it works

- **In-process AST rules** powered by [tree-sitter](https://tree-sitter.github.io/) — no Node runtime required for most checks.
- **Delegation to best-in-class tools**: [oxlint](https://oxc.rs/) for TypeScript/JS, [clippy](https://doc.rust-lang.org/clippy/) plus [cargo-shear](https://github.com/Boshen/cargo-shear) for Rust. These back part of the rule set and are **not optional** — if comply finds code in a language whose linter isn't on your `PATH`, it exits with an error rather than silently under-reporting.
- **Framework-aware** — comply detects your stack from `package.json` and project files, then unlocks the matching rules automatically.

## Features

- **Git-aware scanning** — lint the whole tree, the working tree, staged files, a specific commit, or a commit range.
- **CI-friendly diff mode** — `--diff-only` reports only findings on lines you actually changed, so you don't drown in pre-existing tech debt.
- **Auto-fix** — `--fix` applies fixes for any rule whose backend supports them.
- **Interactive TUI** — browse and triage diagnostics in your terminal.
- **Editor integration** — built-in Language Server (LSP) for inline diagnostics as you type.
- **Inline suppressions with mandatory justification** — `// comply-ignore:` requires a reason, so silenced rules stay accountable. Your project's own `eslint-disable-line` directives are honored too.
- **Type-aware analysis** — rules that query a real TypeScript checker for deeper correctness checks; always on and requires a TypeScript toolchain (Node + `typescript` >= 7, `@typescript/native` or `@typescript/native-preview`). The checker is taken from the project, or failing that from a machine-wide install (`npm root -g`, a volta image, the tree a `tsgo` on your `PATH` lives in); set `COMPLY_TYPESCRIPT_GO_ROOTS` to the `node_modules` directories to use instead, or to an empty value to keep resolution strictly project-local.

## Getting started

### Prerequisites

- A recent **Rust toolchain** (edition 2024) — install via [rustup](https://rustup.rs/).
- The delegated linters comply depends on. **oxlint** is required to lint TypeScript/JS; **clippy** and **cargo-shear** are required to lint Rust. comply errors out if it needs one that isn't on your `PATH`. Install them all at once:
  ```bash
  # TypeScript/JS
  npm install -g oxlint oxlint-tsgolint

  # Rust
  rustup component add clippy
  cargo install cargo-shear
  ```

### Build

```bash
git clone https://github.com/rbaumier/comply.git
cd comply
cargo build --release
# binary at ./target/release/comply
```

Or install it onto your `PATH`:

```bash
cargo install --path .
```

### Run

```bash
comply            # lint the current directory
comply ./src      # lint a specific path
```

comply exits `0` when clean and `1` when it finds violations.
`2` means it could not judge: a required tool is missing or unusable, a git scope left a file unanalyzed, or comply crashed.
The message names the cause and its remedy.

## Usage

### Scan modes

```bash
comply --working-tree         # only files modified in the working tree
comply --staged               # only staged files
comply --last-commit          # files changed in the last commit
comply --commit <sha>         # files changed in a specific commit
comply --range <from> <to>    # files changed between two commits
comply --working-tree --diff-only   # restrict findings to changed lines only
```

### Common flags

| Flag | Description |
| --- | --- |
| `--fix` | Apply auto-fixes where supported |
| `--json` | Emit diagnostics as JSON (for editors and CI) |
| `--tui` | Launch the interactive terminal UI |
| `--timings` | Print a per-phase timing breakdown to stderr |

### Subcommands

```bash
comply list                  # list every registered rule
comply explain <rule-id>     # full description + remediation for one rule
comply catalog               # generate a markdown catalog grouped by category
comply rules "id-a,id-b"     # run only the named rules
comply config init           # write a comply.toml seeded with all defaults
comply config print          # print the default config to stdout
comply lsp                   # run as a Language Server on stdio
```

## Configuration

comply reads an optional `comply.toml` from your project root, merged on top of its built-in defaults. Generate a fully-commented starting point with `comply config init`. Every tunable threshold lives under a `[rules.<id>]` table:

```toml
[rules.id-length]
min = 3

[rules.max-params]
max = 4
```

comply.toml can also hold clippy's configuration. Any `clippy.toml` key goes under `[clippy]`, copied as-is into the file comply hands to clippy:

```toml
[clippy]
disallowed-methods = [{ path = "reqwest::get", reason = "use the shared Http client" }]
```

Without a `[clippy]` table (or without any comply.toml), comply reads the project's own clippy file, the one plain `cargo clippy` finds first going up from the workspace root (`.clippy.toml` or `clippy.toml`), and copies its keys as-is in place of `[clippy]`. With a `[clippy]` table, even an empty one, comply.toml is the single source: such a file is an error, comply names it, lists its keys to move under `[clippy]`, and does not run clippy. comply hands one clippy configuration to the whole workspace, so a file plain clippy would read but comply cannot is an error naming it: `.clippy.toml` and `clippy.toml` side by side, or a clippy file in a member's directory below the workspace root. A key set both as a `[rules."clippy::…"]` threshold and in `[clippy]` or the project file is an error naming the key. A key clippy rejects (unknown, wrong type) is an error naming the line of the file it was written in. Clippy lints every target, tests included. A crate that denies a clippy lint does not hide its findings: comply runs clippy once more with every denied lint as a warning, and reports them all. Two known limits: a `CLIPPY_CONF_DIR` set in `.cargo/config.toml` is not read, and a clippy file that starts with a byte order mark is an error.

Keep this file short — if you find yourself reaching for lots of overrides, that's usually a signal a rule needs fixing rather than silencing. Issues welcome.

## Suppressing diagnostics

When a rule fires where it genuinely shouldn't, suppress it inline. A justification is **required**:

```ts
// comply-ignore: no-throw — rethrowing after cleanup, intentional
throw err;
```

Suppress a rule for an entire file with `// comply-ignore-file: <rule-id> — <reason>`.

In a language without `//` comments, write the marker in the language's own comment: `-- comply-ignore:` in SQL, `# comply-ignore:` in YAML, TOML, Dockerfiles and nginx configs, `/* comply-ignore: … */` in CSS, `<!-- comply-ignore: … -->` in a Vue template. A marker in a string, a template literal, a YAML block scalar or a heredoc is text. So is one after a Dockerfile instruction, in an unquoted CSS `url(…)`, in an nginx word or Lua block, in a Vue tag, `{{ … }}`, `<textarea>` or Pug template, in the data of a SQL `COPY … FROM stdin`, and in a function body in another language than SQL or PL/pgSQL. `//` counts where it opens a comment, and anywhere in JSON, Markdown and HTML.

The SQL rules read the statements of a `.sql` file, never its comments: a statement commented out is not reported, and a word in a comment does not change the statement next to it. A rule that asks for a comment (a rationale, a deprecation note) still finds it there. A known limit: comply may read a SQL comment as text, and its marker then suppresses nothing. That happens below a line it cannot read for sure, such as `--l'essai`, `'C:\'` or `$$it's$$`, and in the body of a `DO LANGUAGE plpgsql`.

comply also honors the inline ESLint directives your project already wrote, for the comply rule that re-implements the named ESLint rule: `// eslint-disable-line <rule>` and `// eslint-disable-next-line <rule>`, in either comment syntax. The plugin prefix does not matter — `@typescript-eslint/no-explicit-any`, `typescript/no-explicit-any` and `no-explicit-any` all reach `ts-no-explicit-any`. A directive naming no rule suppresses nothing, and the file-scope `eslint-disable` / `eslint-enable` range form is not honored — comply tracks no enable/disable range.

## Editor integration

comply ships a Language Server. Point your editor at `comply lsp` (stdio transport) to get diagnostics inline as you type. The LSP path skips the oxlint/clippy subprocesses for responsiveness and runs the in-process tree-sitter rules only.

## Status & feedback

This is alpha software under active development. Expect rough edges and false positives, and please report them — every issue helps tighten the rules. Bug reports, rule suggestions, and false-positive examples are all welcome at [github.com/rbaumier/comply/issues](https://github.com/rbaumier/comply/issues).
