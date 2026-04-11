//! Extract only the snippets relevant for LLM analysis from a source
//! file. Cuts token usage by 60-80% compared to sending the full file.
//!
//! Each snippet keeps its original line numbers so the LLM can report
//! accurate locations. Irrelevant stretches are replaced with a single
//! `... (lines N-M omitted)` marker.
//!
//! Ranges are collected, then merged — if block A contains block B,
//! only A survives. This prevents a `///` doc comment on a struct
//! field from duplicating the enclosing struct.

/// Walk the source and keep only blocks that matter for LLM rules.
/// Returns a new string with `N: original_line` numbering preserved.
pub fn extract_snippets(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let total = lines.len();
    if total == 0 {
        return String::new();
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // 1. Comment lines → keep the comment + the block it documents.
        if is_comment_start(trimmed) {
            let r = comment_and_block_range(&lines, i);
            ranges.push(r);
        }

        // 2. Functions >20 lines → keep full body (mixed_abstraction).
        //    Smaller functions → signature only (intent_naming, shallow_module).
        if is_function_decl(trimmed) {
            let block_end = block_end(&lines, i);
            let size = block_end - i + 1;
            if size > 20 {
                ranges.push((i, block_end));
            } else {
                let sig_end = signature_end(&lines, i);
                ranges.push((i, sig_end));
                ranges.push((block_end, block_end));
            }
        }

        // 2b. Non-function declarations (struct, enum, impl, trait,
        //     type, const, mod, pub use) → keep the full block.
        if is_declaration(trimmed) {
            ranges.push((i, block_end(&lines, i)));
        }

        // 3. Log/print statements → 2 lines of context each side.
        if is_log_statement(trimmed) {
            ranges.push((i.saturating_sub(2), (i + 2).min(total - 1)));
        }
    }

    // Merge overlapping/contained ranges.
    let merged = merge_ranges(&mut ranges);

    // Build output with line numbers, inserting gap markers.
    let mut out = String::with_capacity(source.len() / 2);
    let mut last_end: Option<usize> = None;

    for &(start, end) in &merged {
        if let Some(prev) = last_end {
            if start > prev + 1 {
                out.push_str(&format!(
                    "... (lines {}-{} omitted)\n",
                    prev + 2,
                    start,
                ));
            }
        } else if start > 0 {
            out.push_str(&format!("... (lines 1-{} omitted)\n", start));
        }
        for (i, line) in lines.iter().enumerate().take(end + 1).skip(start) {
            out.push_str(&format!("{}: {}\n", i + 1, line));
        }
        last_end = Some(end);
    }

    if let Some(prev) = last_end
        && prev + 1 < total {
            out.push_str(&format!(
                "... (lines {}-{} omitted)\n",
                prev + 2,
                total,
            ));
        }

    out
}

// ── Range helpers ───────────────────────────────────────────────────

/// Sort ranges by start, then merge overlapping/contained ones.
fn merge_ranges(ranges: &mut [(usize, usize)]) -> Vec<(usize, usize)> {
    if ranges.is_empty() {
        return vec![];
    }
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = vec![ranges[0]];
    for &(s, e) in &ranges[1..] {
        let last = merged.last_mut().unwrap();
        if s <= last.1 + 1 {
            // Overlapping or adjacent — extend.
            last.1 = last.1.max(e);
        } else {
            merged.push((s, e));
        }
    }
    merged
}

// ── Block detection ─────────────────────────────────────────────────

fn is_comment_start(trimmed: &str) -> bool {
    trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with("* ")
        || trimmed == "*/"
}

fn is_function_decl(trimmed: &str) -> bool {
    // Rust
    if trimmed.starts_with("pub fn ")
        || trimmed.starts_with("pub async fn ")
        || trimmed.starts_with("fn ")
        || trimmed.starts_with("async fn ")
        || trimmed.starts_with("pub(crate) fn ")
    {
        return true;
    }
    // TypeScript/JavaScript
    if trimmed.starts_with("export function ")
        || trimmed.starts_with("export async function ")
        || trimmed.starts_with("export default function ")
        || trimmed.starts_with("function ")
        || trimmed.starts_with("async function ")
    {
        return true;
    }
    // Arrow/method patterns
    if (trimmed.contains("=> {") || trimmed.contains("=> ("))
        && (trimmed.starts_with("export const ") || trimmed.starts_with("const "))
    {
        return true;
    }
    false
}

