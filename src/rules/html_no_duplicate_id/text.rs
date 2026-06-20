//! html-no-duplicate-id — Vue/HTML text backend.
//!
//! Scans the `<template>` block for static `id="..."` / `id='...'` attributes
//! and flags any value that appears more than once. Dynamic bindings
//! (`:id="foo"`, `v-bind:id="foo"`) are ignored because their runtime value
//! is unknown at lint time.
//!
//! A value is not flagged when every one of its occurrences is rendered under
//! a mutually-exclusive conditional: the id-carrying element, or an enclosing
//! ancestor, holds a `v-if` / `v-else-if` / `v-else` directive, and all those
//! guarding elements sit at the same nesting depth (sibling conditional
//! branches). Only one such branch renders at a time, so the id is never
//! duplicated in the live DOM. `v-show` (toggles display, stays in the DOM)
//! and `v-for` (can render many at once) do not count as guards.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{
    attr_value, extract_elements, is_vue_file, mask_html_comments,
};

#[derive(Debug)]
pub struct Check;

/// One static-id occurrence: its source line and the conditional branch
/// guarding it, if any.
struct Occurrence {
    line: usize,
    guard: Option<Guard>,
}

/// The conditional branch an id occurrence renders under. `indent` is the
/// nesting depth of the guarding element; only same-depth guards are sibling
/// branches that can be mutually exclusive.
struct Guard {
    indent: usize,
    kind: GuardKind,
}

enum GuardKind {
    /// `v-else-if` / `v-else`: always exclusive with the `v-if` it follows.
    Else,
    /// `v-if="EXPR === LITERAL"` — exclusive with sibling branches that test
    /// the same `EXPR` (`lhs`) against a different `literal`.
    Equality { lhs: String, literal: String },
    /// Any other `v-if="..."`: two such conditions may both hold, so a pair of
    /// them is not assumed mutually exclusive.
    Other,
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let masked = mask_html_comments(ctx.source);
        let lines: Vec<&str> = masked.lines().collect();

        // Group every static-id occurrence by value, preserving source order.
        let mut groups: Vec<(String, Vec<Occurrence>)> = Vec::new();
        for elem in extract_elements(ctx.source) {
            // Only consider static `id="..."` — not `:id` or `v-bind:id`.
            let Some(value) = static_id_value(elem.attrs) else {
                continue;
            };
            if value.is_empty() {
                continue;
            }
            let occ = Occurrence {
                line: elem.line,
                guard: conditional_guard(&lines, elem.line),
            };
            match groups.iter_mut().find(|(v, _)| v == value) {
                Some((_, occs)) => occs.push(occ),
                None => groups.push((value.to_string(), vec![occ])),
            }
        }

