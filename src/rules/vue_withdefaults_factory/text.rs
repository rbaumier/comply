//! vue-withdefaults-factory AST backend.
//!
//! Scans inside `withDefaults(defineProps<...>(), { ... })` for keys whose
//! value is a literal `[]` / `[...]` / `{}` / `{...}` instead of a factory.
//!
//! Only the *direct* properties of the defaults object (nesting depth 0 relative
//! to that object) are prop defaults. A literal `[]` / `{}` nested inside a
//! factory's return value (`foo: () => ({ bar: [] })`) is correct and
//! shared-instance-safe, so keys at depth > 0 are skipped.

use crate::diagnostic::{Diagnostic, Severity};

fn find_withdefaults_block(src: &str) -> Option<(usize, usize, usize)> {
    let pos = src.find("withDefaults(")?;
    let after = pos + "withDefaults(".len();
    let bytes = src.as_bytes();
    let mut depth = 1i32;
    let mut j = after;
    let mut top_comma: Option<usize> = None;
    while j < bytes.len() && depth > 0 {
        match bytes[j] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 1 => {
                if top_comma.is_none() {
                    top_comma = Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    let comma = top_comma?;
    let rest = &src[comma + 1..];
    let obj_rel = rest.find('{')?;
    let obj_start = comma + 1 + obj_rel + 1;
    let mut odepth = 1i32;
    let mut k = obj_start;
    while k < bytes.len() && odepth > 0 {
        match bytes[k] {
            b'{' => odepth += 1,
            b'}' => odepth -= 1,
            _ => {}
        }
        k += 1;
    }
    let base_line = src[..obj_start].matches('\n').count();
    Some((obj_start, k.saturating_sub(1), base_line))
}

/// Nesting depth (parens / brackets / braces) at the start of each line of
/// `body`, counting only delimiters in executable code — those inside strings
/// (`'`, `"`, `` ` ``) or comments (`//`, `/* */`) are ignored so a delimiter in
/// a string or comment does not corrupt the count. `depths[i]` is the depth at
/// the start of line `i` (line 0 starts at depth 0); a `key: value` whose line
/// begins at depth 0 is a direct property of the defaults object. Backtick
/// regions are treated as opaque strings (`${...}` interpolation is not
/// descended into — vanishingly rare inside a defaults object). Regex literals
/// (`/.../`) are not recognized, so an unbalanced delimiter inside one could
/// mis-count depth; also vanishingly rare inside a defaults object.
fn line_start_depths(body: &str) -> Vec<i32> {
    enum State {
        Code,
        Str(u8),
        LineComment,
        BlockComment,
    }
    let bytes = body.as_bytes();
    let mut depths = vec![0i32];
    let mut depth = 0i32;
    let mut state = State::Code;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match state {
            State::Code => match b {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b'\'' | b'"' | b'`' => state = State::Str(b),
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    state = State::LineComment;
                    i += 1;
                }
                b'/' if bytes.get(i + 1) == Some(&b'*') => {
                    state = State::BlockComment;
                    i += 1;
                }
                _ => {}
            },
            State::Str(quote) => match b {
                b'\\' => i += 1, // skip the escaped byte
                _ if b == quote => state = State::Code,
                _ => {}
            },
            State::LineComment => {
                if b == b'\n' {
                    state = State::Code;
                }
            }
            State::BlockComment => {
                if b == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    state = State::Code;
                    i += 1;
                }
            }
        }
        if b == b'\n' {
            depths.push(depth);
        }
        i += 1;
    }
    depths
}

crate::ast_check! { on ["component"] prefilter = ["withDefaults"] => |node, source, ctx, diagnostics|
    let _ = source;
    let Some((start, end, base_line)) = find_withdefaults_block(ctx.source) else {
        return;
    };
    let body = &ctx.source[start..end];
    let depths = line_start_depths(body);
    for (idx, line) in body.lines().enumerate() {
        // Only direct properties (depth 0) of the defaults object are prop
        // defaults; a literal `[]`/`{}` nested inside a factory is not.
        if depths.get(idx).copied().unwrap_or(0) != 0 {
            continue;
        }
        let trimmed = line.trim();
        let Some(colon) = trimmed.find(':') else { continue };
        let key = trimmed[..colon].trim();
        if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '\'' || c == '"') {
            continue;
        }
        let value = trimmed[colon + 1..].trim_start();
        let first = value.chars().next().unwrap_or(' ');
        if first == '[' || first == '{' {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: base_line + idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{key}` default is a literal {} — in `withDefaults` it must be a factory `() => {}`.",
                    if first == '[' { "array" } else { "object" },
                    if first == '[' { "[]" } else { "({})" }
                ),
                severity: Severity::Error,
                span: None,
            });
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
    fn flags_array_literal_default() {
        let sfc = "<script setup>\nconst p = withDefaults(defineProps<{ items?: string[] }>(), {\n  items: []\n})\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_object_literal_default() {
        let sfc = "<script setup>\nconst p = withDefaults(defineProps<{ cfg?: object }>(), {\n  cfg: { a: 1 }\n})\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_factory() {
        let sfc = "<script setup>\nconst p = withDefaults(defineProps<{ items?: string[] }>(), {\n  items: () => []\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_primitive() {
        let sfc = "<script setup>\nconst p = withDefaults(defineProps<{ n?: number }>(), {\n  n: 42\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_literal_nested_inside_factory() {
        // Repro from #7673: `higherDeptOptions: []` is nested inside the
        // `formInline` factory's return object (depth > 0), not a prop default.
        let sfc = "<script setup>\nconst props = withDefaults(defineProps<FormProps>(), {\n  formInline: () => ({\n    higherDeptOptions: [],\n    parentId: 0,\n    title: \"x\"\n  })\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_top_level_only_alongside_nested_factory_literal() {
        // A top-level literal-array default is flagged; a literal array nested
        // inside a sibling factory in the same `withDefaults` is not.
        let sfc = "<script setup>\nconst p = withDefaults(defineProps<Props>(), {\n  items: [],\n  formInline: () => ({\n    nested: []\n  })\n})\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`items`"));
    }

    #[test]
    fn flags_top_level_array_and_object_defaults() {
        let sfc = "<script setup>\nconst p = withDefaults(defineProps<Props>(), {\n  foo: [],\n  bar: {}\n})\n</script>";
        assert_eq!(run(sfc).len(), 2);
    }

    #[test]
    fn unbalanced_delimiter_in_string_does_not_hide_next_default() {
        // A `{` inside a string value must not corrupt the depth for the
        // following line, which would drop a real top-level literal default.
        let sfc = "<script setup>\nconst p = withDefaults(defineProps<Props>(), {\n  label: \"{\",\n  items: []\n})\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`items`"));
    }
}
