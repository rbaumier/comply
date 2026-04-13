# Miette Pretty Diagnostics Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Render human-facing comply diagnostics with miette-powered source frames, rule help, and doc URLs — while preserving the current eslint-like format for piped/CI usage.

**Architecture:** Miette lives at the output layer only. `Diagnostic` stays a POD struct — we add an optional `span` field so native (tree-sitter) rules can attach byte ranges at detection time. At render time we group diagnostics by file, read each source once, look up `RuleMeta` via a boot-time registry for help/url, wrap each diagnostic in a throwaway `struct MietteDiag` that implements `miette::Diagnostic`, and pipe them through `GraphicalReportHandler`. Delegated diagnostics (oxlint/clippy/knip/madge) with no span fall back to whole-line highlighting. TTY auto-detection selects pretty vs eslint format.

**Tech Stack:** `miette 7` (renderer), `supports-color` (TTY detection — already transitive via miette), existing tree-sitter node API (`node.byte_range()` for spans), existing `RuleMeta` registry.

**Scope exclusions (confirmed YAGNI):** no related/secondary labels, no custom miette theme, no miette JSON output, no oxlint/clippy span remapping, no message/remediation deduplication.

---

## Phase 1 — Foundation: Diagnostic span + codemod + helper

### Task 1.1: Add miette dependency

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add dep**

Add under `[dependencies]`:
```toml
miette = { version = "7", features = ["fancy"] }
```

**Step 2: Verify build**

Run: `cargo build`
Expected: compiles without errors (no miette usage yet).

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add miette dependency"
```

---

### Task 1.2: Add `span` field to `Diagnostic`

**Files:**
- Modify: `src/diagnostic.rs`

**Step 1: Write the failing test first**

Append to `src/diagnostic.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_serializes_span_when_present() {
        let d = Diagnostic {
            path: std::path::PathBuf::from("f.rs"),
            line: 1,
            column: 1,
            rule_id: "r".into(),
            message: "m".into(),
            severity: Severity::Warning,
            span: Some((10, 5)),
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"span\""));
    }

    #[test]
    fn diagnostic_omits_span_when_absent() {
        let d = Diagnostic {
            path: std::path::PathBuf::from("f.rs"),
            line: 1,
            column: 1,
            rule_id: "r".into(),
            message: "m".into(),
            severity: Severity::Warning,
            span: None,
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("\"span\""));
    }
}
```

**Step 2: Run test, expect compile failure**

Run: `cargo nextest run -p comply --test-threads 8 diagnostic`
Expected: FAIL — `Diagnostic` has no field `span`.

**Step 3: Add the field**

Edit the struct in `src/diagnostic.rs`:
```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub rule_id: String,
    pub message: String,
    pub severity: Severity,
    /// Byte range into the source file, `(offset, length)`. Populated by
    /// native tree-sitter rules that have the node in scope. `None` for
    /// delegated diagnostics (oxlint/clippy/knip/madge) — the renderer
    /// falls back to whole-line highlighting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<(usize, usize)>,
}
```

**Step 4: Compile — expect 500+ errors**

Run: `cargo build 2>&1 | head -30`
Expected: many `missing field \`span\` in initializer of \`Diagnostic\`` errors.

This is intentional — the codemod in Task 1.3 fixes them all mechanically.

**Step 5: Do NOT commit yet** — the build is broken. Move to Task 1.3.

---

### Task 1.3: Codemod existing literals to add `span: None,`

**Files:**
- Modify: every file containing a `Diagnostic { ... }` struct literal (~150 files).

**Step 1: Run the codemod**

Use a Perl in-place edit that inserts `span: None,` right before the closing `}` of every `Diagnostic { ... }` literal. The pattern is: find `severity:` line in a Diagnostic literal, append a `span: None,` line after it (preserving indentation).

Run (from repo root):
```bash
find src -name '*.rs' -print0 | xargs -0 perl -i -pe '
  s/(severity:\s*Severity::(?:Error|Warning),\n)(\s*\})/$1$2/g;
  s/(severity:\s*Severity::(?:Error|Warning),)(\n)(\s*\})/$1\n$` =~ m{(\s*)severity} ? $1 : "            " . "span: None,$2$3"/ge if 0;
