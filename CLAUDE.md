# comply

Rust-based multi-language linter with ~500 native rules covering TypeScript, JavaScript, TSX, Rust, and Vue.

## Commands

```bash
cargo nextest run              # Run tests (2940 tests, ~4s)
cargo clippy --all --all-targets -- -D warnings  # Lint (0 warnings policy)
cargo build --release          # Release build
./target/release/comply src/   # Run comply on its own source
```

Always use `cargo nextest run` over `cargo test` — parallel execution, better output, per-test timeouts.

## Architecture

```
src/
  main.rs           # CLI entry point
  engine.rs         # File → parse → dispatch rules → collect diagnostics
  project/
    import_index.rs # Cross-file import/export index (TS/JS/Rust)
  rules/
    mod.rs          # Rule registry: all_rule_defs() + pub mod declarations
    backend.rs      # Backend enum (TreeSitter/Text/Oxlint/Clippy/Tsc)
    meta.rs         # RuleMeta struct (id, description, remediation, severity, doc_url)
    registry.rs     # Macros: register_ts_family!, ast_check!, etc.
    walker.rs       # Iterative tree-sitter AST walker
    test_helpers.rs # run_ts(), run_tsx(), run_rust() test fixtures
    jsx.rs          # Shared JSX attribute helpers
    rust_helpers.rs # Shared Rust AST + regex extraction helpers
    vue_template_helpers.rs  # Shared Vue <template> HTML parser
    delegated/      # Oxlint-delegated rules (eslint, ts, import, unicorn, promise, oxc)
    {rule_name}/    # One directory per rule
      mod.rs        # RuleMeta + register()
      typescript.rs # AstCheck backend (tree-sitter TS/JS/TSX)
      rust.rs       # AstCheck backend (tree-sitter Rust) — optional
      text.rs       # TextCheck backend (line scanning) — for text-only rules or Vue
```

## Cross-file Analysis

comply supporte l'analyse cross-file via `ImportIndex`:
- Indexe exports/imports pour TS/JS/TSX et Rust
- APIs: `get_exports()`, `get_imports()`, `get_usages()`, `get_call_sites()`
- Utilisé par: `no-identical-functions`, `inconsistent-function-call`, `god-module`, `dead-export`

## Adding a rule

### AstCheck (tree-sitter) — preferred for code structure
```rust
// src/rules/{snake_case}/mod.rs
mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rule-id",
    description: "One-line summary.",
    remediation: "Actionable fix the user can follow.",
    severity: Severity::Warning,
    doc_url: Some("https://..."),  // Link to original rule if imported
    categories: &["category"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
    // or: crate::register_ts_family_with_rust!(META, typescript, rust)
}

// src/rules/{snake_case}/typescript.rs
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    // detection logic...
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "rule-id".into(),
        message: "...".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }
    #[test]
    fn flags_violation() { assert_eq!(run_on("bad code").len(), 1); }
    #[test]
    fn allows_correct() { assert!(run_on("good code").is_empty()); }
}
```

### TextCheck — only for text-inherent rules (comments, regex, SQL, secrets, filenames)
```rust
// src/rules/{snake_case}/text.rs
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;
impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> { /* line scanning */ }
}
```

### Registration
1. Add `pub mod {rule_name};` in `src/rules/mod.rs`
2. Add `{rule_name}::register()` in `all_rule_defs()`

## Conventions

- Rule IDs: kebab-case (`no-array-reduce`). Directories: snake_case (`no_array_reduce/`).
- Categories: `&["unicorn"]`, `&["typescript"]`, `&["react"]`, `&["security"]`, `&["node"]`, `&["imports"]`, `&["testing"]`, `&["jsdoc"]`, `&["regex"]`, `&["code-quality"]`, etc.
- Imported rules must have `doc_url: Some("...")` pointing to the original documentation.
- Every rule needs 2+ tests (violation + pass). Use `run_ts`, `run_tsx`, or `run_rust` from `test_helpers`.
- When a rule applies to multiple languages, tests should cover each backend.
- Clippy must pass with `-D warnings` before committing. No `#[allow]` without justification.
