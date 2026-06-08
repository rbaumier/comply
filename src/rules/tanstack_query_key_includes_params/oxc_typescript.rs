use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

pub struct Check;

const QUERY_HOOKS: &[&str] = &[
    "useQuery",
    "useSuspenseQuery",
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
    "queryOptions",
    "infiniteQueryOptions",
];

const IGNORED_GLOBALS: &[&str] = &[
    "fetch", "console", "Math", "JSON", "window", "document", "Promise", "Object",
    "Array", "Number", "String", "Boolean", "Date", "Error", "Symbol", "Map", "Set",
    "WeakMap", "WeakSet", "RegExp", "undefined", "null", "true", "false", "this",
    "globalThis", "localStorage", "sessionStorage", "URL", "URLSearchParams",
    "FormData", "Headers", "Request", "Response", "AbortController", "AbortSignal",
    "parseInt", "parseFloat", "isNaN", "isFinite", "NaN", "Infinity",
];

const KEYWORDS: &[&str] = &[
    "async", "await", "return", "const", "let", "var", "if", "else", "for", "while",
    "function", "class", "import", "export", "from", "of", "in", "new", "typeof",
    "instanceof", "void", "delete", "throw", "try", "catch", "finally", "switch",
    "case", "break", "continue", "default", "do", "debugger", "yield", "static",
    "extends", "super", "get", "set", "type", "interface", "enum", "as", "keyof",
    "readonly", "abstract", "implements", "namespace", "module", "declare",
    "signal", "then", "catch",
];

/// Collect module-scope bindings: imports and top-level declarations.
fn collect_module_scope_bindings(source: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut from = 0usize;
    // Imports: `import { a, b as c } from ...` or `import x from` or `import * as ns`
    while let Some(rel) = source[from..].find("import ") {
        let abs = from + rel;
        let pre = source.as_bytes().get(abs.saturating_sub(1)).copied();
        if pre.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
            from = abs + 1;
            continue;
        }
        let after = abs + "import ".len();
        let rest = source[after..].trim_start();
        if rest.starts_with('"') || rest.starts_with('\'') || rest.starts_with('(') {
            // import "side-effect" or import() dynamic
            from = abs + 1;
            continue;
        }
        // Find `from` or end-of-statement — extract all word tokens before `from`
        let line_end = source[abs..].find('\n').map(|p| abs + p).unwrap_or(source.len());
        let stmt = &source[abs..line_end];
        let from_pos = stmt.rfind(" from ").unwrap_or(stmt.len());
        let before_from = &stmt[..from_pos];
        let mut k = 0usize;
        let bs = before_from.as_bytes();
        while k < bs.len() {
            if bs[k].is_ascii_alphabetic() || bs[k] == b'_' || bs[k] == b'$' {
                let start = k;
                while k < bs.len() && (bs[k].is_ascii_alphanumeric() || bs[k] == b'_' || bs[k] == b'$') {
                    k += 1;
                }
                let word = &before_from[start..k];
                if word != "import" && word != "type" && word != "as" {
                    out.insert(word.to_string());
                }
            } else {
                k += 1;
            }
        }
        from = line_end;
    }
    // Top-level const/let/var/function/class: look for lines that start at column 0
    // with const/let/var/function/class (no leading whitespace = module scope).
    for line in source.lines() {
        let trimmed = line.trim_start();
        let is_export = trimmed.starts_with("export ");
        let effective = if is_export { trimmed["export ".len()..].trim_start() } else { trimmed };
        for kw in &["const ", "let ", "var ", "function ", "class "] {
            if let Some(rest) = effective.strip_prefix(kw) {
                let bs = rest.as_bytes();
                let end = bs.iter().position(|&c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'$').unwrap_or(bs.len());
                if end > 0 {
                    out.insert(rest[..end].to_string());
                }
            }
        }
    }
    out
}

