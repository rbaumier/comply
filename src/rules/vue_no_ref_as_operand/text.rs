//! vue-no-ref-as-operand text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashSet;

#[derive(Debug)]
pub struct Check;

/// Collect identifiers bound to a `ref(...)` / `shallowRef(...)` /
/// `computed(...)` call in the source. Heuristic: look for
/// `const X = ref(...)` patterns.
fn collect_ref_bindings(source: &str) -> HashSet<String> {
    let mut bindings = HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let after_kw = trimmed
            .strip_prefix("const ")
            .or_else(|| trimmed.strip_prefix("let "));
        let Some(rest) = after_kw else { continue };
        // Split on '=' to get the name.
        let Some((lhs, rhs)) = rest.split_once('=') else { continue };
        let name = lhs.split([':', ' ']).next().unwrap_or("").trim();
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
            continue;
        }
        let rhs_trim = rhs.trim_start();
        if rhs_trim.starts_with("ref(")
            || rhs_trim.starts_with("shallowRef(")
            || rhs_trim.starts_with("customRef(")
            || rhs_trim.starts_with("computed(")
            || rhs_trim.starts_with("toRef(")
        {
            bindings.insert(name.to_string());
        }
    }
    bindings
}

/// A block-bodied `function`/arrow scope: the byte range of its `{ … }` body
/// and the bare parameter names that shadow any outer binding inside it.
struct ShadowScope {
    body: std::ops::Range<usize>,
    params: HashSet<String>,
}

/// Whether `byte` is part of a JS/TS identifier (so we can require word
/// boundaries when matching the `function` keyword).
fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