'
```

*(The regex above is illustrative — the executor should write a small Rust or Bun codemod script instead: parse each file, find `Diagnostic {` openings, balance braces, insert `span: None,` on the line before the matching `}`. Commit the script under `scripts/codemod-add-diag-span.ts` so it's reproducible.)*

**Preferred concrete approach — Bun codemod:**

Create `scripts/codemod-add-diag-span.ts`:
```ts
#!/usr/bin/env bun
import { $ } from "bun";
import { readFileSync, writeFileSync } from "node:fs";

const files = (await $`grep -rl "Diagnostic {" src --include="*.rs"`.text())
  .trim().split("\n").filter(Boolean);

let edited = 0;
for (const f of files) {
  const src = readFileSync(f, "utf8");
  // Match a Diagnostic { ... } struct literal, insert span: None before closing brace.
  // Use a state machine, not regex, to handle nested braces inside messages.
  const out: string[] = [];
  let i = 0;
  while (i < src.length) {
    const start = src.indexOf("Diagnostic {", i);
    if (start < 0) { out.push(src.slice(i)); break; }
    out.push(src.slice(i, start + "Diagnostic {".length));
    let depth = 1;
    let j = start + "Diagnostic {".length;
    while (j < src.length && depth > 0) {
      const c = src[j];
      if (c === "{") depth++;
      else if (c === "}") depth--;
      if (depth === 0) break;
      j++;
    }
    const body = src.slice(start + "Diagnostic {".length, j);
    // Skip if span already present
    if (/\bspan\s*:/.test(body)) {
      out.push(body);
      out.push(src[j]);
      i = j + 1;
      continue;
    }
    // Detect indentation from last line of body
    const lastNewline = body.lastIndexOf("\n");
    const lastLine = lastNewline >= 0 ? body.slice(lastNewline + 1) : "";
    const indent = lastLine.match(/^\s*/)?.[0] ?? "            ";
    const insert = `\n${indent}span: None,`;
    // Insert before trailing whitespace of body
    const trimmed = body.replace(/\s*$/, "");
    out.push(trimmed);
    out.push(",".endsWith(trimmed.slice(-1)) ? "" : ",");
    out.push(insert);
    out.push(body.slice(trimmed.length));
    out.push(src[j]);
    i = j + 1;
    edited++;
  }
  writeFileSync(f, out.join(""));
}
console.log(`Edited ${edited} literals across ${files.length} files`);
```

Run: `bun scripts/codemod-add-diag-span.ts`
Expected: "Edited N literals across M files" where N ≥ 500.

**Step 2: Format**

Run: `cargo fmt`

**Step 3: Verify compile**

Run: `cargo build`
Expected: zero errors. If any remain, it's a literal the codemod missed — fix manually.

**Step 4: Run full test suite**

Run: `cargo nextest run`
Expected: 2940 tests pass (same count as before).

**Step 5: Clippy clean**

Run: `cargo clippy --all --all-targets -- -D warnings`
Expected: zero warnings.

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor(diagnostic): add optional span field, codemod existing literals to None"
```

---

### Task 1.4: Add `Diagnostic::at_node` helper

**Files:**
- Modify: `src/diagnostic.rs`

**Step 1: Write failing test**

In `src/diagnostic.rs` tests module:
```rust
#[test]
fn at_node_captures_byte_range() {
    // Use a real tree-sitter parser — the helper is meant to be called
    // with a real node, and fake byte ranges would defeat the point.
    let source = "const x = 1;";
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let child = root.child(0).unwrap(); // lexical_declaration
    let d = Diagnostic::at_node(
        std::path::Path::new("x.ts"),
        &child,
        "rule-id",
        "msg".into(),
        Severity::Warning,
    );
    assert_eq!(d.span, Some((0, child.byte_range().len())));
    assert_eq!(d.line, 1);
    assert_eq!(d.column, 1);
}
```

**Step 2: Run, expect compile fail**

Run: `cargo nextest run at_node_captures_byte_range`
Expected: FAIL — `Diagnostic::at_node` does not exist.

**Step 3: Add the constructor**

In `src/diagnostic.rs`:
```rust
impl Diagnostic {
    /// Build a diagnostic anchored on a tree-sitter node. Captures both the
    /// human-friendly `(line, column)` and the byte `span` from `node.byte_range()`
    /// so the renderer can highlight the exact source range.
    ///
    /// Native rules should prefer this over constructing `Diagnostic` literals —
    /// delegated diagnostics (oxlint/clippy) that only have `(line, col)` stay on
    /// the literal path with `span: None`.
    pub fn at_node(
        path: &std::path::Path,
        node: &tree_sitter::Node<'_>,
        rule_id: &str,
        message: String,
        severity: Severity,
    ) -> Self {
        let pos = node.start_position();
        let range = node.byte_range();
        Self {
            path: path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: rule_id.into(),
            message,
            severity,
            span: Some((range.start, range.len())),
        }
    }
}
```