        let mut diagnostics = Vec::new();
        for (value, occs) in &groups {
            if occs.len() < 2 || is_mutually_exclusive(occs) {
                continue;
            }
            // Flag every occurrence past the first (occurrences 2..N).
            for occ in &occs[1..] {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: occ.line,
                    column: 1,
                    rule_id: "html-no-duplicate-id".into(),
                    message: format!("Duplicate id `{value}` in the same file."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

/// Whether all occurrences render under mutually-exclusive sibling branches, so
/// at most one carries the id into the live DOM. Every occurrence must be
/// guarded, all guards must sit at the same nesting depth (siblings), and the
/// branches must be provably exclusive — either a `v-if` / `v-else-if` /
/// `v-else` chain, or `v-if` equality tests of one discriminant against
/// distinct literals.
fn is_mutually_exclusive(occs: &[Occurrence]) -> bool {
    let guards: Option<Vec<&Guard>> = occs.iter().map(|o| o.guard.as_ref()).collect();
    let Some(guards) = guards else {
        return false;
    };
    let Some(first) = guards.first() else {
        return false;
    };
    if guards.iter().any(|g| g.indent != first.indent) {
        return false;
    }

    // A v-if/v-else-if/v-else chain: any `Else` branch is exclusive with the
    // others, and at most one branch can be a plain (non-equality) condition.
    let has_else = guards.iter().any(|g| matches!(g.kind, GuardKind::Else));
    if has_else {
        let non_else_plain = guards
            .iter()
            .filter(|g| !matches!(g.kind, GuardKind::Else))
            .count();
        return non_else_plain <= 1;
    }

    // Otherwise require every branch to be an equality test on the same
    // discriminant (`lhs`) against pairwise-distinct literals.
    let mut discriminant: Option<&str> = None;
    let mut literals: Vec<&str> = Vec::new();
    for g in &guards {
        let GuardKind::Equality { lhs, literal } = &g.kind else {
            return false;
        };
        match discriminant {
            Some(prev) if prev != lhs.as_str() => return false,
            _ => discriminant = Some(lhs),
        }
        if literals.contains(&literal.as_str()) {
            return false;
        }
        literals.push(literal);
    }
    true
}

/// Find the conditional branch guarding the id on `line` (1-based): the
/// element's own opening tag, or the nearest enclosing ancestor.
///
/// Ancestors are approximated by indentation, the established idiom for these
/// Vue text backends: walking upward, a line opening a tag at strictly smaller
/// indentation is an enclosing ancestor. Returns `None` when neither the
/// element nor any ancestor carries a conditional directive.
fn conditional_guard(lines: &[&str], line: usize) -> Option<Guard> {
    let idx = line.checked_sub(1)?;
    let self_line = lines.get(idx)?;
    let mut min_indent = leading_whitespace_width(self_line);
    if let Some(kind) = directive_kind(self_line) {
        return Some(Guard {
            indent: min_indent,
            kind,
        });
    }
    // Scan upward through strictly-shallower opening tags (the ancestor chain).
    for &prev in lines[..idx].iter().rev() {
        let indent = leading_whitespace_width(prev);
        if indent >= min_indent || !opens_tag(prev) {
            continue;
        }
        if let Some(kind) = directive_kind(prev) {
            return Some(Guard { indent, kind });
        }
        min_indent = indent;
        if indent == 0 {
            break;
        }
    }
    None
}

/// Count of leading whitespace characters (spaces and tabs alike).
fn leading_whitespace_width(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

/// Whether the trimmed line begins an opening tag (`<tag`, not `</close>`).
fn opens_tag(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed
        .strip_prefix('<')
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_alphabetic()))
}

/// Classify the conditional-rendering directive on `line`, if any. `v-show`
/// (stays in the DOM) and `v-for` (renders many at once) are excluded — they do
/// not make an id mutually exclusive.
fn directive_kind(line: &str) -> Option<GuardKind> {
    // `v-else` and `v-else-if` are both exclusive branches of their `v-if`.
    if contains_directive(line, "v-else") {
        return Some(GuardKind::Else);
    }
    let cond = v_if_condition(line)?;
    Some(match equality_discriminant(cond) {
        Some((lhs, literal)) => GuardKind::Equality { lhs, literal },
        None => GuardKind::Other,
    })
}

/// Extract the condition of a `v-if="..."` directive on the line.
fn v_if_condition(line: &str) -> Option<&str> {
    if !contains_directive(line, "v-if") {
        return None;
    }
    for quote in ['"', '\''] {
        let marker = format!("v-if={quote}");
        if let Some(pos) = line.find(&marker) {
            let rest = &line[pos + marker.len()..];
            if let Some(end) = rest.find(quote) {
                return Some(rest[..end].trim());
            }
        }
    }
    None
}

/// For a `cond` of the form `EXPR === LITERAL` / `EXPR == LITERAL` (string or
/// numeric literal), return `(EXPR, LITERAL)`. Returns `None` for any other
/// condition: a non-equality operator (`!==`, `>=`, …), a right side that is
/// not a literal, or a compound expression with more than one comparison.
fn equality_discriminant(cond: &str) -> Option<(String, String)> {
    let op = cond.find("===").map(|p| (p, 3)).or_else(|| {
        let p = cond.find("==")?;
        Some((p, 2))
    })?;
    let (pos, op_len) = op;
    let lhs = cond[..pos].trim();
    let rhs = cond[pos + op_len..].trim();
    // Reject inequality / relational operators (`!==`, `>=`, `<=`) that end in
    // `==`, and any second comparison in either operand.
    if lhs.ends_with(['!', '<', '>', '=']) || rhs.contains(['=', '<', '>', '!']) {
        return None;
    }
    if lhs.is_empty() || !is_literal(rhs) {
        return None;
    }
    Some((lhs.to_string(), rhs.to_string()))
}

/// Whether `s` is a string or numeric literal (the right side of a discriminant
/// equality), not an identifier or expression.
fn is_literal(s: &str) -> bool {
    let is_quoted = (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
        || (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('`') && s.ends_with('`') && s.len() >= 2);
    is_quoted || s.parse::<f64>().is_ok()
}

/// Whether `directive` appears as a standalone attribute token in `line`: it
/// starts at a tag/whitespace boundary (never inside a quoted value) and ends
/// at `=`, whitespace, `>`, `/`, or end of line. `v-else` also matches the
/// `v-else-if` token, whose trailing `-` is accepted as a continuation.
fn contains_directive(line: &str, directive: &str) -> bool {
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(rel) = line[from..].find(directive) {
        let start = from + rel;
        let before_ok = start == 0 || matches!(bytes[start - 1], b' ' | b'\t' | b'<');
        let after = start + directive.len();
        let after_ok = match bytes.get(after) {
            None => true,
            // `v-else` legitimately precedes `-if`; otherwise require a boundary.
            Some(&b) => matches!(b, b'=' | b' ' | b'\t' | b'>' | b'/' | b'-'),
        };
        if before_ok && after_ok {
            return true;
        }
        from = start + directive.len();
    }
    false
}

/// Return the value of a static `id` attribute, or `None` if the element has
/// no static id (e.g., only `:id` / `v-bind:id`, or no id at all).
fn static_id_value(attrs: &str) -> Option<&str> {
    // `attr_value` already handles both single and double quotes. But we
    // must not match `:id=` or `v-bind:id=` — ensure `id=` is not preceded
    // by `:` or `-`.
    let value = attr_value(attrs, "id")?;
    // Re-verify by scanning for the actual position to guard against the
    // case where `id=` follows `:` (dynamic binding).
    if has_static_id(attrs) {
        Some(value)
    } else {
        None
    }
}

fn has_static_id(attrs: &str) -> bool {
    let bytes = attrs.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 2 < len {
        if bytes[i] == b'i' && bytes[i + 1] == b'd' && bytes[i + 2] == b'=' {
            // Check what precedes `id`
            let ok_prefix = i == 0 || bytes[i - 1].is_ascii_whitespace();
            if ok_prefix {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    fn run_named(name: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(name), source))
    }

    #[test]
    fn flags_duplicate_id() {
        let source =
            "<template>\n  <div id=\"foo\"></div>\n  <span id=\"foo\"></span>\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foo"));
    }

    #[test]
    fn allows_unique_ids() {
        let source =
            "<template>\n  <div id=\"foo\"></div>\n  <span id=\"bar\"></span>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_duplicate_id_single_quotes() {
        let source = "<template>\n  <div id='x'></div>\n  <p id='x'></p>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn ignores_dynamic_id_binding() {
        // `:id` and `v-bind:id` are dynamic — can't compare at lint time.
        let source =
            "<template>\n  <div :id=\"foo\"></div>\n  <span v-bind:id=\"foo\"></span>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_non_vue_file() {
        let source =
            "<template>\n  <div id=\"foo\"></div>\n  <span id=\"foo\"></span>\n</template>";
        assert!(run_named("component.tsx", source).is_empty());
    }

    #[test]
    fn different_files_dont_cross_contaminate() {
        // Each file is checked independently; reusing ids across files is fine.
        let source_a = "<template>\n  <div id=\"foo\"></div>\n</template>";
        let source_b = "<template>\n  <div id=\"foo\"></div>\n</template>";
        assert!(run_named("a.vue", source_a).is_empty());
        assert!(run_named("b.vue", source_b).is_empty());
    }

    #[test]
    fn flags_triple_duplicate() {
        let source = "<template>\n  <div id=\"x\"></div>\n  <p id=\"x\"></p>\n  <span id=\"x\"></span>\n</template>";
        // Two duplicates flagged (occurrences 2 and 3).
        assert_eq!(run(source).len(), 2);
    }

    #[test]
    fn allows_same_id_under_mutually_exclusive_v_if_ancestors() {
        // Regression #4992 (vue-advanced-chat Loader.vue): the same id sits on a
        // child of each of several sibling `<slot v-if="...">` branches. Exactly
        // one branch renders, so the id never duplicates in the live DOM.
        let source = "<template>\n\
            \x20 <slot v-if=\"type === 'rooms'\" name=\"a\">\n\
            \x20   <div id=\"vac-circle\" />\n\
            \x20 </slot>\n\
            \x20 <slot v-if=\"type === 'infinite-rooms'\" name=\"b\">\n\
            \x20   <div id=\"vac-circle\" />\n\
            \x20 </slot>\n\
            \x20 <slot v-if=\"type === 'message-file'\" name=\"c\">\n\
            \x20   <div id=\"vac-circle\" />\n\
            \x20 </slot>\n\
            </template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_same_id_on_v_if_v_else_siblings() {
        // The directive directly on the id-carrying sibling elements.
        let source = "<template>\n\
            \x20 <div v-if=\"open\" id=\"panel\" />\n\
            \x20 <div v-else id=\"panel\" />\n\
            </template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_same_id_on_v_show_siblings() {
        // `v-show` only toggles display; both elements stay in the DOM, so the
        // id genuinely duplicates and must still be flagged.
        let source = "<template>\n\
            \x20 <div v-show=\"open\" id=\"panel\" />\n\
            \x20 <div v-show=\"!open\" id=\"panel\" />\n\
            </template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_same_id_when_only_one_occurrence_is_guarded() {
        // One branch is conditional, the other is unconditional — they can
        // co-render, so the id can duplicate.
        let source = "<template>\n\
            \x20 <div v-if=\"open\" id=\"panel\" />\n\
            \x20 <div id=\"panel\" />\n\
            </template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_same_id_on_v_if_ancestors_at_different_depths() {
        // The guarding `v-if` elements are at different nesting depths, so they
        // are not sibling branches and could both render — still flagged.
        let source = "<template>\n\
            \x20 <section v-if=\"a\">\n\
            \x20   <div id=\"x\" />\n\
            \x20 </section>\n\
            \x20 <div v-if=\"b\">\n\
            \x20   <span>\n\
            \x20     <div id=\"x\" />\n\
            \x20   </span>\n\
            \x20 </div>\n\
            </template>";
        assert_eq!(run(source).len(), 1);
    }
}
