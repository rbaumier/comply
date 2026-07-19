//! vue-no-mutate-prop text backend.
//!
//! Flags direct prop mutations (`props.foo = ...`, `props.items.length = 0`,
//! compound assignments, etc.) inside a `<script setup>` region. Props are a
//! one-way contract — the parent owns them, so the child must emit events or
//! copy the value into a local ref before mutating. Equality comparisons
//! (`==`, `===`) and reads are untouched.
//!
//! A `props.X = ...` counts as a prop mutation only when the block binds `props`
//! to the props macro — `const props = defineProps(...)` or
//! `const props = withDefaults(defineProps(...), ...)`. A block with no such
//! binding has no prop object named `props`, so a same-named local payload
//! object is left alone. A `props` redeclared as a parameter or an earlier local
//! inside a function/arrow scope shadows the macro binding, so its mutations are
//! likewise untouched.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_shadow_scope::{ShadowScope, collect_shadow_scopes};

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
        if p >= bytes.len()
            || !(bytes[p] as char).is_ascii_alphabetic() && bytes[p] != b'_' && bytes[p] != b'$'
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

/// Whether `byte` can be part of a JS/TS identifier — used for keyword and
/// macro-call word boundaries.
fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

/// Whether `s` begins with a call to `name` — `name` immediately followed by a
/// non-identifier byte (`(`, `<`, …) so `definePropsHelper(` is not matched.
fn starts_with_macro_call(s: &str, name: &str) -> bool {
    match s.strip_prefix(name) {
        Some(rest) => match rest.as_bytes().first() {
            None => true,
            Some(&b) => !is_ident_byte(b),
        },
        None => false,
    }
}

