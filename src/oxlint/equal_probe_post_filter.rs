//! Post-filter for `no-unnecessary-type-parameters` false positives on the
//! type-challenges `Equal<X, Y>` probe idiom.
//!
//! The probe wraps each side in a function type `<T>() => T extends X ? 1 : 2`.
//! The `<T>` generic is load-bearing: it forces TypeScript to compare the two
//! conditionals structurally, which only collapses identically when `X` and
//! `Y` are the same type. Without `<T>` the function type degenerates to
//! `() => 1 | 2` and the `Equal` check breaks down to one-way `Extends`.
//!
//! tsgolint reports `<T>` as "used only once" — technically correct but the
//! generic is required for the idiom to work. Drop the diagnostic when the
//! same line (or, for multi-line probes, the next few lines) carries the
//! canonical pattern: a conditional return whose two branches are unit
//! literals (numeric, string, boolean, null, or undefined).
//!
//! Additionally, tsgolint sometimes misses the second occurrence of a type
//! parameter when a multi-line function signature splits parameters across
//! lines. In that case the diagnostic is dropped when the parameter name
//! appears on at least two separate lines after the declaration line.

use crate::diagnostic::Diagnostic;
use rustc_hash::FxHashMap;
use std::path::PathBuf;

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    let mut file_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "no-unnecessary-type-parameters" {
            return true;
        }
        let entry = file_cache
            .entry(d.path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(d.path.as_ref()).ok());
        let Some(src) = entry.as_deref() else {
            return true;
        };
        !is_equal_probe_fp(src, d.line) && !is_multiline_param_fp(src, d.line)
    });
}

/// True when the diagnostic line (or a following continuation line for
/// multi-line probes) carries a function-type conditional whose branches are
/// both unit literals — the canonical type-challenges Equal probe.
fn is_equal_probe_fp(src: &str, line_1based: usize) -> bool {
    if line_1based == 0 {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    let Some(&line) = lines.get(line_1based - 1) else {
        return false;
    };

    // Single-line case: `<T>() => T extends X ? 1 : 2` all on one line.
    if has_unit_conditional(line) {
        return true;
    }

    // Multi-line probe: `<T>()` is on the diagnostic line but the conditional
    // `T extends X ? 1 : 2` is on the next line(s). Check up to 3 more lines.
    if has_generic_arrow_fn(line) {
        for next_line in lines.iter().skip(line_1based).take(3) {
            if has_unit_conditional_expr(next_line) {
                return true;
            }
        }
    }

    false
}

/// True when the diagnostic fires on a multi-line function/type signature
/// where tsgolint only sees the first occurrence of the type parameter — a
/// known tsgolint limitation when parameters span multiple lines. The check
/// suppresses the diagnostic when the type parameter name appears on at least
/// two separate lines after the declaration line, making it clear the
/// parameter is actually used more than once.
fn is_multiline_param_fp(src: &str, line_1based: usize) -> bool {
    if line_1based == 0 {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    let Some(&decl_line) = lines.get(line_1based - 1) else {
        return false;
    };

    // Extract the first type parameter name from the declaration line.
    let Some(param_name) = extract_type_param_name(decl_line) else {
        return false;
    };

    // Walk the lines following the declaration, counting distinct lines that
    // mention `param_name` as a whole identifier. Stop when the parameter
    // list closes (paren_depth < 0 after processing a line) or when a `{`
    // is seen at depth 0 (function body opens), or after 15 lines.
    let mut paren_depth: i32 = 0;
    // Account for any `(` already on the declaration line.
    for b in decl_line.bytes() {
        match b {
            b'(' => paren_depth += 1,
            b')' => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    break;
                }
            }
            _ => {}
        }
    }

    let mut lines_with_param: usize = 0;
    for next_line in lines.iter().skip(line_1based).take(15) {
        if contains_word(next_line, &param_name) {
            lines_with_param += 1;
            if lines_with_param >= 2 {
                return true;
            }
        }

        let mut hit_body = false;
        for b in next_line.bytes() {
            match b {
                b'(' => paren_depth += 1,
                b')' => {
                    paren_depth -= 1;
                    if paren_depth < 0 {
                        break;
                    }
                }
                b'{' if paren_depth <= 0 => {
                    hit_body = true;
                    break;
                }
                _ => {}
            }
        }
        if hit_body || paren_depth < 0 {
            break;
        }
    }

    false
}

fn has_unit_conditional(line: &str) -> bool {
    // The Equal probe is specifically a function-generic arrow: `<T>()`.
    // Without this guard a nested conditional type like
    // `type A<T> = T extends (U extends V ? 1 : 2) ? 3 : 4` would match the
    // unit-conditional check and suppress a legitimate diagnostic on `T`.
    if !has_generic_arrow_fn(line) {
        return false;
    }
    has_unit_conditional_expr(line)
}

