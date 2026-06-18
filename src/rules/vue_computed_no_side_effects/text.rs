//! vue-computed-no-side-effects AST backend.
//!
//! Scans lines between `computed(` and the matching brace/paren end, flagging
//! emit() / console.* / fetch() / axios. / assignments to `.value =` etc.
//!
//! Uses the Vue tree-sitter grammar as the dispatch mechanism: we run once per
//! file on the root `component` node, then scan the source for `computed(`
//! calls via text-matching (the grammar exposes `<script>` bodies as a single
//! `raw_text` blob, not as parsed JS/TS, so a true AST walk of the call body
//! would require re-parsing TS — which is not cheaper than the text scan for
//! this heuristic).
//!
//! Writable-computed exemption: when the `computed(...)` argument is an options
//! object with a `set` property (`computed({ get, set })`), the lines inside the
//! `set` function block are exempt from both the side-effect-marker and the
//! `.value =` checks — assignments and reactive writes are a setter's purpose.
//! The pure getter body stays checked.

use crate::diagnostic::{Diagnostic, Severity};

const SIDE_EFFECT_MARKERS: &[&str] = &[
    "emit(", "console.", "fetch(", "axios.", ".post(", ".put(", ".delete(", ".patch(", "$emit(",
];

/// Byte range `[open_brace ..= close_brace]` of the writable-computed `set`
/// function block within `body`, or `None` when there is no such block.
///
/// Matches a `set` property KEY — a `set` whose preceding char is not part of a
/// larger identifier (`offset`/`reset`/`asset` are rejected) and whose next
/// non-whitespace char is `(` (method shorthand) or `:` (arrow / function
/// expression). From the key it finds the next `{` (the function body open
/// brace) and brace-matches to the close. String-literal awareness is out of
/// scope: a `{`/`}` inside a string in the params or body could skew the match.
fn set_block_range(body: &str) -> Option<(usize, usize)> {
    let bytes = body.as_bytes();
    let mut search = 0;
    while let Some(rel) = body[search..].find("set") {
        let key_start = search + rel;
        let key_end = key_start + 3;
        search = key_end;
        let prev = if key_start == 0 {
            ' '
        } else {
            bytes[key_start - 1] as char
        };
        if prev.is_alphanumeric() || prev == '_' || prev == '.' {
            continue;
        }
        let mut k = key_end;
        while k < bytes.len() && (bytes[k] as char).is_whitespace() {
            k += 1;
        }
        if k >= bytes.len() || (bytes[k] != b'(' && bytes[k] != b':') {
            continue;
        }
        let open_rel = body[key_end..].find('{')?;
        let open = key_end + open_rel;
        let mut depth: i32 = 0;
        let mut j = open;
        while j < bytes.len() {
            match bytes[j] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((open, j));
                    }
                }
                _ => {}
            }
            j += 1;
        }
        return None;
    }
    None
}

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    // Gate: run once per file, when we first see the root `component` node.
    let _ = source;
    let src = ctx.source;
    let bytes = src.as_bytes();
    let mut i = 0;
    while let Some(pos) = src[i..].find("computed(") {
        let abs = i + pos;
        let prev = if abs == 0 { ' ' } else { bytes[abs - 1] as char };
        if prev.is_alphanumeric() || prev == '_' {
            i = abs + 9;
            continue;
        }
        let start = abs + 9;
        let mut depth: i32 = 1;
        let mut j = start;
        while j < bytes.len() && depth > 0 {
            match bytes[j] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            j += 1;
        }
        let body = &src[start..j.saturating_sub(1)];
        let base_line = src[..abs].matches('\n').count();
        let set_range = set_block_range(body);
        let mut line_start = 0usize;
        for (line_off, line) in body.lines().enumerate() {
            let cur_start = line_start;
            let cur_end = cur_start + line.len();
            line_start += line.len() + 1; // +1 for the stripped '\n'
            if let Some((set_open, set_close)) = set_range {
                // Skip a line whose span overlaps the `set` block range — covers
                // both a single-line `set(v) { ... }` and a multi-line body.
                if cur_start <= set_close && cur_end > set_open {
                    continue;
                }
            }
            for marker in SIDE_EFFECT_MARKERS {
                if line.contains(marker) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: base_line + line_off + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Side effect `{marker}` inside `computed()` — computeds must be pure."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                    break;
                }
            }
            let trimmed = line.trim_start();
            if trimmed.contains(".value =") && !trimmed.contains("==") {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: base_line + line_off + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Assignment to `.value` inside `computed()` — computeds must be pure.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        i = j;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source), &tree)
    }

    #[test]
    fn flags_console_in_computed() {
        let sfc =
            "<script setup>\nconst c = computed(() => { console.log('x'); return 1 })\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_emit_in_computed() {
        let sfc = "<script setup>\nconst c = computed(() => { emit('x'); return 1 })\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_value_assign_in_computed() {
        let sfc =
            "<script setup>\nconst c = computed(() => { other.value = 2; return 1 })\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_pure_computed() {
        let sfc = "<script setup>\nconst c = computed(() => x.value * 2)\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_value_assign_in_writable_setter_method() {
        let sfc = "<script setup>\nconst displayX = computed({\n  get() { return x.value },\n  set(val) { x.value = Number.parseFloat(val) },\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_value_assign_in_writable_setter_arrow() {
        let sfc = "<script setup>\nconst c = computed({\n  get: () => x.value,\n  set: (v) => { x.value = v },\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_value_assign_in_writable_getter() {
        let sfc = "<script setup>\nconst c = computed({\n  get() { y.value = 1; return x.value },\n  set(v) { x.value = v },\n})\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        // The diagnostic is the getter assignment (line 3), not the setter (line 4).
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn set_substring_in_identifier_is_not_a_set_block() {
        let sfc = "<script setup>\nconst c = computed(() => { offset.value = 1; return offset.value })\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }
}