/// Find the matching closing delimiter for the opener at `open` (a `(` or `{`),
/// returning the index of the closer. Returns `None` if unbalanced.
fn match_delimiter(bytes: &[u8], open: usize) -> Option<usize> {
    let (opener, closer) = (bytes[open], match bytes[open] {
        b'(' => b')',
        b'{' => b'}',
        _ => return None,
    });
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        if b == opener {
            depth += 1;
        } else if b == closer {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Extract bare parameter identifiers from a parameter list (the text between
/// the parens). Splits on depth-0 commas, takes each param's leading
/// identifier, and skips destructuring (`{…}` / `[…]`) params, which never
/// bind a bare outer-ref name.
fn parse_param_names(param_list: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let bytes = param_list.as_bytes();
    let mut segments: Vec<&str> = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            b',' if depth == 0 => {
                segments.push(&param_list[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    segments.push(&param_list[start..]);
    for seg in segments {
        // Strip leading modifiers/spread; destructuring params start with
        // `{`/`[` and bind no bare name.
        let trimmed = seg.trim().trim_start_matches("...").trim_start();
        let ident: String = trimmed
            .chars()
            .take_while(|&c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
            .collect();
        if !ident.is_empty() {
            names.insert(ident);
        }
    }
    names
}

/// Collect block-bodied `function …(params) { … }` and arrow `(params) => { … }`
/// shadow scopes. A misuse match whose offset lies inside a scope's body and
/// whose name is one of that scope's params is shadowing the outer ref and must
/// not be flagged.
fn collect_shadow_scopes(source: &str) -> Vec<ShadowScope> {
    let bytes = source.as_bytes();
    let mut scopes = Vec::new();

    // `function`-keyword forms: `function name(params) {` and
    // `function (params) {` (anonymous).
    for (kw, _) in source.match_indices("function") {
        let before_ok = kw == 0 || !is_ident_byte(bytes[kw - 1]);
        let after = kw + "function".len();
        let after_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
        if !before_ok || !after_ok {
            continue;
        }
        if let Some(scope) = scope_from_params_at(source, after) {
            scopes.push(scope);
        }
    }

    // Arrow forms: a `(params) => {` whose `=>` is immediately followed by a
    // block body. Anchor on `=>` then walk back to the param-list parens.
    for (arrow, _) in source.match_indices("=>") {
        let mut j = arrow + 2;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'{' {
            continue;
        }
        // Walk back over whitespace to the `)` that closes the param list.
        let mut k = arrow;
        while k > 0 && bytes[k - 1].is_ascii_whitespace() {
            k -= 1;
        }
        if k == 0 || bytes[k - 1] != b')' {
            continue;
        }
        let close_paren = k - 1;
        let Some(open_paren) = find_matching_open(bytes, close_paren) else {
            continue;
        };
        let params = parse_param_names(&source[open_paren + 1..close_paren]);
        let Some(body_close) = match_delimiter(bytes, j) else {
            continue;
        };
        scopes.push(ShadowScope {
            body: j..body_close,
            params,
        });
    }

    scopes
}

/// Build a shadow scope from the position just after a `function` keyword:
/// skips the optional name, reads the `(params)` list, then the `{ … }` body.
fn scope_from_params_at(source: &str, after_kw: usize) -> Option<ShadowScope> {
    let bytes = source.as_bytes();
    let mut i = after_kw;
    // Skip whitespace, an optional `*` (generator) and the optional name.
    while i < bytes.len() && bytes[i] != b'(' {
        if bytes[i] == b'{' {
            return None;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let open_paren = i;
    let close_paren = match_delimiter(bytes, open_paren)?;
    let params = parse_param_names(&source[open_paren + 1..close_paren]);
    // Find the `{` that opens the body (skipping an optional `: ReturnType`).
    let mut b = close_paren + 1;
    while b < bytes.len() && bytes[b] != b'{' {
        b += 1;
    }
    if b >= bytes.len() {
        return None;
    }
    let body_close = match_delimiter(bytes, b)?;
    Some(ShadowScope {
        body: b..body_close,
        params,
    })
}

/// Find the `(` matching the `)` at `close` by scanning backwards.
fn find_matching_open(bytes: &[u8], close: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = close;
    loop {
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}

fn byte_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let bindings = collect_ref_bindings(ctx.source);
        if bindings.is_empty() {
            return Vec::new();
        }
        let shadow_scopes = collect_shadow_scopes(ctx.source);
        let mut diagnostics = Vec::new();
        // Look for `<name> + ` / `<name> ===` / `<name>++` / `<name>--`
        // patterns where the binding is used like a primitive.
        for name in &bindings {
            for (i, _) in ctx.source.match_indices(name.as_str()) {
                // A function/arrow parameter with the same name shadows the
                // outer ref inside its body; the bare name there is the param,
                // not the ref, so it is not a misuse.
                if shadow_scopes
                    .iter()
                    .any(|s| s.body.contains(&i) && s.params.contains(name))
                {
                    continue;
                }
                // Word boundary on left.
                let prev_ok = i == 0
                    || ctx.source.as_bytes()[i - 1].is_ascii_whitespace()
                    || matches!(
                        ctx.source.as_bytes()[i - 1],
                        b'(' | b'[' | b'{' | b',' | b';' | b'=' | b'+' | b'-' | b'!'
                    );
                if !prev_ok {
                    continue;
                }
                let end = i + name.len();
                if end >= ctx.source.len() {
                    continue;
                }
                let after = &ctx.source[end..];
                let next_char = after.chars().next();
                let after_trim = after.trim_start();
                // Allow `.value`, `.something`, function-call, assignment.
                if after.starts_with('.') {
                    continue;
                }
                // Operators that misuse the ref as a primitive.
                let misuse = after_trim.starts_with("++")
                    || after_trim.starts_with("--")
                    || after_trim.starts_with("+ ")
                    || after_trim.starts_with("- ")
                    || after_trim.starts_with("* ")
                    || after_trim.starts_with("/ ")
                    || after_trim.starts_with("=== ")
                    || after_trim.starts_with("!== ")
                    || after_trim.starts_with("== ")
                    || after_trim.starts_with("!= ")
                    || (next_char == Some(' ')
                        && (after_trim.starts_with("+ ")
                            || after_trim.starts_with("- ")
                            || after_trim.starts_with("> ")
                            || after_trim.starts_with("< ")));
                if !misuse {
                    continue;
                }
                let (line, column) = byte_to_line_col(ctx.source, i);
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{name}` is a ref — unwrap with `.value` before using it as \
                         an arithmetic/comparison operand."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                break;
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("App.vue"), src))
    }

    #[test]
    fn flags_ref_arithmetic() {
        let src = "const count = ref(0);\nconst x = count + 1;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ref_value_arithmetic() {
        let src = "const count = ref(0);\nconst x = count.value + 1;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_param_shadowing_ref() {
        let src = "const page = shallowRef(1);\nfunction f(page: number) { return (page - 1) * 2; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_param_shadowing_ref_declared_later() {
        // The vueuse repro: the param-using function precedes the module-scope ref.
        let src = "function fetch(page: number, pageSize: number) {\n  const start = (page - 1) * pageSize\n  return start\n}\nconst page = shallowRef(1)";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arrow_param_shadowing_ref() {
        let src = "const page = ref(1);\nconst f = (page: number) => { return page - 1; };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_ref_misuse_inside_function_without_shadow() {
        // `count` is the module-scope ref inside `f` — no shadowing param.
        let src = "const count = ref(0);\nfunction f(n: number) { return count + n; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ref_misuse_outside_shadow_scope() {
        // The function shadows `page`, but the module-scope misuse must still flag.
        let src = "const page = ref(1);\nfunction f(page: number) { return page - 1; }\nconst y = page + 1;";
        assert_eq!(run(src).len(), 1);
    }
}