**Step 4: Run test**

Run: `cargo nextest run at_node_captures_byte_range`
Expected: PASS.

**Step 5: Clippy clean**

Run: `cargo clippy --all --all-targets -- -D warnings`

**Step 6: Commit**

```bash
git add src/diagnostic.rs
git commit -m "feat(diagnostic): add Diagnostic::at_node helper for span-aware native rules"
```

---

## Phase 2 — Renderer

### Task 2.1: RuleMeta registry

**Files:**
- Create: `src/rules/meta_registry.rs`
- Modify: `src/rules/mod.rs` (add `pub mod meta_registry;`)

**Step 1: Write failing test**

Create `src/rules/meta_registry.rs`:
```rust
//! Boot-time lookup table: rule_id → &'static RuleMeta. Built once from
//! `all_rule_defs()`. The renderer uses this to surface RuleMeta-only fields
//! (description, remediation, doc_url) that aren't present in `Diagnostic`.

use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;
use std::collections::HashMap;
use std::sync::OnceLock;

static REGISTRY: OnceLock<HashMap<&'static str, &'static RuleMeta>> = OnceLock::new();

pub fn registry() -> &'static HashMap<&'static str, &'static RuleMeta> {
    REGISTRY.get_or_init(|| {
        crate::rules::all_rule_defs()
            .iter()
            .map(|r: &RuleDef| (r.meta.id, &r.meta))
            .collect()
    })
}

pub fn lookup(rule_id: &str) -> Option<&'static RuleMeta> {
    registry().get(rule_id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_finds_registered_rule() {
        let meta = lookup("no-weak-cipher").expect("rule should be registered");
        assert_eq!(meta.id, "no-weak-cipher");
    }

    #[test]
    fn lookup_returns_none_for_unknown() {
        assert!(lookup("not-a-real-rule-id-zzz").is_none());
    }
}
```

**Step 2: Register module**

In `src/rules/mod.rs`, add near the top of the `pub mod` block:
```rust
pub mod meta_registry;
```

**Step 3: Run tests**

Run: `cargo nextest run meta_registry`
Expected: PASS (both tests).

**Note:** If `RuleDef.meta` is not `pub` or is not `&'static RuleMeta` but `RuleMeta` by value, adapt: the registry keys off `&'static str` (rule id) and stores the `RuleMeta` by value (`Copy`) — `RuleMeta` is already `Copy` per the source. Adjust the HashMap to `HashMap<&'static str, RuleMeta>` if needed.

**Step 4: Clippy clean**

Run: `cargo clippy --all --all-targets -- -D warnings`

**Step 5: Commit**

```bash
git add src/rules/meta_registry.rs src/rules/mod.rs
git commit -m "feat(rules): add boot-time RuleMeta lookup registry"
```

---

### Task 2.2: `resolve_span_from_line_col` helper

**Files:**
- Create: `src/output/span_resolver.rs`
- Modify: `src/output.rs` → become `src/output/mod.rs` (or add `pub mod span_resolver;` in existing output.rs; simpler to keep output.rs and add a sibling module via `mod output { ... }`). **Decision:** convert `src/output.rs` into `src/output/mod.rs` and add `span_resolver.rs` as a sibling.

**Step 1: Restructure output module**

```bash
mkdir src/output
git mv src/output.rs src/output/mod.rs
```

Add to the top of `src/output/mod.rs`:
```rust
mod span_resolver;
pub use span_resolver::resolve_line_span;
```

**Step 2: Write failing tests**

