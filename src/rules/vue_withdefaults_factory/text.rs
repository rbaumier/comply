//! vue-withdefaults-factory AST backend.
//!
//! Scans inside `withDefaults(defineProps<...>(), { ... })` for keys whose
//! value is a literal `[]` / `[...]` / `{}` / `{...}` instead of a factory.

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

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    let Some((start, end, base_line)) = find_withdefaults_block(ctx.source) else {
        return;
    };
    let body = &ctx.source[start..end];
    for (idx, line) in body.lines().enumerate() {
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
}
