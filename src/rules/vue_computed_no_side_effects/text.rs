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
//!
//! Nested-function exemption: markers and `.value =` inside a function the getter
//! *returns* (an arrow or `function` expression stored as a property/element of
//! the returned value, e.g. `onClick: () => emit('x')`) are skipped. That body
//! runs when the callback is later invoked, not during getter evaluation, so it
//! is not a side effect of the computed. Only side effects in the getter's own
//! body (top-level statements, conditionals, loops that run during evaluation)
//! stay flagged.
//!
//! String/comment awareness: markers and `.value =` matches that land inside a
//! string literal, a template literal, or a comment are skipped — they are text
//! content, not executed code. A computed that builds a string of code (an
//! exporter) is pure even though its output contains `.value =`. `${...}`
//! interpolation spans inside template literals are code and stay checked.
//! Regex literals (`/.../`) are not recognized as strings — a backtick inside
//! one would mis-open a template literal; this is consistent with not
//! re-parsing TS and is vanishingly rare inside a computed body.

use crate::diagnostic::{Diagnostic, Severity};

const SIDE_EFFECT_MARKERS: &[&str] = &[
    "emit(", "console.", "fetch(", "axios.", ".post(", ".put(", ".delete(", ".patch(", "$emit(",
];

/// A boolean mask over `body`'s bytes: `true` marks a byte that is *not*
/// executable code — it is inside a string literal, a template literal, or a
/// comment. Markers and `.value =` matches landing on a `true` byte are text
/// content and must not be flagged.
///
/// Template-literal `${...}` interpolations are code (`false`), tracked with a
/// brace-depth stack so nested template literals inside interpolations are
/// handled. Escapes (`\`) inside `'`/`"`/`` ` `` strings are honored so an
/// escaped quote does not end the string early.
fn non_code_mask(body: &str) -> Vec<bool> {
    let bytes = body.as_bytes();
    let mut mask = vec![false; bytes.len()];
    // Brace depth at which each open template literal started; the literal
    // resumes (its content becomes string again) when depth returns to it.
    let mut template_stack: Vec<u32> = Vec::new();
    let mut brace_depth: u32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let in_template = template_stack.last() == Some(&brace_depth);
        if in_template {
            match c {
                b'\\' => {
                    mask[i] = true;
                    if i + 1 < bytes.len() {
                        mask[i + 1] = true;
                    }
                    i += 2;
                    continue;
                }
                b'`' => {
                    template_stack.pop();
                    i += 1;
                    continue;
                }
                b'$' if i + 1 < bytes.len() && bytes[i + 1] == b'{' => {
                    // Enter an interpolation: the `${` and its contents are code.
                    brace_depth += 1;
                    i += 2;
                    continue;
                }
                _ => {
                    mask[i] = true;
                    i += 1;
                    continue;
                }
            }
        }
        match c {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    mask[i] = true;
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                mask[i] = true;
                mask[i + 1] = true;
                i += 2;
                while i < bytes.len() {
                    if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                        mask[i] = true;
                        mask[i + 1] = true;
                        i += 2;
                        break;
                    }
                    mask[i] = true;
                    i += 1;
                }
            }
            b'\'' | b'"' => {
                let quote = c;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        mask[i] = true;
                        if i + 1 < bytes.len() {
                            mask[i + 1] = true;
                        }
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote || bytes[i] == b'\n' {
                        break;
                    }
                    mask[i] = true;
                    i += 1;
                }
                i += 1;
            }
            b'`' => {
                template_stack.push(brace_depth);
                i += 1;
            }
            b'{' => {
                brace_depth += 1;
                i += 1;
            }
            b'}' => {
                brace_depth = brace_depth.saturating_sub(1);
                i += 1;
            }
            _ => i += 1,
        }
    }
    mask
}