Create `src/output/span_resolver.rs`:
```rust
//! Resolve a `(line, column)` pair to a byte `(offset, length)` pair suitable
//! for miette's labeled spans. Used as the fallback for diagnostics without a
//! pre-captured span — primarily delegated diagnostics (oxlint/clippy/knip/madge).
//!
//! The returned length covers the *rest of the line* from the reported column,
//! matching whole-line highlighting behavior.

/// Returns `Some((offset, length))` or `None` if the line doesn't exist in `source`.
/// Handles LF and CRLF line endings. Columns are 1-based (as in diagnostics).
pub fn resolve_line_span(source: &str, line: usize, column: usize) -> Option<(usize, usize)> {
    if line == 0 { return None; }
    let mut offset = 0usize;
    let mut current_line = 1usize;
    let bytes = source.as_bytes();
    while current_line < line {
        let nl = bytes[offset..].iter().position(|&b| b == b'\n')?;
        offset += nl + 1;
        current_line += 1;
    }
    // Now `offset` is the start of the target line.
    let line_end = bytes[offset..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|p| offset + p)
        .unwrap_or(bytes.len());
    // Strip trailing \r for CRLF.
    let line_end = if line_end > offset && bytes[line_end - 1] == b'\r' {
        line_end - 1
    } else {
        line_end
    };
    let col_offset = column.saturating_sub(1);
    let start = (offset + col_offset).min(line_end);
    Some((start, line_end - start))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_line_col_1() {
        let s = "abc\ndef\n";
        assert_eq!(resolve_line_span(s, 1, 1), Some((0, 3)));
    }

    #[test]
    fn second_line_col_2() {
        let s = "abc\ndef\n";
        assert_eq!(resolve_line_span(s, 2, 2), Some((5, 2)));
    }

    #[test]
    fn crlf_strips_carriage_return() {
        let s = "abc\r\ndef\r\n";
        assert_eq!(resolve_line_span(s, 1, 1), Some((0, 3)));
        assert_eq!(resolve_line_span(s, 2, 1), Some((5, 3)));
    }

    #[test]
    fn line_out_of_range_returns_none() {
        let s = "one\ntwo\n";
        assert!(resolve_line_span(s, 99, 1).is_none());
    }

    #[test]
    fn column_past_end_clamps_to_line_end() {
        let s = "short\n";
        assert_eq!(resolve_line_span(s, 1, 100), Some((5, 0)));
    }

    #[test]
    fn line_zero_returns_none() {
        assert!(resolve_line_span("anything", 0, 1).is_none());
    }
}
```

**Step 3: Run tests**

Run: `cargo nextest run span_resolver`
Expected: PASS (6 tests).

**Step 4: Clippy clean**

Run: `cargo clippy --all --all-targets -- -D warnings`

**Step 5: Commit**

```bash
git add src/output/
git commit -m "feat(output): add line/col → byte span resolver for fallback rendering"
```

---

### Task 2.3: `MietteDiag` wrapper + `render_pretty`

**Files:**
- Create: `src/output/pretty.rs`
- Modify: `src/output/mod.rs` (add `mod pretty; pub use pretty::render_pretty;`)

**Step 1: Write failing integration test first**