/// Check for `extends X ? <unit> : <unit>` without requiring `<T>()` on the
/// same line. Used for continuation lines in multi-line probe detection.
fn has_unit_conditional_expr(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 7 <= bytes.len() {
        if &bytes[i..i + 7] == b"extends"
            && (i == 0 || !is_ident_byte(bytes[i - 1]))
            && (i + 7 == bytes.len() || !is_ident_byte(bytes[i + 7]))
        {
            // After `extends`, find the next `?` (not `??`) on this line, then `:`.
            if let Some((q_pos, c_pos)) = find_ternary_after(line, i + 7) {
                let arm1 = line[q_pos + 1..c_pos].trim();
                // For the else arm, stop at a closing `)` or end-of-line.
                let rest = &line[c_pos + 1..];
                let end = rest
                    .find(|c: char| c == ')' || c == ',' || c == ';')
                    .unwrap_or(rest.len());
                let arm2 = rest[..end].trim();
                if is_unit_literal(arm1) && is_unit_literal(arm2) {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

/// True when the line contains a generic arrow-function type pattern: `<T>()`,
/// `<T extends X>()`, `<T, U>()`, etc. This is the fingerprint of the
/// type-challenges Equal probe: `<T>() => T extends X ? 1 : 2`.
///
/// The check accepts any non-empty type parameter list between `<` and `>`,
/// including constraints (`extends Something`) and multiple parameters, as
/// long as the immediately following characters are `()`.
fn has_generic_arrow_fn(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Look for `<`.
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let start = i + 1;
        // Require at least one identifier-start char after `<`.
        if start >= bytes.len() || !is_ident_start(bytes[start]) {
            i += 1;
            continue;
        }
        // Find the matching `>` that closes this type-parameter list,
        // skipping nested angle brackets (e.g. `Array<T>`).
        let Some(close) = find_matching_angle(bytes, i + 1) else {
            i += 1;
            continue;
        };
        // Must be followed immediately by `(`.
        let after_gt = close + 1;
        if after_gt >= bytes.len() || bytes[after_gt] != b'(' {
            i += 1;
            continue;
        }
        // Must be followed immediately by `)`.
        let after_open = after_gt + 1;
        if after_open >= bytes.len() || bytes[after_open] != b')' {
            i += 1;
            continue;
        }
        return true;
    }
    false
}

/// Find the index of the `>` that matches the opening `<` whose interior
/// starts at `start`. Tracks nested angle brackets.
fn find_matching_angle(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth: i32 = 1;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => {
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

/// Extract the first type-parameter name from a declaration line.
/// Finds the first `<Ident` sequence and returns the identifier.
fn extract_type_param_name(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let start = i + 1;
            if start < bytes.len() && is_ident_start(bytes[start]) {
                let mut j = start;
                while j < bytes.len() && is_ident_byte(bytes[j]) {
                    j += 1;
                }
                if j > start {
                    return Some(String::from_utf8_lossy(&bytes[start..j]).into_owned());
                }
            }
        }
        i += 1;
    }
    None
}

/// True when `word` appears in `line` as a whole identifier (surrounded by
/// non-identifier characters or line boundaries).
fn contains_word(line: &str, word: &str) -> bool {
    let word_bytes = word.as_bytes();
    let line_bytes = line.as_bytes();
    if word_bytes.is_empty() || line_bytes.len() < word_bytes.len() {
        return false;
    }
    let mut i = 0;
    while i + word_bytes.len() <= line_bytes.len() {
        if &line_bytes[i..i + word_bytes.len()] == word_bytes {
            let before_ok = i == 0 || !is_ident_byte(line_bytes[i - 1]);
            let after_pos = i + word_bytes.len();
            let after_ok = after_pos >= line_bytes.len() || !is_ident_byte(line_bytes[after_pos]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Find positions of `?` and matching `:` for a ternary on the line starting
/// at `from`. Skips `??` (nullish coalescing) and obvious non-conditional
/// uses. Returns `(question_pos, colon_pos)`.
fn find_ternary_after(line: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut depth_paren: i32 = 0;
    let mut depth_angle: i32 = 0;
    let mut i = from;
    let mut q_pos: Option<usize> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'(' => depth_paren += 1,
            b')' => depth_paren -= 1,
            b'<' => depth_angle += 1,
            b'>' => depth_angle -= 1,
            b'?' if depth_paren == 0 && depth_angle == 0 => {
                // Skip `??` (nullish coalescing).
                if i + 1 < bytes.len() && bytes[i + 1] == b'?' {
                    i += 2;
                    continue;
                }
                q_pos = Some(i);
                break;
            }
            _ => {}
        }
        i += 1;
    }
    let q = q_pos?;
    // Find matching `:` after `?` at depth 0.
    let mut depth_paren: i32 = 0;
    let mut depth_angle: i32 = 0;
    let mut j = q + 1;
    while j < bytes.len() {
        let b = bytes[j];
        match b {
            b'(' => depth_paren += 1,
            b')' => depth_paren -= 1,
            b'<' => depth_angle += 1,
            b'>' => depth_angle -= 1,
            b':' if depth_paren == 0 && depth_angle == 0 => return Some((q, j)),
            _ => {}
        }
        j += 1;
    }
    None
}

/// True when `s` is a TS unit literal type — numeric, string, boolean, null,
/// or undefined. The Equal probe canonically uses `1` and `2`.
fn is_unit_literal(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    matches!(s, "true" | "false" | "null" | "undefined")
        || is_numeric_literal(s)
        || is_string_literal(s)
}

fn is_numeric_literal(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let start = if bytes[0] == b'-' || bytes[0] == b'+' { 1 } else { 0 };
    let rest = &s[start..];
    !rest.is_empty()
        && rest.chars().all(|c| c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '_')
}

fn is_string_literal(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
            || (bytes[0] == b'`' && bytes[bytes.len() - 1] == b'`'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;
    use std::borrow::Cow;
    use std::path::{Path, PathBuf};

    fn fake_diag(path: &Path, line: usize, rule: &'static str) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed(rule),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        }
    }

    fn write_temp(name: &str, src: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("comply-equal-probe-post-filter-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, src).unwrap();
        path
    }

    fn line_of(src: &str, needle: &str) -> usize {
        src.lines()
            .enumerate()
            .find(|(_, l)| l.contains(needle))
            .map(|(i, _)| i + 1)
            .expect("needle not in source")
    }

    #[test]
    fn drops_equal_probe_identity() {
        let src = "type IdentityProbe<X> = <T>() => T extends X ? 1 : 2;\n";
        let path = write_temp("identity_probe.ts", src);
        let mut diags = vec![fake_diag(&path, 1, "no-unnecessary-type-parameters")];
        apply(&mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn drops_equal_inline_probe() {
        let src = "export type Equal<X, Y> =\n  (<T>() => T extends X ? 1 : 2) extends\n  (<T>() => T extends Y ? 1 : 2) ? true : false;\n";
        let path = write_temp("equal_inline.ts", src);
        let l1 = line_of(src, "T extends X ? 1 : 2");
        let l2 = line_of(src, "T extends Y ? 1 : 2");
        let mut diags = vec![
            fake_diag(&path, l1, "no-unnecessary-type-parameters"),
            fake_diag(&path, l2, "no-unnecessary-type-parameters"),
        ];
        apply(&mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn drops_equal_probe_with_boolean_units() {
        let src = "type P<X> = <T>() => T extends X ? true : false;\n";
        let path = write_temp("bool_units.ts", src);
        let mut diags = vec![fake_diag(&path, 1, "no-unnecessary-type-parameters")];
        apply(&mut diags);
        assert!(diags.is_empty());
    }

    // ── New: adjacent probe patterns ──────────────────────────────────────────

    #[test]
    fn drops_probe_with_constrained_type_param() {
        // `<T extends unknown>()` — has_generic_arrow_fn previously required
        // exactly `<Ident>()` with no constraint.
        let src = "type P<X> = <T extends unknown>() => T extends X ? 1 : 2;\n";
        let path = write_temp("constrained_probe.ts", src);
        let mut diags = vec![fake_diag(&path, 1, "no-unnecessary-type-parameters")];
        apply(&mut diags);
        assert!(diags.is_empty(), "constrained probe must be suppressed");
    }

    #[test]
    fn drops_probe_with_two_type_params() {
        // `<T, U>()` — previously only `<Ident>()` (single bare param) matched.
        let src = "type P<X, Y> = <T, U>() => T extends X ? 1 : 2;\n";
        let path = write_temp("two_param_probe.ts", src);
        let line = line_of(src, "<T, U>()");
        let mut diags = vec![fake_diag(&path, line, "no-unnecessary-type-parameters")];
        apply(&mut diags);
        assert!(diags.is_empty(), "two-param probe must be suppressed");
    }

    #[test]
    fn drops_multi_line_probe() {
        // The `<T>()` is on one line, `T extends X ? 1 : 2` is on the next.
        // Use distinct type params (<T> and <U>) so `line_of` finds each line
        // uniquely — `line_of` matches single-line substrings.
        let src = concat!(
            "export type Equal<X, Y> =\n",
            "  (<T>()\n",
            "    => T extends X ? 1 : 2\n",
            "  ) extends\n",
            "  (<U>()\n",
            "    => U extends Y ? 1 : 2\n",
            "  ) ? true : false;\n",
        );
        let path = write_temp("multiline_probe.ts", src);
        // Diagnostics fire on the lines with `<T>()` and `<U>()`.
        let l1 = line_of(src, "<T>()");
        let l2 = line_of(src, "<U>()");
        let mut diags = vec![
            fake_diag(&path, l1, "no-unnecessary-type-parameters"),
            fake_diag(&path, l2, "no-unnecessary-type-parameters"),
        ];
        apply(&mut diags);
        assert!(diags.is_empty(), "multi-line probe must be suppressed");
    }

    #[test]
    fn drops_multiline_signature_fp() {
        // Mirrors the amadeo `useListSearchSync` pattern: tsgolint only sees
        // the first occurrence of `TSearch` even though it appears in two
        // separate parameter type annotations across multiple lines.
        let src = concat!(
            "export function useListSearchSync<TSearch extends ListRouteSearch>(\n",
            "  routeApi: ListRouteApi<TSearch>,\n",
            "  { filterKeys }: UseListSearchSyncOptions<TSearch>,\n",
            "): void {}\n",
        );
        let path = write_temp("multiline_sig.ts", src);
        let line = line_of(src, "useListSearchSync");
        let mut diags = vec![fake_diag(&path, line, "no-unnecessary-type-parameters")];
        apply(&mut diags);
        assert!(
            diags.is_empty(),
            "multi-line signature with param in two separate args must be suppressed"
        );
    }

    // ── Existing negative tests — must still fire ─────────────────────────────

    #[test]
    fn keeps_real_unused_type_parameter() {
        // `T` is genuinely unused — not a probe.
        let src = "function f<T>(x: number): string { return ''; }\n";
        let path = write_temp("real_unused.ts", src);
        let mut diags = vec![fake_diag(&path, 1, "no-unnecessary-type-parameters")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn keeps_conditional_with_non_unit_branch() {
        // Conditional returns a real shape — keep the diagnostic.
        let src = "type F<T> = T extends string ? { kind: 'str' } : { kind: 'other' };\n";
        let path = write_temp("non_unit_branch.ts", src);
        let mut diags = vec![fake_diag(&path, 1, "no-unnecessary-type-parameters")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_touch_other_rules() {
        let src = "type P<X> = <T>() => T extends X ? 1 : 2;\n";
        let path = write_temp("other_rule.ts", src);
        let mut diags = vec![
            fake_diag(&path, 1, "no-unnecessary-type-parameters"),
            fake_diag(&path, 1, "no-explicit-any"),
        ];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }

    #[test]
    fn keeps_diagnostic_on_unreadable_file() {
        let nonexistent = std::env::temp_dir().join("comply-equal-probe-missing.ts");
        let mut diags = vec![fake_diag(&nonexistent, 42, "no-unnecessary-type-parameters")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_match_nullish_coalescing() {
        // `??` must not be mistaken for a ternary.
        let src = "type Q<T> = T extends string ? T : T; const x = a ?? 1;\n";
        let path = write_temp("nullish.ts", src);
        // Diagnostic on this line — not a unit-conditional, must be kept.
        let mut diags = vec![fake_diag(&path, 1, "no-unnecessary-type-parameters")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_drop_nested_conditional_without_function_generic() {
        // A nested conditional type that happens to have unit arms but no
        // `<T>()` function-generic pattern must NOT be suppressed.
        // `T` at column 6 is a genuine unused type parameter on this line.
        let src = "type A<T> = T extends (U extends V ? 1 : 2) ? 3 : 4;\n";
        let path = write_temp("nested_conditional.ts", src);
        let line = line_of(src, "T extends");
        let mut diags = vec![fake_diag(&path, line, "no-unnecessary-type-parameters")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "diagnostic on T must be kept — no <T>() probe present");
    }

    #[test]
    fn keeps_single_param_multiline_signature() {
        // Type param is only used ONCE even across multiple lines — keep diagnostic.
        let src = concat!(
            "function f<T>(\n",
            "  x: number,\n",
            "  y: string,\n",
            "): void {}\n",
        );
        let path = write_temp("single_param_multiline.ts", src);
        let line = line_of(src, "function f<T>");
        let mut diags = vec![fake_diag(&path, line, "no-unnecessary-type-parameters")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "T appears on no param lines — must be kept");
    }
}