/// A boolean mask over `body`'s bytes: `true` marks a byte that lives inside a
/// *nested* function body — an arrow or `function` expression defined within the
/// getter. Such a function is a value the getter produces (an event handler, a
/// callback); its body runs later when that function is invoked, not during
/// getter evaluation, so a side-effect marker or `.value =` landing there is not
/// a side effect of the computed itself.
///
/// The getter is the outermost function (function-nesting depth 1); only depth
/// 2 or deeper is deferred. `code` is `non_code_mask(body)` — only code bytes
/// (not strings/comments) drive bracket and token tracking.
///
/// Function bodies are recognized two ways:
/// - **Arrow `=>`**: a braced body (`=> { ... }`) ends when its `{` closes; a
///   concise body (`=> expr`) ends at the first `,` or closing `)`/`}`/`]` at or
///   below the bracket depth where the arrow appeared (its natural expression
///   boundary).
/// - **`function` keyword**: the body is the next `{ ... }` block and ends when
///   that brace closes.
///
/// Method-shorthand callbacks (`onClick() { ... }`) are not recognized as nested
/// functions; the real-world deferred-callback shapes are arrows and `function`
/// expressions stored as property values.
///
/// Known limitation: an immediately-invoked nested function (an IIFE,
/// `(() => { ... })()`) runs synchronously during getter evaluation, but its
/// body is treated as deferred because the trailing `()` invocation is not
/// detected without re-parsing TS. This trades a vanishingly-rare false negative
/// for eliminating the common callback-as-property false positive.
fn deferred_mask(body: &str, code: &[bool]) -> Vec<bool> {
    let bytes = body.as_bytes();
    let mut mask = vec![false; bytes.len()];
    // Each active function body, in nesting order. `Brace(depth)` ends when the
    // bracket depth returns to `depth` (the `{` that opened it closes).
    // `Concise(depth)` ends at the first `,` or closing bracket at depth <=
    // `depth`.
    enum Body {
        Brace(i32),
        Concise(i32),
    }
    let mut stack: Vec<Body> = Vec::new();
    let mut depth: i32 = 0;
    // After an arrow `=>`, await the body's first non-whitespace char to choose
    // brace vs concise; holds the bracket depth at the arrow.
    let mut pending_arrow: Option<i32> = None;
    // After a `function` keyword, the next `{` opens a (always-braced) body.
    let mut expect_fn_brace = false;
    let is_code = |k: usize| code.get(k).copied() == Some(false);
    let mut i = 0;
    while i < bytes.len() {
        if !is_code(i) {
            if !stack.is_empty() {
                mask[i] = true;
            }
            i += 1;
            continue;
        }
        let c = bytes[i];
        if let Some(arrow_depth) = pending_arrow {
            if (c as char).is_whitespace() {
                if stack.len() >= 2 {
                    mask[i] = true;
                }
                i += 1;
                continue;
            }
            // First real char of the arrow body decides its shape.
            if c == b'{' {
                stack.push(Body::Brace(depth));
            } else {
                stack.push(Body::Concise(arrow_depth));
            }
            pending_arrow = None;
            // Fall through so this char is processed under the now-active body.
        }
        // Close every concise body whose boundary delimiter is this char, before
        // attributing the delimiter to the enclosing function.
        while let Some(Body::Concise(d)) = stack.last() {
            let ends_here = (matches!(c, b')' | b'}' | b']') || c == b',') && depth <= *d;
            if ends_here {
                stack.pop();
            } else {
                break;
            }
        }
        if stack.len() >= 2 {
            mask[i] = true;
        }
        match c {
            b'(' | b'[' => depth += 1,
            b'{' => {
                depth += 1;
                if expect_fn_brace {
                    stack.push(Body::Brace(depth - 1));
                    expect_fn_brace = false;
                }
            }
            b')' | b']' => depth -= 1,
            b'}' => {
                depth -= 1;
                if let Some(&Body::Brace(d)) = stack.last()
                    && depth == d
                {
                    stack.pop();
                }
            }
            b'=' if i + 1 < bytes.len() && bytes[i + 1] == b'>' && is_code(i + 1) => {
                if stack.len() >= 2 {
                    mask[i + 1] = true;
                }
                pending_arrow = Some(depth);
                i += 2;
                continue;
            }
            _ => {
                if is_function_keyword(body, code, i) {
                    expect_fn_brace = true;
                }
            }
        }
        i += 1;
    }
    mask
}