/// Find the opening `{` of the object literal that directly contains the property at `prop_pos`.
fn find_enclosing_object_open(source: &str, prop_pos: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut i = prop_pos;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Check if the call expression wrapping this object is one of the query hooks.
fn is_inside_query_hook(source: &str, obj_open: usize) -> bool {
    let bytes = source.as_bytes();
    let mut i = obj_open;
    // Skip whitespace backward to find `(`
    while i > 0 && bytes[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    if i == 0 || bytes[i - 1] != b'(' {
        return false;
    }
    i -= 1; // now at `(`
    // Extract identifier before `(`
    let end = i;
    while i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_' || bytes[i - 1] == b'$') {
        i -= 1;
    }
    let name = &source[i..end];
    QUERY_HOOKS.contains(&name)
}

/// Extract params text (the content inside the outermost `(...)`) starting at `pos`.
fn extract_params_text(source: &str, pos: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let mut i = pos;
    // Skip to `(`
    while i < bytes.len() && bytes[i] != b'(' {
        if bytes[i] == b'=' || bytes[i] == b'{' {
            return None;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let open = i;
    let mut depth = 1i32;
    i += 1;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    Some((source[open + 1..i - 1].to_string(), i))
}

/// Collect all word tokens from a params text as bound names.
fn collect_param_names(params: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = params.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' || bytes[i] == b'$' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$') {
                i += 1;
            }
            let word = &params[start..i];
            if !KEYWORDS.contains(&word) {
                out.push(word.to_string());
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Find the function body text after `queryFn:`, returning (body_text, fn_value_start_offset).
fn extract_fn_value(source: &str, after_colon: usize) -> Option<(String, usize, Vec<String>)> {
    let rest = source[after_colon..].trim_start();
    let fn_start = after_colon + (source[after_colon..].len() - rest.len());
    let bytes = source.as_bytes();

    // Skip `async` keyword
    let (rest2, fn_start2) = if rest.starts_with("async") {
        let after_async = fn_start + 5;
        let r = source[after_async..].trim_start();
        (r, after_async + (source[after_async..].len() - r.len()))
    } else {
        (rest, fn_start)
    };

    // Not a function? (e.g., bare identifier reference like `queryFn: myFn`)
    if !rest2.starts_with('(') && !rest2.starts_with('{') {
        // Single-arg arrow: `arg => body`
        let bs = rest2.as_bytes();
        let mut k = 0;
        while k < bs.len() && (bs[k].is_ascii_alphanumeric() || bs[k] == b'_' || bs[k] == b'$') {
            k += 1;
        }
        if k == 0 {
            return None; // bare identifier like `queryFn: myFn` — can't analyze
        }
        let param = &rest2[..k];
        let after_param = rest2[k..].trim_start();
        if !after_param.starts_with("=>") {
            return None;
        }
        let body_start_abs = fn_start2 + k + (rest2[k..].len() - after_param.len()) + 2;
        let body_text = extract_body(source, body_start_abs)?;
        return Some((body_text, fn_start2, vec![param.to_string()]));
    }

    // Params in `(...)`
    let (params_text, after_params) = extract_params_text(source, fn_start2)?;
    let param_names = collect_param_names(&params_text);

    // After params: skip whitespace, optional `: ReturnType`, then `=>` or `{`
    let after_str = source[after_params..].trim_start();
    let body_abs_start = after_params + (source[after_params..].len() - after_str.len());

    // Skip `: ReturnType` annotation
    let (after_annot_str, body_search_start) = if after_str.starts_with(':') {
        let mut j = body_abs_start + 1;
        let mut depth = 0i32;
        while j < bytes.len() {
            match bytes[j] {
                b'<' => depth += 1,
                b'>' => {
                    depth -= 1;
                }
                b'=' if depth == 0 => break,
                b'{' if depth == 0 => break,
                _ => {}
            }
            j += 1;
        }
        let r = source[j..].trim_start();
        (r, j + (source[j..].len() - r.len()))
    } else {
        (after_str, body_abs_start)
    };

    // Skip `=>`
    let body_start = if after_annot_str.starts_with("=>") {
        let r = source[body_search_start + 2..].trim_start();
        body_search_start + 2 + (source[body_search_start + 2..].len() - r.len())
    } else {
        body_search_start
    };

    let body_text = extract_body(source, body_start)?;
    Some((body_text, fn_start2, param_names))
}

/// Extract body text starting at `pos` (either `{...}` block or expression up to `,`/`}`).
fn extract_body(source: &str, pos: usize) -> Option<String> {
    let bytes = source.as_bytes();
    if pos >= bytes.len() {
        return None;
    }
    if bytes[pos] == b'{' {
        // Block body
        let mut depth = 1i32;
        let mut i = pos + 1;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        Some(source[pos..i].to_string())
    } else {
        // Expression body: ends at `,` or `)` or `}` at depth 0
        let mut depth_curly = 0i32;
        let mut depth_paren = 0i32;
        let mut depth_bracket = 0i32;
        let mut i = pos;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => depth_curly += 1,
                b'}' => {
                    if depth_curly == 0 {
                        break;
                    }
                    depth_curly -= 1;
                }
                b'(' => depth_paren += 1,
                b')' => {
                    if depth_paren == 0 {
                        break;
                    }
                    depth_paren -= 1;
                }
                b'[' => depth_bracket += 1,
                b']' => {
                    if depth_bracket == 0 {
                        break;
                    }
                    depth_bracket -= 1;
                }
                b',' if depth_curly == 0 && depth_paren == 0 && depth_bracket == 0 => break,
                _ => {}
            }
            i += 1;
        }
        Some(source[pos..i].to_string())
    }
}

/// Collect local variable declarations inside a function body.
fn collect_local_decls(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for kw in &["const ", "let ", "var "] {
        let mut from = 0usize;
        while let Some(rel) = body[from..].find(kw) {
            let abs = from + rel;
            let rest = &body[abs + kw.len()..];
            let bs = rest.as_bytes();
            let mut k = 0;
            // Handle destructuring
            if bs.first() == Some(&b'{') || bs.first() == Some(&b'[') {
                // Collect all identifiers in the destructure pattern
                let mut j = 1;
                let open = bs[0];
                let close = if open == b'{' { b'}' } else { b']' };
                let mut depth = 1i32;
                while j < bs.len() && depth > 0 {
                    match bs[j] {
                        c if c == open => depth += 1,
                        c if c == close => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                let inner = &rest[1..j - 1];
                out.extend(collect_param_names(inner));
            } else {
                while k < bs.len() && (bs[k].is_ascii_alphanumeric() || bs[k] == b'_' || bs[k] == b'$') {
                    k += 1;
                }
                if k > 0 {
                    let word = &rest[..k];
                    if !KEYWORDS.contains(&word) {
                        out.push(word.to_string());
                    }
                }
            }
            from = abs + 1;
        }
    }
    out
}

/// Collect identifiers referenced in the body that are potential free variables.
/// Skips: string literal contents, property access targets (preceded by `.`),
/// callees (followed by `(`), PascalCase, keywords, globals.
fn collect_body_refs(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Skip string literals to avoid collecting identifiers inside them.
        if bytes[i] == b'\'' || bytes[i] == b'"' {
            let q = bytes[i];
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == q {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' || bytes[i] == b'$' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$') {
                i += 1;
            }
            let word = &body[start..i];
            if KEYWORDS.contains(&word) {
                continue;
            }
            if word.chars().next().is_some_and(|c| c.is_uppercase()) {
                continue;
            }
            if start > 0 && bytes[start - 1] == b'.' {
                continue;
            }
            let after_word = body[i..].trim_start();
            if after_word.starts_with('(') {
                continue;
            }
            if IGNORED_GLOBALS.contains(&word) {
                continue;
            }
            out.push(word.to_string());
        } else {
            i += 1;
        }
    }
    out
}

/// Collect all identifier tokens from the queryKey value.
fn collect_key_idents(key_value: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let bytes = key_value.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' || bytes[i] == b'$' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$') {
                i += 1;
            }
            let word = &key_value[start..i];
            if !KEYWORDS.contains(&word) {
                out.insert(word.to_string());
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Find the sibling `queryKey:` value text within the same object literal (bounded by `obj_open`).
fn find_query_key_value(source: &str, obj_open: usize, obj_close: usize) -> Option<String> {
    let obj_body = &source[obj_open..obj_close];
    let mut from = 0usize;
    while let Some(rel) = obj_body[from..].find("queryKey") {
        let abs = from + rel;
        let after = abs + "queryKey".len();
        // Skip whitespace and find `:`
        let rest = obj_body[after..].trim_start();
        if !rest.starts_with(':') {
            from = abs + 1;
            continue;
        }
        let colon_rel = after + (obj_body[after..].len() - rest.len());
        let value_start_str = obj_body[colon_rel + 1..].trim_start();
        let value_start = colon_rel + 1 + (obj_body[colon_rel + 1..].len() - value_start_str.len());
        // Extract to end of value (next `,` or end of object at depth 0)
        let value_text = extract_body(source, obj_open + value_start)?;
        return Some(value_text);
    }
    None
}

fn find_obj_close(source: &str, obj_open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 1i32;
    let mut i = obj_open + 1;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    if depth == 0 { Some(i - 1) } else { None }
}

fn find_offenses(source: &str) -> Vec<(usize, String)> {
    let module_bindings = collect_module_scope_bindings(source);
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find("queryFn") {
        let abs = from + rel;
        // Word boundary
        let pre = source.as_bytes().get(abs.saturating_sub(1)).copied();
        let post = source.as_bytes().get(abs + "queryFn".len()).copied();
        if pre.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'$') {
            from = abs + 1;
            continue;
        }
        if post != Some(b':') {
            from = abs + 1;
            continue;
        }

        // Find enclosing object
        let Some(obj_open) = find_enclosing_object_open(source, abs) else {
            from = abs + 1;
            continue;
        };
        if !is_inside_query_hook(source, obj_open) {
            from = abs + 1;
            continue;
        }
        let Some(obj_close) = find_obj_close(source, obj_open) else {
            from = abs + 1;
            continue;
        };

        // Extract function value
        let after_colon = abs + "queryFn".len() + 1; // skip `:`
        let Some((body_text, fn_start, params)) = extract_fn_value(source, after_colon) else {
            from = abs + 1;
            continue;
        };

        let local_decls = collect_local_decls(&body_text);
        let body_refs = collect_body_refs(&body_text);

        let bound: HashSet<&str> = params.iter()
            .chain(local_decls.iter())
            .map(String::as_str)
            .collect();

        let mut needed: BTreeSet<String> = BTreeSet::new();
        for name in &body_refs {
            if bound.contains(name.as_str()) { continue; }
            if IGNORED_GLOBALS.contains(&name.as_str()) { continue; }
            if name.chars().next().is_some_and(char::is_uppercase) { continue; }
            if module_bindings.contains(name) { continue; }
            needed.insert(name.clone());
        }
        if needed.is_empty() {
            from = abs + 1;
            continue;
        }

        // Find sibling queryKey
        let key_value = find_query_key_value(source, obj_open, obj_close);
        if key_value.is_none() {
            // No queryKey at all — let tanstack-query-array-key handle it
            from = abs + 1;
            continue;
        }
        let key_idents = collect_key_idents(&key_value.unwrap());

        let missing: Vec<&String> = needed.iter().filter(|n| !key_idents.contains(*n)).collect();
        if missing.is_empty() {
            from = abs + 1;
            continue;
        }

        let list = missing.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>().join(", ");
        out.push((fn_start, format!(
            "`queryFn` references {list} but `queryKey` does not include it — \
             different values will collide on the same cache slot. Add the \
             identifier(s) to the `queryKey` array."
        )));
        from = abs + 1;
    }
    out
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["queryFn"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.source_contains("queryFn") {
            return Vec::new();
        }
        find_offenses(ctx.source)
            .into_iter()
            .map(|(offset, message)| {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message,
                    severity: Severity::Error,
                    span: None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_closure_var_missing_from_key() {
        let diags = run("useQuery({ queryKey: ['user'], queryFn: () => fetchUser(userId) });");
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(diags[0].message.contains("userId"), "{diags:?}");
    }

    #[test]
    fn allows_closure_var_present_in_key() {
        let diags = run("useQuery({ queryKey: ['user', userId], queryFn: () => fetchUser(userId) });");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn flags_only_missing_when_multiple_vars() {
        let diags = run("useQuery({ queryKey: ['user', userId], queryFn: () => api(userId, filter) });");
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(diags[0].message.contains("filter"), "{diags:?}");
        assert!(!diags[0].message.contains("`userId`"), "{diags:?}");
    }

    #[test]
    fn ignores_param_references() {
        let diags = run(
            "useQuery({ queryKey: ['user'], queryFn: ({ signal }) => fetch('/x', { signal }) });",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn ignores_globals_and_pascal_case() {
        let diags = run("useQuery({ queryKey: ['x'], queryFn: () => fetch(URL).then(JSON.parse) });");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn handles_query_options_factory() {
        let diags = run("queryOptions({ queryKey: ['user'], queryFn: () => fetchUser(userId) });");
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn ignores_non_query_hooks() {
        let diags = run("someOther({ queryKey: ['user'], queryFn: () => fetchUser(userId) });");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn ignores_module_level_imported_singleton() {
        let src = r#"
            import { api } from "./client";
            export function usersQueryOptions(query: string) {
                return queryOptions({
                    queryKey: ["users", query],
                    queryFn: async ({ signal }) => api.users.get({ query, fetch: { signal } }),
                });
            }
        "#;
        let diags = run(src);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn ignores_local_const_in_body() {
        let diags = run(
            "useQuery({ queryKey: ['user', userId], queryFn: () => { const x = 1; return fetchUser(userId, x); } });",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn template_string_interpolation_in_key_counts() {
        let diags = run("useQuery({ queryKey: [`user-${userId}`], queryFn: () => fetchUser(userId) });");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn object_property_in_key_counts() {
        let diags = run(
            "useQuery({ queryKey: ['user', { id: userId }], queryFn: () => fetchUser(userId) });",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }
}
