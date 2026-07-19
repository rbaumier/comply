# comply

> Your code will comply.

A fast, opinionated, multi-language linter that enforces architecture and
coding-standards rules across your whole repository — from a single binary.
It exists to keep code **clean, consistent, and correct** no matter who (or
**what**) wrote it.

> [!WARNING]
> **comply is in early alpha and very much a work in progress.** It ships ~1900 rules, many of them aggressively opinionated, and **you should expect a fair number of false positives.** Treat its output as suggestions to review, not gospel — and please [open an issue](https://github.com/rbaumier/comply/issues) when a rule fires where it shouldn't. Rule IDs, defaults, and behavior are all still subject to change.

## Why comply? — a quality guardrail for AI-generated code

AI coding agents — Claude Code, Cursor, Copilot, and friends — ship code
*fast*. But they optimize for **"it runs"**, not **"it's clean"**. Across a
real codebase that drift adds up: inconsistent naming, silent `any`, unwrapped
panics, floating promises, dead files, duplicated blocks, one-off abstractions.
The code compiles and the tests pass — and the repo slowly rots.

comply is the **deterministic guardrail** you run *after* the agent (or the
human): a reviewer that holds every file to the same architecture and
coding-standards bar, every time, with no opinions of the day. The result is a
codebase that reads like it was written by **one careful senior engineer**, not
stitched together from a dozen prompts.

It slots in wherever quality gets decided:

- **As an agent's self-review step** — point your agent at `comply --diff-only`
  so it only has to fix what it just touched, not the whole repo's backlog.
- **In a pre-commit hook** — nothing lands until it complies.
- **In CI** — `--diff-only` keeps the signal on the lines of the PR, so nobody
  drowns in pre-existing tech debt.

Fast, single-binary, no per-language config to wire up. Clean code, on autopilot.

## What it is

comply walks your project, runs **1893 rules across 169 categories**, and
reports violations in a familiar ESLint-style format. Rather than wiring up a
different linter per language and framework, it bundles everything into one tool:

- **In-process AST rules** powered by [tree-sitter](https://tree-sitter.github.io/) — no Node runtime required for most checks.
- **Delegation to best-in-class tools** when they're installed: [oxlint](https://oxc.rs/) for TypeScript/JS, [clippy](https://doc.rust-lang.org/clippy/) for Rust. If they're not on your `PATH`, comply degrades gracefully and runs its own rules only.
- **Framework awareness** — comply detects what you're using (Next.js, Nuxt, Elysia, Drizzle, TanStack, Zod…) and unlocks framework-specific rules automatically.

## Supported languages & frameworks

### Languages

comply's engine runs its rule set on these languages:

| Language | Extensions | What runs |
| --- | --- | --- |
| **TypeScript / JavaScript** | `.ts` `.tsx` `.js` `.jsx` `.mts` `.mjs` | Full in-process rule set + oxlint delegation + type-aware checks |
| **Rust** | `.rs` | In-process tree-sitter rules + clippy delegation |
| **Vue** | `.vue` | Single-File-Component template/script rules |
| **JSON** | `.json` | Config & i18n-translation rules |

Markdown (`.md`/`.mdx`) and HTML (`.html`) are **read for context** — comply
indexes their `import` / `<script src>` references so a component used only from
a docs page or an HTML entry point still counts as "used" — but no lint rules
target them.

### Frameworks

comply auto-detects these from your `package.json` and project files, then turns
on the matching framework-specific rules:

- **Web / meta-frameworks** — Next.js, Nuxt
- **Backend & APIs** — Elysia, Hono, Express
- **Data & validation** — Drizzle ORM, Zod
- **State & routing** — TanStack Query, TanStack Router, Vue Router, XState
- **UI** — shadcn/ui, React Email, React Native
- **Auth & errors** — Better Auth, better-result
- **Testing** — Jest, Playwright
- **Tooling & i18n** — Vite, Webpack, i18n

## Before / after

A few of the ~1900 rules, showing the kind of cleanup comply nudges you (or your
agent) toward. Run `comply explain <rule-id>` for the full rationale of any rule.

**`boolean-naming`** — a boolean's name should state its predicate.

```ts
// before
const active = user.subscription !== null;
if (active) grantAccess();

// after
const isActive = user.subscription !== null;
if (isActive) grantAccess();
```

**`no-floating-promise`** — a discarded promise swallows errors and loses ordering.

```ts
// before — the promise is dropped: a failure vanishes silently
function onSignup(user: User) {
  sendWelcomeEmail(user);
  track("signup");
}

// after
async function onSignup(user: User) {
  await sendWelcomeEmail(user);
  track("signup");
}
```

**`rust-no-unwrap`** — no `.unwrap()` / `.expect()` in production paths.

```rust
// before — panics at runtime if the file is missing
let config = fs::read_to_string("config.toml").unwrap();

// after — propagate the error to the caller with `?`
let config = fs::read_to_string("config.toml")?;
```

**`no-nested-ternary`** — nested ternaries are hard to scan; give each branch a name.

```ts
// before
const label =
  status === "ok" ? "Done" : status === "err" ? "Failed" : "Pending";

// after
function labelFor(status: Status) {
  if (status === "ok") return "Done";
  if (status === "err") return "Failed";
  return "Pending";
}
```

## Features

- **One binary, many languages** — TypeScript / JSX / TSX / JavaScript, Rust, Vue SFCs, and JSON, with a single install.
- **Framework-aware** — detects your stack and unlocks the matching rules automatically.
- **Git-aware scanning** — lint the whole tree, the working tree, staged files, a specific commit, or a commit range.
- **CI-friendly diff mode** — `--diff-only` reports only findings on lines you actually changed, so you don't drown in pre-existing tech debt.
- **Auto-fix** — `--fix` applies fixes for any rule whose backend supports them.
- **Interactive TUI** — browse and triage diagnostics in your terminal.
- **Editor integration** — built-in Language Server (LSP) for inline diagnostics as you type.
- **Inline suppressions with mandatory justification** — `// comply-ignore:` requires a reason, so silenced rules stay accountable.
- **Type-aware analysis (on by default)** — rules that query a real TypeScript checker for deeper correctness checks; pass `--no-type-aware` to skip them for an AstCheck-only run.

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

Lint the current directory:

```bash
comply
```

Lint a specific path:

```bash
comply ./src
```

That's it. comply exits `0` when clean, `1` when it finds violations, and `2` if it crashes.

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

comply reads an optional `comply.toml` from your project root, merged on top of its built-in defaults. Generate a fully-commented starting point with:

```bash
comply config init
```

Every tunable threshold lives under a `[rules.<id>]` table:

```toml
[rules.id-length]
min = 3

[rules.no-multi-op-oneliner]
min_ops = 10
min_line_length = 100
```

The goal is to keep this file short — if you find yourself reaching for lots of overrides, that's usually a signal a rule needs fixing rather than silencing. Issues welcome.

## Suppressing diagnostics

When a rule fires where it genuinely shouldn't, suppress it inline. A justification is **required**:

```ts
// comply-ignore: no-throw — rethrowing after cleanup, intentional
throw err;

const x = cheat(); // comply-ignore: rust-no-unwrap — value is a compile-time constant
```

Suppress a rule for an entire file with:

```ts
// comply-ignore-file: no-anonymous-default-export — this is a framework entry point
```

## Editor integration

comply ships a Language Server. Point your editor at `comply lsp` (stdio transport) to get diagnostics inline as you type. The LSP path skips the oxlint/clippy subprocesses for responsiveness and runs the in-process tree-sitter rules only.

## How it works

```
comply [path]
  ├─ 1. Parse CLI → scan mode (which files)
  ├─ 2. Discover files (filesystem walk or git diff)
  ├─ 3. Detect frameworks from package.json + project files
  ├─ 4. For each file, run matching rule backends:
  │       • tree-sitter  — in-process AST walk
  │       • text         — line/regex/filesystem checks
  │       • oxlint        — delegated TS/JS linting (if installed)
  │       • clippy        — delegated Rust linting (if installed)
  │       • type-aware    — TypeScript checker queries (opt-in)
  ├─ 5. Apply comply-ignore suppressions
  └─ 6. Format, print, exit 0/1/2
```

Rules are grouped into categories you can explore with `comply catalog`. Each rule has a stable ID, a severity, a one-line description, and a remediation message surfaced via `comply explain <rule-id>`.

## Status & feedback

This is alpha software under active development. Expect rough edges and false positives, and please report them — every issue helps tighten the rules. Bug reports, rule suggestions, and false-positive examples are all welcome at [github.com/rbaumier/comply/issues](https://github.com/rbaumier/comply/issues).
