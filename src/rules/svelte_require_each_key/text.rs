//! svelte-require-each-key — text backend.
//!
//! Flags `{#each ... as ...}` blocks that omit a `(key)` clause. Svelte syntax
//! is `{#each expr as name (key)}` or `{#each expr as name, i (key)}`; the
//! trailing `(key)` is what lets Svelte track items across updates. Blocks with
//! no `as` binding (e.g. `{#each items, i}`) are never keyed and are skipped.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_svelte(path: &std::path::Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("svelte")
}

/// Returns the byte index of the `}` that closes the block tag opened at
/// `open` (the index of the `{`), tracking nested `{...}` interpolations. None
/// if the brace never closes (malformed source — caller skips).
fn matching_close(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Given the inner text of an `{#each ...}` block (between `{#each` and its
/// closing `}`), returns true when it declares an `as` binding but no `(key)`
/// clause. A `(...)` group at paren-depth 0 anywhere after the `as` keyword is
/// the key; the iterable expression's own parens sit before `as` and the
/// binding pattern uses `{}`/`[]`, so a trailing top-level `(` is unambiguous.
fn is_unkeyed_each(inner: &str) -> bool {
    let Some(after_as) = find_as_clause(inner) else {
        return false;
    };
    !after_as.contains('(')
}

/// Returns the slice following the `as` keyword if `inner` contains one as a
/// standalone word (not part of an identifier like `cast`). None otherwise.
fn find_as_clause(inner: &str) -> Option<&str> {
    let bytes = inner.as_bytes();
    let mut search_from = 0;
    while let Some(rel) = inner[search_from..].find("as") {
        let i = search_from + rel;
        let before = if i == 0 { b' ' } else { bytes[i - 1] };
        let after = bytes.get(i + 2).copied().unwrap_or(b' ');
        let boundary_before = !before.is_ascii_alphanumeric() && before != b'_' && before != b'$';
        let boundary_after = !after.is_ascii_alphanumeric() && after != b'_' && after != b'$';
        if boundary_before && boundary_after {
            return Some(&inner[i + 2..]);
        }
        search_from = i + 2;
    }
    None
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["{#each"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_svelte(ctx.path) {
            return Vec::new();
        }
        let source = ctx.source;
        let bytes = source.as_bytes();
        let mut diagnostics = Vec::new();
        let mut search_from = 0;
        while let Some(rel) = source[search_from..].find("{#each") {
            let open = search_from + rel;
            let Some(close) = matching_close(bytes, open) else {
                break;
            };
            // Inner content sits between `{#each` and the closing `}`.
            let inner = &source[open + "{#each".len()..close];
            if is_unkeyed_each(inner) {
                let line = source[..open].bytes().filter(|b| *b == b'\n').count() + 1;
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column: 1,
                    rule_id: "svelte-require-each-key".into(),
                    message: "Add a `(key)` clause to this `{#each}` block, e.g. `{#each items as item (item.id)}`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            search_from = close + 1;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.svelte"), source))
    }

    // --- Biome invalid.svelte fixtures: each must fire ---

    #[test]
    fn flags_plain_each() {
        let src = "{#each items as item}\n  <div>{item}</div>\n{/each}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_each_with_index() {
        let src = "{#each items as item, i}\n  <div>{i}: {item}</div>\n{/each}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_each_with_destructuring() {
        let src = "{#each users as { id, name }}\n  <div>{id}: {name}</div>\n{/each}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_all_three_invalid_fixtures() {
        let src = "<!-- should generate diagnostics -->\n\n\
{#each items as item}\n  <div>{item}</div>\n{/each}\n\n\
{#each items as item, i}\n  <div>{i}: {item}</div>\n{/each}\n\n\
{#each users as { id, name }}\n  <div>{id}: {name}</div>\n{/each}";
        assert_eq!(run(src).len(), 3);
    }

    // --- Biome valid.svelte fixtures: none must fire ---

    #[test]
    fn allows_keyed_each() {
        let src = "{#each items as item (item.id)}\n  <div>{item}</div>\n{/each}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_keyed_each_with_index() {
        let src = "{#each items as item, i (item.id)}\n  <div>{i}: {item}</div>\n{/each}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_keyed_each_with_destructuring() {
        let src = "{#each users as { id, name } (id)}\n  <div>{id}: {name}</div>\n{/each}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_each_without_as_binding() {
        // No `as` clause → cannot be keyed; Biome does not flag this.
        let src = "{#each items, i}\n  <div>{i}</div>\n{/each}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_all_four_valid_fixtures() {
        let src = "<!-- should not generate diagnostics -->\n\n\
{#each items as item (item.id)}\n  <div>{item}</div>\n{/each}\n\n\
{#each items as item, i (item.id)}\n  <div>{i}: {item}</div>\n{/each}\n\n\
{#each users as { id, name } (id)}\n  <div>{id}: {name}</div>\n{/each}\n\n\
{#each items, i}\n  <div>{i}</div>\n{/each}";
        assert!(run(src).is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_if_block() {
        let src = "{#if cond}\n  <div>x</div>\n{/if}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_await_block() {
        let src = "{#await promise}\n  <p>loading</p>\n{:then value}\n  <p>{value}</p>\n{/await}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_keyed_each_over_call_expression() {
        // The iterable's own parens precede `as`; the key still resolves.
        let src = "{#each getItems() as item (item.id)}\n  <div>{item}</div>\n{/each}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_unkeyed_each_over_call_expression() {
        // Parens on the iterable expression are not a key.
        let src = "{#each getItems() as item}\n  <div>{item}</div>\n{/each}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_non_svelte_file() {
        let diags = Check.check(&CheckCtx::for_test(
            Path::new("notes.md"),
            "{#each items as item}",
        ));
        assert!(diags.is_empty());
    }

    #[test]
    fn handles_keyed_with_interpolation_in_expression() {
        // Nested `{...}` inside the iterable must not end the block tag early.
        let src = "{#each data[`${prefix}`] as item (item.id)}\n  <div>{item}</div>\n{/each}";
        assert!(run(src).is_empty());
    }
}