/// Whether a `function` keyword starts at byte `i` in `body` — a code-byte run
/// spelling `function` whose neighbors are not identifier characters (so
/// `myfunction`/`functions` are rejected).
fn is_function_keyword(body: &str, code: &[bool], i: usize) -> bool {
    const KW: &[u8] = b"function";
    let bytes = body.as_bytes();
    if i + KW.len() > bytes.len() || &bytes[i..i + KW.len()] != KW {
        return false;
    }
    if !(0..KW.len()).all(|k| code.get(i + k).copied() == Some(false)) {
        return false;
    }
    let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
    let after_ok = i + KW.len() >= bytes.len() || !is_ident_char(bytes[i + KW.len()]);
    before_ok && after_ok
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// Byte range `[key_start ..= end]` spanning the writable-computed `set`
/// property within `body`, or `None` when there is no such property.
///
/// Matches a `set` property KEY — a `set` whose preceding char is not part of a
/// larger identifier (`offset`/`reset`/`asset` are rejected) and whose next
/// non-whitespace char is `(` (method shorthand) or `:` (arrow / function
/// expression). From the key it scans forward tracking bracket nesting over
/// `()`, `{}` and `[]`, and ends at the first top-level (depth 0) `,` or
/// closing `}`/`)`/`]` — the boundary of the `set` property within the
/// enclosing options object — or at `body.len()` when none is found. This
/// covers both a brace body (`set() { ... }`, `set: (v) => { ... }`, where the
/// `{ ... }` is consumed as nested depth) and a concise arrow without braces
/// (`set: v => emit('x', v)`, whose argument comma sits at depth > 0 and is
/// ignored). String-literal awareness is out of scope: a bracket/comma inside a
/// string in the params or body could skew the match.
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
        let mut depth: i32 = 0;
        let mut j = key_end;
        while j < bytes.len() {
            match bytes[j] {
                b'{' | b'(' | b'[' => depth += 1,
                b'}' | b')' | b']' => {
                    if depth == 0 {
                        return Some((key_start, j));
                    }
                    depth -= 1;
                }
                b',' if depth == 0 => return Some((key_start, j)),
                _ => {}
            }
            j += 1;
        }
        return Some((key_start, bytes.len()));
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
        let mask = non_code_mask(body);
        let deferred = deferred_mask(body, &mask);
        let mut line_start = 0usize;
        // Split on '\n' (not `.lines()`) so a trailing '\r' stays inside the
        // segment and `seg.len() + 1` advances the byte cursor exactly — the
        // cursor must stay aligned with `mask`'s byte offsets. `\r` only ever
        // trails a line, so it never shifts a marker/assignment match offset.
        for (line_off, line) in body.split('\n').enumerate() {
            let cur_start = line_start;
            let cur_end = cur_start + line.len();
            line_start += line.len() + 1; // +1 for the consumed '\n'
            if let Some((set_open, set_close)) = set_range {
                // Skip a line whose span overlaps the `set` block range — covers
                // both a single-line `set(v) { ... }` and a multi-line body.
                if cur_start <= set_close && cur_end > set_open {
                    continue;
                }
            }
            // A marker/assignment is a synchronous side effect only when it
            // starts on a code byte (not inside a string, template literal, or
            // comment) AND is not inside a nested function body — a callback the
            // getter returns runs later, not during evaluation.
            let is_code =
                |off_in_line: usize| !mask[cur_start + off_in_line] && !deferred[cur_start + off_in_line];
            for marker in SIDE_EFFECT_MARKERS {
                if line.match_indices(marker).any(|(off, _)| is_code(off)) {
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
            const VALUE_ASSIGN: &str = ".value =";
            let line_bytes = line.as_bytes();
            let assigns = line.match_indices(VALUE_ASSIGN).any(|(off, _)| {
                // Reject `.value ==` / `.value ===` (comparison, not assignment).
                is_code(off) && line_bytes.get(off + VALUE_ASSIGN.len()) != Some(&b'=')
            });
            if assigns {
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

    #[test]
    fn allows_emit_in_concise_arrow_setter() {
        let sfc = "<script setup>\nconst value = computed({\n  get: () => props.modelValue,\n  set: value => emit('update:modelValue', value),\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_emit_in_parenthesized_concise_arrow_setter() {
        let sfc = "<script setup>\nconst value = computed({\n  get: () => props.modelValue,\n  set: (value) => emit('x', value),\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_emit_in_brace_arrow_setter() {
        let sfc = "<script setup>\nconst value = computed({\n  get: () => props.modelValue,\n  set: (v) => { emit('x', v) },\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_emit_in_method_shorthand_setter() {
        let sfc = "<script setup>\nconst value = computed({\n  get() { return props.modelValue },\n  set(v) { emit('x', v) },\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_emit_in_getter_with_concise_arrow_setter() {
        let sfc = "<script setup>\nconst value = computed({\n  get: () => emit('x'),\n  set: v => {},\n})\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        // The diagnostic is the getter emit (line 3), not the setter.
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn flags_emit_in_readonly_computed() {
        let sfc = "<script setup>\nconst c = computed(() => emit('x'))\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_value_assign_inside_template_literal() {
        // The computed builds a string of Vue code for display; `.value =` is
        // string content, not an executed assignment. (Issue #4741)
        let sfc = "<script setup>\nconst layoutExport = computed(() => {\n  let code = `toggle () {\n    leftDrawerOpen.value = !leftDrawerOpen.value\n  }`\n  return code\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_marker_inside_template_literal() {
        let sfc = "<script setup>\nconst c = computed(() => {\n  return `call emit('x') here`\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_value_assign_inside_template_interpolation() {
        // `${...}` is executed code, so a real assignment there is a side effect.
        let sfc = "<script setup>\nconst c = computed(() => {\n  return `${(other.value = 2)}`\n})\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_value_assign_in_string_literal() {
        let sfc = "<script setup>\nconst c = computed(() => 'x.value = 1')\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_value_assign_in_comment() {
        let sfc = "<script setup>\nconst c = computed(() => {\n  // other.value = 2\n  return 1\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_emit_in_nested_arrow_callback() {
        // Issue #4892: `emit(...)` inside arrow callbacks stored as object
        // properties are deferred — invoked later by the consumer, not during
        // getter evaluation. The getter returns a config object and is pure.
        let sfc = "<script setup>\nconst c = useScrollable(computed(() => ({\n  ...reactive({ direction }),\n  onDragStart: (data) => emit('onDragStart', data),\n  onDragEnd: (data) => emit('onDragEnd', data),\n  onScroll: (data) => emit('onScroll', data),\n})))\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_value_assign_in_nested_arrow_callback() {
        // Issue #4887: `.value =` inside nested arrow callbacks (event handlers
        // returned in a config array) are deferred, not getter side effects.
        let sfc = "<script setup>\nconst actions = computed(() => [\n  {\n    onClick: async () => {\n      await navigator.clipboard.writeText('x')\n      copied.value = true\n      copied.value = false\n    },\n  },\n  {\n    onClick: () => {\n      showCode.value = !showCode.value\n    },\n  },\n])\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_emit_in_nested_function_expression() {
        let sfc = "<script setup>\nconst c = computed(() => ({\n  handler: function (e) { emit('x', e) },\n}))\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_emit_directly_in_getter_with_nested_callback_sibling() {
        // A real side effect in the getter body is still flagged even when the
        // getter also returns nested callbacks.
        let sfc = "<script setup>\nconst c = computed(() => {\n  emit('side')\n  return { onClick: () => emit('deferred') }\n})\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        // Only the synchronous getter emit (line 3), not the nested one (line 4).
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn flags_value_assign_directly_in_getter_with_nested_callback() {
        let sfc = "<script setup>\nconst c = computed(() => {\n  other.value = 1\n  return { onClick: () => { x.value = 2 } }\n})\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn iife_in_getter_is_not_flagged_known_limitation() {
        // An IIFE runs synchronously during getter evaluation, so its `emit` is
        // a real side effect — but the trailing `()` invocation is not detected
        // without re-parsing TS, so the body is treated as deferred. Documented
        // as a deliberate false negative (see `deferred_mask` docblock): the
        // tradeoff eliminates the common callback-as-property false positive.
        let sfc = "<script setup>\nconst c = computed(() => {\n  (() => { emit('x') })()\n  return 1\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn crlf_keeps_mask_aligned_with_real_assignment() {
        // CRLF terminators must not desync the byte cursor from the mask. Two
        // template-literal marker lines precede the real one: if the cursor
        // miscounts `\r\n` as one byte, the accumulated drift unmasks a
        // templated `emit(` and produces extra false positives. Only the
        // executed `emit(z)` on line 5 must be flagged.
        let sfc = "<script setup>\r\nconst c = computed(() => {\r\n  const a = `emit(x)`\r\n  const b = `emit(y)`\r\n  emit(z)\r\n  return a + b\r\n})\r\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 5);
    }
}