Create `src/output/pretty.rs`:
```rust
//! Miette-powered pretty renderer. Groups diagnostics by file, reads each
//! source file once, and emits a labeled source frame per diagnostic with
//! rule help and doc URL pulled from the RuleMeta registry.
//!
//! Fallbacks:
//! - Diagnostic with no `span`: uses `span_resolver::resolve_line_span` to
//!   highlight the full line at `(line, column)`.
//! - File unreadable (race, virtual, deleted): falls back to the eslint-like
//!   single line for that diagnostic — no crash, no error.
//! - `rule_id` absent from the `RuleMeta` registry: help/url omitted,
//!   diagnostic still rendered.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::meta::RuleMeta;
use crate::rules::meta_registry;
use miette::{GraphicalReportHandler, GraphicalTheme, LabeledSpan, NamedSource, SourceSpan};
use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::span_resolver::resolve_line_span;

struct MietteDiag<'a> {
    diag: &'a Diagnostic,
    meta: Option<&'static RuleMeta>,
    source: NamedSource<String>,
    span: SourceSpan,
}

impl<'a> std::fmt::Debug for MietteDiag<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MietteDiag").field("rule_id", &self.diag.rule_id).finish()
    }
}

impl<'a> std::fmt::Display for MietteDiag<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.diag.message)
    }
}

impl<'a> std::error::Error for MietteDiag<'a> {}

impl<'a> miette::Diagnostic for MietteDiag<'a> {
    fn code<'b>(&'b self) -> Option<Box<dyn std::fmt::Display + 'b>> {
        Some(Box::new(self.diag.rule_id.clone()))
    }
    fn severity(&self) -> Option<miette::Severity> {
        Some(match self.diag.severity {
            Severity::Error => miette::Severity::Error,
            Severity::Warning => miette::Severity::Warning,
        })
    }
    fn help<'b>(&'b self) -> Option<Box<dyn std::fmt::Display + 'b>> {
        self.meta.map(|m| Box::new(m.remediation) as Box<dyn std::fmt::Display + 'b>)
    }
    fn url<'b>(&'b self) -> Option<Box<dyn std::fmt::Display + 'b>> {
        self.meta.and_then(|m| m.doc_url).map(|u| Box::new(u) as Box<dyn std::fmt::Display + 'b>)
    }
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.source)
    }
    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(std::iter::once(LabeledSpan::new_with_span(
            Some(self.diag.message.clone()),
            self.span,
        ))))
    }
}

/// Render a slice of diagnostics in pretty format. Groups by file, reads each
/// source once, falls back to eslint-like for any diag whose file can't be read.
pub fn render_pretty(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    let handler = GraphicalReportHandler::new().with_theme(GraphicalTheme::unicode());

    // Group preserving the caller's order — BTreeMap by path gives stable order,
    // within a file we preserve insertion order.
    let mut by_file: BTreeMap<&std::path::Path, Vec<&Diagnostic>> = BTreeMap::new();
    for d in diagnostics {
        by_file.entry(d.path.as_path()).or_default().push(d);
    }

    for (path, diags) in by_file {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => {
                // Fallback: eslint-like line per diagnostic.
                for d in &diags {
                    let sev = match d.severity {
                        Severity::Error => "error",
                        Severity::Warning => "warning",
                    };
                    let _ = writeln!(
                        out,
                        "{}:{}:{}: {} [{}] {}",
                        d.path.display(),
                        d.line,
                        d.column,
                        sev,
                        d.rule_id,
                        d.message,
                    );
                }
                continue;
            }
        };

        for d in diags {
            let span = d
                .span
                .or_else(|| resolve_line_span(&source, d.line, d.column))
                .unwrap_or((0, 0));
            let md = MietteDiag {
                diag: d,
                meta: meta_registry::lookup(&d.rule_id),
                source: NamedSource::new(path.display().to_string(), source.clone()),
                span: SourceSpan::new(span.0.into(), span.1),
            };
            let mut buf = String::new();
            // `render_report` returns Err only on fmt::Write failure into a String,
            // which is infallible — unwrap is correct here.
            handler.render_report(&mut buf, &md).expect("render into String is infallible");
            out.push_str(&buf);
            out.push('\n');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;
    use std::path::PathBuf;

    fn write_fixture(name: &str, contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("comply-miette-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn renders_rule_id_and_message_and_frame() {
        let path = write_fixture("fixture_a.ts", "const x = 1;\n");
        let diag = Diagnostic {
            path: path.clone(),
            line: 1,
            column: 7,
            rule_id: "no-weak-cipher".into(), // real rule id → has RuleMeta
            message: "example message".into(),
            severity: Severity::Warning,
            span: Some((6, 1)),
        };
        let out = render_pretty(&[diag]);
        assert!(out.contains("no-weak-cipher"), "rule id missing: {out}");
        assert!(out.contains("example message"), "message missing: {out}");
        assert!(out.contains("const x = 1;"), "source frame missing: {out}");
    }

    #[test]
    fn unreadable_file_falls_back_to_eslint_line() {
        let path = PathBuf::from("/definitely/does/not/exist/foo.ts");
        let diag = Diagnostic {
            path: path.clone(),
            line: 10,
            column: 5,
            rule_id: "no-weak-cipher".into(),
            message: "msg".into(),
            severity: Severity::Error,
            span: None,
        };
        let out = render_pretty(&[diag]);
        assert!(out.contains("foo.ts:10:5: error [no-weak-cipher] msg"), "got: {out}");
    }

    #[test]
    fn unknown_rule_id_renders_without_help() {
        let path = write_fixture("fixture_b.ts", "abc\n");
        let diag = Diagnostic {
            path,
            line: 1,
            column: 1,
            rule_id: "not-a-real-rule".into(),
            message: "m".into(),
            severity: Severity::Warning,
            span: Some((0, 3)),
        };
        let out = render_pretty(&[diag]);
        assert!(out.contains("not-a-real-rule"));
    }

    #[test]
    fn diag_without_span_resolves_whole_line() {
        let path = write_fixture("fixture_c.ts", "first\nsecond\n");
        let diag = Diagnostic {
            path,
            line: 2,
            column: 1,
            rule_id: "no-weak-cipher".into(),
            message: "m".into(),
            severity: Severity::Warning,
            span: None,
        };
        let out = render_pretty(&[diag]);
        assert!(out.contains("second"), "second line not highlighted: {out}");
    }
}
```