/// Whether the `<script setup>` block binds the identifier `props` to the props
/// macro: `const props = defineProps(...)` or
/// `const props = withDefaults(defineProps(...), ...)`. A `defineProps` result
/// bound to any other name — or no `defineProps` at all — does not bind `props`,
/// so a `props.X = ...` there mutates an ordinary local, not a prop.
fn binds_props_to_macro(block: &str) -> bool {
    let bytes = block.as_bytes();
    for kw in ["const", "let", "var"] {
        for (kw_pos, _) in block.match_indices(kw) {
            let before_ok = kw_pos == 0 || !is_ident_byte(bytes[kw_pos - 1]);
            let after_kw = kw_pos + kw.len();
            let after_ok = after_kw >= bytes.len() || !is_ident_byte(bytes[after_kw]);
            if !before_ok || !after_ok {
                continue;
            }
            // Skip whitespace, then read the declared identifier — it must be
            // exactly `props`.
            let mut i = after_kw;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let name_start = i;
            while i < bytes.len() && is_ident_byte(bytes[i]) {
                i += 1;
            }
            if &block[name_start..i] != "props" {
                continue;
            }
            // Require a plain `=` initializer (not `==`), skipping whitespace.
            // A `: Type` annotation before `=` (e.g. `props: Foo = defineProps()`)
            // is the annotated form, intentionally not resolved here — it is left
            // for the next candidate.
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'=' || bytes.get(i + 1) == Some(&b'=') {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let init = &block[i..];
            if starts_with_macro_call(init, "defineProps")
                || (starts_with_macro_call(init, "withDefaults") && init.contains("defineProps"))
            {
                return true;
            }
        }
    }
    false
}

/// Whether a mutation at absolute byte `offset` sits inside a function/arrow
/// scope that redeclares `props` as a parameter or an earlier local, shadowing
/// the macro binding — such a `props` is an ordinary local, not the prop object.
fn props_shadowed_at(scopes: &[ShadowScope], offset: usize) -> bool {
    scopes.iter().any(|s| {
        s.body.contains(&offset)
            && (s.params.contains("props")
                || s.locals.iter().any(|(name, decl)| name == "props" && *decl < offset))
    })
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let ranges = script_setup_line_ranges(ctx.source);
        if ranges.is_empty() {
            return diags;
        }
        let lines: Vec<&str> = ctx.source.lines().collect();
        let base = ctx.source.as_ptr() as usize;
        // Function/arrow scopes that redeclare `props` shadow the macro binding.
        // Resolved lazily, once, only when a block actually binds `props`.
        let mut shadow_scopes: Option<Vec<ShadowScope>> = None;
        for (start, end) in ranges {
            let upper = end.min(lines.len());
            if start >= upper {
                continue;
            }
            // Only a block that binds `props` to `defineProps()` /
            // `withDefaults(defineProps(...))` has a prop object named `props`;
            // otherwise a `props.X = ...` mutates an ordinary local.
            let body = &lines[start..upper];
            let block_start = body[0].as_ptr() as usize - base;
            let last = body[body.len() - 1];
            let block_end = (last.as_ptr() as usize - base) + last.len();
            if !binds_props_to_macro(&ctx.source[block_start..block_end]) {
                continue;
            }
            let scopes = shadow_scopes.get_or_insert_with(|| collect_shadow_scopes(ctx.source));
            for (idx, line) in lines.iter().enumerate().take(upper).skip(start) {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") {
                    continue;
                }
                if !line.contains("props.") {
                    continue;
                }
                if let Some(col) = prop_mutation_column(line) {
                    let offset = (line.as_ptr() as usize - base) + col;
                    if props_shadowed_at(scopes, offset) {
                        continue;
                    }
                    diags.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: super::META.id.into(),
                        message: "Mutating a prop directly breaks Vue's one-way data flow. \
                             Emit an event or copy the prop into a local ref instead."
                            .into(),
                        severity: Severity::Error,
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

    /// Wrap `body` in a `<script setup>` that binds `props` to `defineProps`, so
    /// `props.X` genuinely refers to the prop object.
    fn wrap_props(body: &str) -> String {
        format!(
            "<script setup lang=\"ts\">\n\
             const props = defineProps<{{ count: number; items: string[] }}>()\n\
             {body}\n</script>\n"
        )
    }

    #[test]
    fn flags_simple_prop_assignment() {
        assert_eq!(run(&wrap_props("props.count = 5")).len(), 1);
    }

    #[test]
    fn flags_compound_assignment() {
        assert_eq!(run(&wrap_props("props.count += 1")).len(), 1);
    }

    #[test]
    fn flags_nested_path_assignment() {
        assert_eq!(run(&wrap_props("props.items.length = 0")).len(), 1);
    }

    #[test]
    fn allows_local_object_named_props_without_define_props() {
        // pipipi-pikachu/PPTist repro: `props` is a local update payload, not a
        // Vue prop — the block has no `defineProps` binding, so nothing is flagged.
        let src = wrap(
            "const props: Partial<PPTLineElement> = {}\n\
             if (line.isBroken) props.broken = midpoint\n\
             slidesStore.updateElement({ id, props })",
        );
        assert!(run(&src).is_empty());
    }

    #[test]
    fn allows_function_param_named_props_without_define_props() {
        // A `props` parameter of a local callback is an ordinary object, not the
        // component prop object.
        let src = wrap(
            "const updateElement = (props: Partial<PPTShapeElement>) => {\n\
             props.fill = color\n\
             }",
        );
        assert!(run(&src).is_empty());
    }

    #[test]
    fn flags_define_props_type_form_mutation() {
        let src = wrap("const props = defineProps<{ foo: string }>()\nprops.foo = 'x'");
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_with_defaults_props_mutation() {
        let src = wrap(
            "const props = withDefaults(defineProps<{ foo: string }>(), { foo: '' })\n\
             props.foo = 'x'",
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn does_not_flag_define_props_bound_to_other_name() {
        // `defineProps` assigned to `p`, not `props`: a `props.X = ...` here is an
        // unrelated local, not the prop object.
        let src = wrap("const p = defineProps<{ foo: string }>()\nprops.foo = 'x'");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn skips_local_shadow_of_macro_binding() {
        // The top-level `props.foo` is the macro binding (flagged); the inner
        // `props` is a fresh local that shadows it (not flagged).
        let src = wrap(
            "const props = defineProps<{ foo: string }>()\n\
             props.foo = 1\n\
             function f() {\n\
             const props = {}\n\
             props.x = 1\n\
             }",
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn preserves_identifier_prefixed_props_guard() {
        // A member path whose `props.` is prefixed by an identifier char
        // (`myprops.foo`) is a different object and must not be flagged, even when
        // the block binds `props`.
        let src = wrap("const props = defineProps<{ foo: string }>()\nmyprops.foo = 1");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn allows_emit_call() {
        assert!(run(&wrap("emit('update', x)")).is_empty());
    }

    #[test]
    fn allows_read_access() {
        // Gate open (block binds `props`): a read of `props.foo` is not a mutation.
        assert!(run(&wrap_props("const x = props.foo")).is_empty());
    }

    #[test]
    fn allows_equality_comparison() {
        // Gate open: `==`/`===` are comparisons, not assignments.
        assert!(run(&wrap_props("if (props.foo == null) { return; }")).is_empty());
        assert!(run(&wrap_props("if (props.foo === null) { return; }")).is_empty());
    }

    #[test]
    fn allows_comment_line() {
        // Gate open: a commented-out mutation is skipped.
        assert!(run(&wrap_props("// props.count = 5")).is_empty());
    }

    #[test]
    fn ignores_outside_script_setup() {
        let src = "<template><div>{{ foo }}</div></template>\n\
                   <script>\nprops.count = 5\n</script>\n";
        assert!(run(src).is_empty());
    }
}