fn is_declaration(trimmed: &str) -> bool {
    trimmed.starts_with("pub struct ")
        || trimmed.starts_with("struct ")
        || trimmed.starts_with("pub enum ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("impl ")
        || trimmed.starts_with("pub trait ")
        || trimmed.starts_with("trait ")
        || trimmed.starts_with("pub type ")
        || trimmed.starts_with("type ")
        || trimmed.starts_with("pub const ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("pub static ")
        || trimmed.starts_with("static ")
        || trimmed.starts_with("pub mod ")
        || trimmed.starts_with("mod ")
        || trimmed.starts_with("pub use ")
        || trimmed.starts_with("pub(crate) struct ")
        || trimmed.starts_with("pub(crate) enum ")
        || trimmed.starts_with("pub(crate) type ")
        || trimmed.starts_with("pub(crate) mod ")
        || trimmed.starts_with("pub(crate) use ")
        // TS/JS exports
        || trimmed.starts_with("export type ")
        || trimmed.starts_with("export interface ")
        || trimmed.starts_with("export class ")
}

fn is_log_statement(trimmed: &str) -> bool {
    trimmed.contains("console.log")
        || trimmed.contains("console.warn")
        || trimmed.contains("console.error")
        || trimmed.contains("console.info")
        || trimmed.contains("logger.")
        || trimmed.contains("log::info!")
        || trimmed.contains("log::warn!")
        || trimmed.contains("log::error!")
        || trimmed.contains("log::debug!")
        || trimmed.contains("tracing::info!")
        || trimmed.contains("tracing::warn!")
        || trimmed.contains("tracing::error!")
        || trimmed.contains("tracing::debug!")
        || trimmed.contains("eprintln!")
        || trimmed.contains("println!")
        || trimmed.contains("info!(")
        || trimmed.contains("warn!(")
        || trimmed.contains("error!(")
        || trimmed.contains("debug!(")
}

/// Range covering a comment block + the code it documents.
fn comment_and_block_range(lines: &[&str], start: usize) -> (usize, usize) {
    let total = lines.len();
    let mut i = start;

    // Skip consecutive comment lines.
    while i < total && is_comment_start(lines[i].trim()) {
        i += 1;
    }
    // Skip blanks.
    while i < total && lines[i].trim().is_empty() {
        i += 1;
    }

    if i >= total {
        return (start, i.saturating_sub(1));
    }

    let end = block_end(lines, i);
    (start, end)
}

/// Find the last line of a brace-delimited block starting at `start`.
fn block_end(lines: &[&str], start: usize) -> usize {
    let total = lines.len();
    let mut depth: i32 = 0;
    let mut found_open = false;

    for (i, line) in lines.iter().enumerate().take(total).skip(start) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                found_open = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        if found_open && depth <= 0 {
            return i;
        }
        // No braces after a few lines → single-line declaration.
        if !found_open && i > start + 2 {
            return i;
        }
    }
    total.saturating_sub(1)
}

/// Find the end of a function signature (up to and including the `{`).
fn signature_end(lines: &[&str], start: usize) -> usize {
    let total = lines.len();
    for (i, line) in lines.iter().enumerate().take(total).skip(start) {
        if line.contains('{') {
            return i;
        }
        if i > start + 3 {
            return i;
        }
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_function_with_comment() {
        let src = "\
use std::io;

/// Reads config from disk.
fn read_config(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn helper() {
    let x = 1;
    let y = 2;
}";
        let out = extract_snippets(src);
        assert!(out.contains("/// Reads config from disk."));
        assert!(out.contains("fn read_config"));
        assert!(out.contains("fn helper"));
        assert!(out.contains("omitted"));
    }

    #[test]
    fn extracts_log_with_context() {
        let src = "\
fn process() {
    let user = get_user();
    let email = user.email;
    println!(\"Processing {email}\");
    do_stuff();
}";
        let out = extract_snippets(src);
        assert!(out.contains("println!"));
        assert!(out.contains("email"));
    }

    #[test]
    fn empty_source() {
        assert!(extract_snippets("").is_empty());
    }

    #[test]
    fn preserves_line_numbers() {
        let src = "line1\nline2\nfn foo() {\n}\nline5";
        let out = extract_snippets(src);
        assert!(out.contains("3: fn foo()"));
    }

    #[test]
    fn dedup_nested_ranges() {
        let mut ranges = vec![(5, 50), (10, 20), (15, 18), (48, 55)];
        let merged = merge_ranges(&mut ranges);
        assert_eq!(merged, vec![(5, 55)]);
    }

    #[test]
    fn dedup_disjoint_ranges() {
        let mut ranges = vec![(1, 5), (10, 20), (30, 40)];
        let merged = merge_ranges(&mut ranges);
        assert_eq!(merged, vec![(1, 5), (10, 20), (30, 40)]);
    }

    #[test]
    fn doc_comments_on_struct_fields_dont_duplicate_struct() {
        // A struct with 3 doc-commented fields — the struct should
        // appear once, not 3 times.
        let src = "\
use something;

/// The main config.
struct Config {
    /// Server IP.
    ip: String,
    /// Server port.
    port: u16,
    /// Enable TLS.
    tls: bool,
}

fn other() {}";
        let out = extract_snippets(src);
        let config_count = out.matches("struct Config").count();
        assert_eq!(config_count, 1, "struct Config should appear once, got:\n{out}");
    }
}