**Step 2: Wire module**

In `src/output/mod.rs`, add:
```rust
mod pretty;
pub use pretty::render_pretty;
```

**Step 3: Run tests**

Run: `cargo nextest run output::pretty`
Expected: PASS (4 tests). Cleanup the temp fixtures after run if desired.

If `miette::GraphicalReportHandler::render_report` signature or trait shape differs slightly in miette 7, adjust the call site per `cargo build` errors — the rest of the logic stands.

**Step 4: Clippy clean**

Run: `cargo clippy --all --all-targets -- -D warnings`

**Step 5: Commit**

```bash
git add src/output/
git commit -m "feat(output): add miette-powered pretty renderer with source frames and rule help"
```

---

## Phase 3 — CLI integration

### Task 3.1: Wire `report_diagnostics` with TTY auto-detection

**Files:**
- Modify: `src/main.rs` (around lines 537–558, the `report_diagnostics` function)

**Step 1: Update `report_diagnostics`**

Replace the existing body of `report_diagnostics` in `src/main.rs`:
```rust
fn report_diagnostics(diagnostics: &[Diagnostic]) {
    if diagnostics.is_empty() {
        println!("comply: all clear");
        return;
    }
    let stdout_is_tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let formatted = if stdout_is_tty {
        output::render_pretty(diagnostics)
    } else {
        output::format_eslint(diagnostics)
    };
    print!("{formatted}");
    eprintln!(
        "\ncomply: {} violation{} found",
        diagnostics.len(),
        if diagnostics.len() == 1 { "" } else { "s" }
    );
}
```

Ensure `use std::io::IsTerminal;` is present at the top of `main.rs` (add if missing).

**Step 2: Build**

Run: `cargo build`
Expected: zero errors.

**Step 3: Manual smoke test — pretty path (TTY)**

Run: `cargo run --release -- src/rules/no_weak_cipher/` (or a directory with known violations)
Expected: rendered output with source frames, rule ids, help text, coloring.

**Step 4: Manual smoke test — eslint path (piped)**

Run: `cargo run --release -- src/rules/no_weak_cipher/ | cat`
Expected: old eslint-like one-line-per-violation format.

**Step 5: Manual smoke test — `--json` unaffected**

Run: `cargo run --release -- --json src/rules/no_weak_cipher/ | head -20`
Expected: JSON array, now with optional `"span"` on any diag from a rule that sets it.

**Step 6: Clippy clean + full test run**

Run:
```
cargo clippy --all --all-targets -- -D warnings
cargo nextest run
```
Expected: zero clippy warnings, all 2940+ tests pass.

**Step 7: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): auto-select pretty vs eslint format based on stdout TTY"
```

---

## Phase 4 — Native rule opportunistic migration

### Task 4.1: Document the `at_node` pattern in `ast_check!` docblock

**Files:**
- Modify: `src/rules/registry.rs` (doc comment of `ast_check!` macro)

**Step 1: Add guidance to the macro doc**

Append to the doc block of `ast_check!` (above `macro_rules! ast_check`):
```rust
/// **Preferred construction for new rules:** use `Diagnostic::at_node` rather
/// than a `Diagnostic { ... }` literal. It captures the node's byte range so
/// the pretty renderer can highlight the exact offending expression rather than
/// the whole line:
///
/// ```ignore
/// crate::ast_check! { |node, source, ctx, diagnostics|
///     if node.kind() != "throw_statement" { return; }
///     diagnostics.push(Diagnostic::at_node(
///         ctx.path,
///         &node,
///         "no-throw-literal",
///         "throw an Error instance, not a literal".into(),
///         Severity::Warning,
///     ));
/// }
/// ```
///
/// Existing rules still work with the literal form (`span: None`) — migration
/// is opportunistic as rules are touched for other reasons.
```

**Step 2: Commit**

```bash
git add src/rules/registry.rs
git commit -m "docs(rules): document Diagnostic::at_node as preferred construction for new rules"
```

---

### Task 4.2: Migrate 3 flagship rules as examples

**Files:**
- Modify: `src/rules/no_weak_cipher/typescript.rs`
- Modify: `src/rules/no_weak_cipher/rust.rs`
- Modify: `src/rules/no_unverified_certificate/typescript.rs`
- Modify: `src/rules/no_unverified_certificate/rust.rs`
- Modify: `src/rules/no_insecure_jwt/typescript.rs`
- Modify: `src/rules/no_insecure_jwt/rust.rs`

**Step 1: For each file, replace `Diagnostic { ... }` with `Diagnostic::at_node(...)`**

Example migration (`no_weak_cipher/rust.rs`):
```rust
// BEFORE
let pos = node.start_position();
diagnostics.push(Diagnostic {
    path: ctx.path.to_path_buf(),
    line: pos.row + 1,
    column: pos.column + 1,
    rule_id: "no-weak-cipher".into(),
    message: "...".into(),
    severity: Severity::Warning,
    span: None,
});

