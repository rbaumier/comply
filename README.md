# comply

> Your code will comply.

**One fast binary that keeps your whole repo clean — no matter who, or what, wrote it.**

AI agents ship code that compiles, passes tests, and quietly rots your codebase.
comply is the deterministic guardrail that catches the mess: **~1900 opinionated
rules** across your whole tree, run in one command — as an agent's self-review
step (`--diff-only`), a pre-commit hook, or in CI.

> [!WARNING]
> **comply is in early alpha and very much a work in progress.** Many rules are aggressively opinionated, and **you should expect a fair number of false positives.** Treat its output as suggestions to review, not gospel — and please [open an issue](https://github.com/rbaumier/comply/issues) when a rule fires where it shouldn't. Rule IDs, defaults, and behavior are all still subject to change.

## See it in action

An AI agent hands you this. It compiles. It "works". comply flags **16 problems in 19 lines**:

```ts
// TODO: handle partial refunds                                       // ✗ todo-needs-issue-link
async function processOrder(id, items, discount, isGift = false, retry = false) { // ✗ max-params · id-length (`id`) · no-boolean-flag-param · ts-no-unused-vars (items, retry)
  let done = false;                                                   // ✗ boolean-naming → isDone
  const cart = JSON.parse(localStorage.getItem("cart")) as Cart;      // ✗ no-type-assertion · no-unchecked-json-parse
  if (cart.lines.indexOf(id) === -1) return 0;                        // ✗ no-indexof-equality
  let fee = cart.total > 100 ? 5 : cart.vip ? 0 : 2;                  // ✗ no-nested-ternary · no-magic-numbers
  if (!isGift) {                                                       // ✗ no-negated-condition
    fee = fee - discount;
  } else {
    fee = 0;
  }
  try {
    sendReceipt(id);                                                  // ✗ no-floating-promise (result discarded)
    done = true;
  } catch (e) {                                                        // ✗ catch-error-name (e → error)
    throw new Error(e.message);                                       // ✗ error-without-cause
  }
  return done == true ? fee : 0;                                      // ✗ no-redundant-boolean (`== true`)
}
```

Here's what comply wants instead — validated input, named constants, early
guards, propagated errors, extracted branches:

```ts
const EXPRESS_FEE = 5;
const STANDARD_FEE = 2;

async function processOrder(order: Order): Promise<number> {
  const cart = cartSchema.parse(JSON.parse(order.cartJson));
  if (!cart.lines.includes(order.id)) return 0;

  try {
    await sendReceipt(order.id);
  } catch (error) {
    throw new Error("failed to send receipt", { cause: error });
  }

  return order.isGift ? 0 : feeFor(cart) - order.discount;
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

## Supported languages & frameworks

comply's engine lints these languages:

| Language | Extensions | What runs |
| --- | --- | --- |
| **TypeScript / JavaScript** | `.ts` `.tsx` `.js` `.jsx` `.mts` `.mjs` | Full in-process rule set + oxlint delegation + type-aware checks |
| **Rust** | `.rs` | In-process tree-sitter rules + clippy delegation |
| **Vue** | `.vue` | Single-File-Component template/script rules |
| **JSON** | `.json` | Config & i18n-translation rules |

Markdown and HTML are read only for their `import` / `<script src>` references, so
a component used exclusively from a docs page or an HTML entry point still counts
as "used" — no rules target them directly.

**Auto-detected frameworks:** Next.js · Nuxt · Elysia · Hono · Express · Drizzle ORM · Zod · TanStack Query · TanStack Router · Vue Router · XState · shadcn/ui · React Email · React Native · Better Auth · better-result · Jest · Playwright · Vite · Webpack · i18n.

## Features

- **Git-aware scanning** — lint the whole tree, the working tree, staged files, a specific commit, or a commit range.
- **CI-friendly diff mode** — `--diff-only` reports only findings on lines you actually changed, so you don't drown in pre-existing tech debt.
- **Auto-fix** — `--fix` applies fixes for any rule whose backend supports them.
- **Interactive TUI** — browse and triage diagnostics in your terminal.
- **Editor integration** — built-in Language Server (LSP) for inline diagnostics as you type.
- **Inline suppressions with mandatory justification** — `// comply-ignore:` requires a reason, so silenced rules stay accountable.
- **Type-aware analysis (on by default)** — rules that query a real TypeScript checker for deeper correctness checks; pass `--no-type-aware` to skip them.

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
| `--no-type-aware` | Skip type-aware rules (on by default; they're slower and need a TypeScript checker) |
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
