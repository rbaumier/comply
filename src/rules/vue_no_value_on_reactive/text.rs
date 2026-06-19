//! vue-no-value-on-reactive AST backend.
//!
//! Tracks variables assigned from `reactive(...)` and flags any usage of
//! `name.value` for those variables. A reactive whose initializer object
//! literal declares a top-level `value` key (e.g. `reactive({ value: '' })`)
//! is not tracked, since `name.value` is then a real field access.

use crate::diagnostic::{Diagnostic, Severity};

/// Walk from `start` (a `{` byte index in `src`) and return the byte index
/// of the matching `}`, ignoring braces inside strings and template
/// literals. Returns `None` if unbalanced.
fn matching_brace(src: &str, start: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth: i32 = 0;
    let mut i = start;
    let mut in_str: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' | b'`' => in_str = Some(c),
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

/// Extract the `const/let/var NAME` identifier immediately preceding the `=`
/// that sits just before `reactive(` at byte index `reactive_at`. Returns
/// `None` if the call is not a `const/let/var NAME = reactive(...)` binding.
fn binding_name(src: &str, reactive_at: usize) -> Option<&str> {
    let before = src[..reactive_at].trim_end();
    let before = before.strip_suffix('=')?;
    let before = before.trim_end();
    let name_start = before
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map_or(0, |i| i + 1);
    let name = &before[name_start..];
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let keyword = before[..name_start].trim_end();
    if keyword.ends_with("const") || keyword.ends_with("let") || keyword.ends_with("var") {
        Some(name)
    } else {
        None
    }
}

/// Whether the object-literal body (the text strictly between the reactive
/// object's own braces) declares a `value` key at depth 1. Braces inside
/// strings/templates are ignored; only an identifier-bounded `value` followed
/// by `:` (property) or by `,`/`}`/end (shorthand) counts.
fn declares_top_level_value(body: &str) -> bool {
    let bytes = body.as_bytes();
    let mut depth: i32 = 0;
    let mut in_str: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' | b'`' => {
                in_str = Some(c);
                i += 1;
                continue;
            }
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => depth -= 1,
            _ => {}
        }
        if depth == 0 && body.is_char_boundary(i) && body[i..].starts_with("value") {
            let prev_ok = body[..i]
                .chars()
                .next_back()
                .is_none_or(|p| p == '{' || p == ',' || p.is_whitespace());
            let after = body[i + "value".len()..].trim_start();
            let next_ok = after.starts_with(':')
                || after.starts_with(',')
                || after.starts_with('}')
                || after.is_empty();
            if prev_ok && next_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn collect_reactives(source: &str) -> Vec<String> {
    let needle = "reactive(";
    let mut names = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = source[cursor..].find(needle) {
        let abs = cursor + rel;
        cursor = abs + needle.len();
        let Some(name) = binding_name(source, abs) else {
            continue;
        };
        let after_paren = abs + needle.len();
        let rest = source[after_paren..].trim_start();
        if !rest.starts_with('{') {
            // Not an object-literal initializer — track conservatively.
            names.push(name.to_string());
            continue;
        }
        let brace_idx = after_paren + (source[after_paren..].len() - rest.len());
        let Some(end) = matching_brace(source, brace_idx) else {
            names.push(name.to_string());
            continue;
        };
        let body = &source[brace_idx + 1..end];
        if !declares_top_level_value(body) {
            names.push(name.to_string());
        }
    }
    names
}

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    let names = collect_reactives(ctx.source);
    if names.is_empty() {
        return;
    }
    for (idx, line) in ctx.source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var ") {
            continue;
        }
        for name in &names {
            let pattern = format!("{name}.value");
            if line.contains(&pattern) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{name}` is a reactive proxy — `{name}.value` is undefined. Access its keys directly."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                break;
            }
        }
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
    fn flags_value_on_reactive() {
        let sfc =
            "<script setup>\nconst state = reactive({ n: 0 })\nconsole.log(state.value)\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_direct_key_access() {
        let sfc =
            "<script setup>\nconst state = reactive({ n: 0 })\nconsole.log(state.n)\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_value_on_ref() {
        let sfc = "<script setup>\nconst x = ref(0)\nconsole.log(x.value)\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_value_key_single_line() {
        // Regression #4428: `value` is a real property of the reactive object.
        let sfc = "<script setup>\nconst newCookie = reactive({ key: '', value: '' })\nnewCookie.value = 'test'\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_value_key_multi_line() {
        let sfc =
            "<script setup>\nconst c = reactive({\n  key: '',\n  value: '',\n})\nc.value = 'x'\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_value_key_shorthand() {
        let sfc =
            "<script setup>\nconst value = ''\nconst c = reactive({ value })\nc.value = 'x'\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_nested_only_value_key() {
        // The top-level object has no `value` key — only a nested one.
        let sfc =
            "<script setup>\nconst c = reactive({ x: { value: 1 } })\nconsole.log(c.value)\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_when_key_merely_contains_value() {
        let sfc =
            "<script setup>\nconst c = reactive({ myValue: 1 })\nconsole.log(c.value)\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn handles_multibyte_string_in_object_body() {
        // A multi-byte char in a string value must not break byte-indexed
        // scanning; the top-level `value` key still exempts the reactive.
        let sfc =
            "<script setup>\nconst c = reactive({ label: 'café', value: '' })\nc.value = 'x'\n</script>";
        assert!(run(sfc).is_empty());
    }
}