// AFTER
diagnostics.push(Diagnostic::at_node(
    ctx.path,
    &node,
    "no-weak-cipher",
    "...".into(),
    Severity::Warning,
));
```

**Step 2: Run the affected rule tests only**

Run:
```
cargo nextest run no_weak_cipher no_unverified_certificate no_insecure_jwt
```
Expected: all existing rule tests still pass (span is a superset — the (line, col) stays the same).

**Step 3: Visual check — run comply on a fixture with one of these violations**

Create `/tmp/weak_cipher_sample.ts` with a known violation. Run:
```
cargo run --release -- /tmp/weak_cipher_sample.ts
```
Expected: the rendered frame highlights the *exact offending expression*, not the whole line. Contrast with a delegated/unmigrated rule which still shows the whole line — confirming the span pipeline works end-to-end.

**Step 4: Clippy clean + full test run**

Run:
```
cargo clippy --all --all-targets -- -D warnings
cargo nextest run
```

**Step 5: Commit**

```bash
git add src/rules/no_weak_cipher src/rules/no_unverified_certificate src/rules/no_insecure_jwt
git commit -m "refactor(rules): migrate flagship security rules to Diagnostic::at_node for exact span highlighting"
```

---

## Success criteria

At plan completion the following must all hold:

1. `cargo nextest run` — 2940+ tests pass (same or more than before).
2. `cargo clippy --all --all-targets -- -D warnings` — zero warnings, zero new `#[allow]` attributes.
3. `cargo run --release -- <dir>` on a directory with violations, **in a TTY**, renders source frames with rule ids, help, and doc URLs for rules that have them.
4. `cargo run --release -- <dir> | cat` (piped) still renders the **exact** prior eslint-like format (grep-friendly, single line per violation).
5. `cargo run --release -- --json <dir>` still renders a JSON array; diagnostics from migrated native rules include `"span": {"offset": N, "length": N}`; unmigrated and delegated diagnostics omit the `span` key entirely.
6. A diagnostic whose source file is unreadable at render time (e.g., deleted between scan and render) falls back to the eslint line form **without crashing**.
7. A diagnostic whose `rule_id` is not in the `meta_registry` (delegated from oxlint/clippy) renders without `help`/`url` but with a correct source frame (whole line highlighted via `resolve_line_span`).
8. The 3 migrated flagship rules (`no-weak-cipher`, `no-unverified-certificate`, `no-insecure-jwt`) highlight the **exact expression**, not the whole line.

## Open questions (none blocking)

- **Clippy allow-lists:** if miette's own generated code triggers a clippy warning in the renderer's use site, prefer restructuring the call over adding `#[allow]`. This was explicitly confirmed (Q6.3).
- **Terminal width:** miette auto-detects width. No comply-side handling.
- **Message/remediation duplication:** tolerated for v1 per Q5.2. TODO cleanup tracked elsewhere.
- **Oxlint/clippy span extraction:** explicitly out of scope for v1 (Q3.3). Whole-line highlighting is the accepted v1 behavior for delegated diagnostics.

## Not in scope (YAGNI)

- Secondary/related diagnostic labels
- Custom miette theme
- Miette's JSON output mode
- `--max-display=N` cap
- Remapping oxlint rule ids to `RuleMeta`
- Progressive migration of all 500+ existing rules to `Diagnostic::at_node` (only the 3 flagship rules in Phase 4)

## Worktree reminder

This plan should be executed in a dedicated git worktree. The executing session should:
1. Create a worktree via `superpowers:using-git-worktrees` before starting Phase 1.
2. Run all phases in that worktree.
3. Verify success criteria.
4. Return to the main branch and merge via `superpowers:finishing-a-development-branch`.
