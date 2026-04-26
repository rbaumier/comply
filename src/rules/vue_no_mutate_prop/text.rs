//! vue-no-mutate-prop text backend.
//!
//! Scans the `<script setup>` region of a Vue SFC for direct prop
//! mutations (`props.foo = ...`, `props.items.length = 0`, compound
//! assignments, etc.). Props are a one-way contract — the parent owns
//! them, so the child must emit events or copy the value into a local
//! ref before mutating. Equality comparisons (`==`, `===`) and reads
//! are untouched.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns the `(start_line_idx, end_line_idx_exclusive)` lines for each
/// `<script setup ...>...</script>` region in the source, where each
/// index is a 0-based line index.
fn script_setup_line_ranges(source: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        // Look for an opening `<script` that also contains `setup`.
        let lower = line.to_ascii_lowercase();
        if let Some(script_pos) = lower.find("<script") {
            // Find the closing `>` of the opening tag — may span lines,
            // but in practice SFC openings are single-line. We search
            // forward for `>` in the same line first, else across lines.
            let tag_open_line = i;
            let mut tag_end_line = i;
            let mut has_setup = lower[script_pos..]
                .split('>')
                .next()
                .map(|s| s.contains("setup"))
                .unwrap_or(false);
            if !lower[script_pos..].contains('>') {
                // Opening tag spans multiple lines — walk forward.
                let mut j = i + 1;
                while j < lines.len() {
                    let l = lines[j].to_ascii_lowercase();
                    if l.contains("setup") {
                        has_setup = true;
                    }
                    if l.contains('>') {
                        tag_end_line = j;
                        break;
                    }
                    j += 1;
                }
            }
            if has_setup {
                // Body starts on the line after the opening tag ends.
                let body_start = tag_end_line + 1;
                // Find the `</script>` closing tag.
                let mut k = body_start;
                while k < lines.len() {
                    if lines[k].to_ascii_lowercase().contains("</script>") {
                        break;
                    }
                    k += 1;
                }
                ranges.push((body_start, k));
                i = k + 1;
                continue;
            }
            i = tag_open_line + 1;
            continue;
        }
        i += 1;
    }
    ranges
}

/// Given a line already known to contain `props.`, returns `Some(col)`
/// (0-based column of the `props.` token) if the line is a direct prop
/// mutation (assignment), else `None`.
fn prop_mutation_column(line: &str) -> Option<usize> {
    // Scan every occurrence of `props.` in the line, since there may be
    // multiple (e.g. `foo(props.a, props.b = 1)`).
    let bytes = line.as_bytes();
    let mut search_from = 0;
    while let Some(rel) = line[search_from..].find("props.") {
        let start = search_from + rel;
        // Require a non-identifier char immediately before `props.` so
        // we don't match `myProps.` or `userProps.foo`.
        if start > 0 {
            let prev = bytes[start - 1] as char;
            if prev.is_ascii_alphanumeric() || prev == '_' || prev == '$' {
                search_from = start + 6;
                continue;
            }
        }
        // Walk past `props.` and the subsequent identifier / chained path
        // (`.ident`, `[...]` is skipped for simplicity).
        let mut p = start + "props.".len();
        // Must have at least one identifier char.
        if p >= bytes.len() || !(bytes[p] as char).is_ascii_alphabetic()
            && bytes[p] != b'_'
            && bytes[p] != b'$'
        {
            search_from = start + 6;
            continue;
        }
        while p < bytes.len() {
            let c = bytes[p] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                p += 1;
            } else if c == '.' {
                // chained access — next char must start another ident.
                if p + 1 < bytes.len() {
                    let nc = bytes[p + 1] as char;
                    if nc.is_ascii_alphabetic() || nc == '_' || nc == '$' {
                        p += 1;
                        continue;
                    }
                }
                break;
            } else {
                break;
            }
        }
        // Skip whitespace after the path.
        while p < bytes.len() && (bytes[p] == b' ' || bytes[p] == b'\t') {
            p += 1;
        }
        // Inspect the operator.
        if p < bytes.len() {
            let c = bytes[p] as char;
            let next = if p + 1 < bytes.len() {
                Some(bytes[p + 1] as char)
            } else {
                None
            };
            let nnext = if p + 2 < bytes.len() {
                Some(bytes[p + 2] as char)
            } else {
                None
            };
            match c {
                '=' => {
                    // `==`, `===`, `=>` are not assignments.
                    if next == Some('=') || next == Some('>') {
                        search_from = start + 6;
                        continue;
                    }
                    return Some(start);
                }
                '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^' => {
                    // `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=` are
                    // assignments. `**=`, `<<=`, `>>=` handled below.
                    if next == Some('=') && nnext != Some('=') {
                        return Some(start);
                    }
                    if c == '*' && next == Some('*') && nnext == Some('=') {
                        return Some(start);
                    }
                }
                '<' | '>' => {
                    // `<<=`, `>>=`, `>>>=` are assignments.
                    if next == Some(c) {
                        // Look ahead for `=`.
                        let mut q = p + 2;
                        if c == '>' && q < bytes.len() && bytes[q] as char == '>' {
                            q += 1;
                        }
                        if q < bytes.len() && bytes[q] as char == '=' {
                            return Some(start);
                        }
                    }
                }
                _ => {}
            }
        }
        search_from = start + 6;
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let ranges = script_setup_line_ranges(ctx.source);
        if ranges.is_empty() {
            return diags;
        }
        let lines: Vec<&str> = ctx.source.lines().collect();
        for (start, end) in ranges {
            let upper = end.min(lines.len());
            for (idx, line) in lines.iter().enumerate().take(upper).skip(start) {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") {
                    continue;
                }
                if !line.contains("props.") {
                    continue;
                }
                if let Some(col) = prop_mutation_column(line) {
                    diags.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: super::META.id.into(),
                        message:
                            "Mutating a prop directly breaks Vue's one-way data flow. \
                             Emit an event or copy the prop into a local ref instead."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }

    fn wrap(body: &str) -> String {
        format!("<script setup lang=\"ts\">\n{body}\n</script>\n")
    }

    #[test]
    fn flags_simple_prop_assignment() {
        assert_eq!(run(&wrap("props.count = 5")).len(), 1);
    }

    #[test]
    fn flags_compound_assignment() {
        assert_eq!(run(&wrap("props.count += 1")).len(), 1);
    }

    #[test]
    fn flags_nested_path_assignment() {
        assert_eq!(run(&wrap("props.items.length = 0")).len(), 1);
    }

    #[test]
    fn allows_emit_call() {
        assert!(run(&wrap("emit('update', x)")).is_empty());
    }

    #[test]
    fn allows_read_access() {
        assert!(run(&wrap("const x = props.foo")).is_empty());
    }

    #[test]
    fn allows_equality_comparison() {
        assert!(run(&wrap("if (props.foo == null) { return; }")).is_empty());
        assert!(run(&wrap("if (props.foo === null) { return; }")).is_empty());
    }

    #[test]
    fn allows_comment_line() {
        assert!(run(&wrap("// props.count = 5")).is_empty());
    }

    #[test]
    fn ignores_outside_script_setup() {
        let src = "<template><div>{{ foo }}</div></template>\n\
                   <script>\nprops.count = 5\n</script>\n";
        assert!(run(src).is_empty());
    }
}
