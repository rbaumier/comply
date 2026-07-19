# comply

> Your code will comply.

**AI writes your code now, and it ships slop.**

You can improve the prompt, use skills, but it won't catch everything. A linter
will, on every run. I gathered every linter rule I could find and judged worth
keeping, **~1900 of them**, into one fast binary.

**Supported languages & frameworks:** TypeScript, JavaScript, Rust, Vue, CSS, SQL, JSON, YAML, Docker, TOML, React, Next.js, Nuxt, Node, Express, Hono, Elysia, Drizzle, Prisma, Zod, TanStack Query, TanStack Router, Vue Router, XState, shadcn/ui, Tailwind, React Email, React Native, Better Auth, better-result, Jest, Vitest, Playwright, Vite, Webpack, i18n, Kubernetes.

> [!WARNING]
> **comply is in early alpha and very much a work in progress.** Many rules are aggressively opinionated, and **you should expect a fair number of false positives.** Treat its output as suggestions to review, not gospel — and please [open an issue](https://github.com/rbaumier/comply/issues) when a rule fires where it shouldn't. Rule IDs, defaults, and behavior are all still subject to change.

## See it in action

An AI agent hands you this. It compiles. It "works". `comply --comply-only` flags **all 24 problems below** — one caret per finding, pointing at the exact column:

```ts
// TODO: handle partial refunds
// ▲
// └── todo-needs-issue-link
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
//             ▲
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
}
```

Here's the version comply signs off on (`comply: all clear`): validated input,
named constants, early guards, and a `Result` returned instead of a thrown
error, so failures come back to the caller as values.

```ts
import { ok, err } from "better-result";
import type { Result } from "better-result";

const FREE_SHIPPING_THRESHOLD = 100;
const EXPRESS_FEE = 5;
const STANDARD_FEE = 2;

export async function fulfillOrder(order: Order): Promise<Result<number, ReceiptError>> {
  const cart = cartSchema.parse(order.cart);
  if (!cart.lines.includes(order.id)) return ok(0);

  const receipt = await sendReceipt(order.id);
  if (receipt.isErr()) return err(receipt.error);

  return ok(order.isGift ? 0 : feeFor(cart) - order.discount);
}

function feeFor(cart: Cart): number {
  if (cart.total > FREE_SHIPPING_THRESHOLD) return EXPRESS_FEE;
  return cart.vip ? 0 : STANDARD_FEE;
}
```

Run `comply explain <rule-id>` for the full rationale behind any of these.

## How it works

- **In-process AST rules** powered by [tree-sitter](https://tree-sitter.github.io/) — no Node runtime required for most checks.
- **Delegation to best-in-class tools** when installed: [oxlint](https://oxc.rs/) for TypeScript/JS, [clippy](https://doc.rust-lang.org/clippy/) for Rust. Not on your `PATH`? comply degrades gracefully and runs its own rules only.
- **Framework-aware** — comply detects your stack from `package.json` and project files, then unlocks the matching rules automatically.

## Features

- **Git-aware scanning** — lint the whole tree, the working tree, staged files, a specific commit, or a commit range.
- **CI-friendly diff mode** — `--diff-only` reports only findings on lines you actually changed, so you don't drown in pre-existing tech debt.
- **Auto-fix** — `--fix` applies fixes for any rule whose backend supports them.
- **Interactive TUI** — browse and triage diagnostics in your terminal.
- **Editor integration** — built-in Language Server (LSP) for inline diagnostics as you type.
- **Inline suppressions with mandatory justification** — `// comply-ignore:` requires a reason, so silenced rules stay accountable.
- **Type-aware analysis** — rules that query a real TypeScript checker for deeper correctness checks; always on and requires a TypeScript toolchain (Node + `@typescript/native-preview`).

## Getting started

### Prerequisites

- A recent **Rust toolchain** (edition 2024) — install via [rustup](https://rustup.rs/).
- *(Optional, recommended)* **oxlint** for the full TypeScript/JS rule set:
  ```bash
  npm install -g oxlint
  ```
- *(Optional)* **clippy** for Rust delegation (`rustup component add clippy`).

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

comply exits `0` when clean, `1` when it finds violations, and `2` if it crashes.

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
| `--comply-only` | Run only the in-process tree-sitter rules (skip oxlint/clippy subprocesses) |
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

Keep this file short — if you find yourself reaching for lots of overrides, that's usually a signal a rule needs fixing rather than silencing. Issues welcome.

## Suppressing diagnostics

When a rule fires where it genuinely shouldn't, suppress it inline. A justification is **required**:

```ts
// comply-ignore: no-throw — rethrowing after cleanup, intentional
throw err;
```

Suppress a rule for an entire file with `// comply-ignore-file: <rule-id> — <reason>`.

## Editor integration

comply ships a Language Server. Point your editor at `comply lsp` (stdio transport) to get diagnostics inline as you type. The LSP path skips the oxlint/clippy subprocesses for responsiveness and runs the in-process tree-sitter rules only.

## Status & feedback

This is alpha software under active development. Expect rough edges and false positives, and please report them — every issue helps tighten the rules. Bug reports, rule suggestions, and false-positive examples are all welcome at [github.com/rbaumier/comply/issues](https://github.com/rbaumier/comply/issues).
