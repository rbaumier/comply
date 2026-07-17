//! vue-ref-value-in-script AST backend.
//!
//! Scans `<script>` section for `ref()` declarations and flags comparisons /
//! conditions that reference the bare identifier without `.value`. A bare use
//! whose offset falls inside a function/arrow body that redeclares the ref name
//! (a same-named parameter, or an earlier `const`/`let`/`var` local) is the
//! shadowing plain value, not the ref, and is not flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::vue_shadow_scope::collect_shadow_scopes;

fn script_range(source: &str) -> Option<(usize, usize)> {
    let start = source.find("<script")?;
    let after_open = source[start..].find('>')? + start + 1;
    let end_rel = source[after_open..].find("</script>")?;
    Some((after_open, after_open + end_rel))
}

fn collect_refs(script: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in script.lines() {
        let trimmed = line.trim();
        for prefix in ["const ", "let ", "var "] {
            if let Some(rest) = trimmed.strip_prefix(prefix)
                && let Some(eq) = rest.find('=')
            {
                let name = rest[..eq].trim().trim_end_matches(':');
                let after_eq = rest[eq + 1..].trim_start();
                if (after_eq.starts_with("ref(") || after_eq.starts_with("shallowRef("))
                    && !name.is_empty()
                    && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

crate::ast_check! { on ["component"] prefilter = ["ref(", "shallowRef("] => |node, source, ctx, diagnostics|
    let _ = source;
    let Some((start, end)) = script_range(ctx.source) else {
        return;
    };
    let script = &ctx.source[start..end];
    let names = collect_refs(script);
    if names.is_empty() {
        return;
    }

    let shadow_scopes = collect_shadow_scopes(ctx.source);
    let base_line = ctx.source[..start].matches('\n').count();

    for (idx, line) in script.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var ") {
            continue;
        }
        for name in &names {
            let patterns = [
                format!("if ({name})"),
                format!("if ({name} "),
                format!("if (!{name})"),
                format!("if (!{name} "),
                format!("while ({name})"),
                format!("while ({name} "),
                format!("({name} === "),
                format!("({name} !== "),
                format!("({name} == "),
                format!("({name} != "),
                format!("({name} > "),
                format!("({name} < "),
                format!("({name} >= "),
                format!("({name} <= "),
            ];
            let Some(pat) = patterns.iter().find(|p| line.contains(p.as_str())) else {
                continue;
            };
            // Absolute byte offset (in `ctx.source`) of the bare-name use the
            // pattern matched: `line` is a subslice of `ctx.source`, so pointer
            // arithmetic recovers its start, and the name sits at the rightmost
            // position of the matched pattern.
            let name_offset = (line.as_ptr() as usize - ctx.source.as_ptr() as usize)
                + line.find(pat.as_str()).unwrap()
                + pat.rfind(name.as_str()).unwrap();
            // A same-named function/arrow parameter — or a `const`/`let`/`var`
            // local declared earlier in the body — shadows the outer ref inside
            // that scope; the bare name there is the plain local value, so the
            // comparison is correct and must not be flagged.
            if shadow_scopes.iter().any(|s| {
                s.body.contains(&name_offset)
                    && (s.params.contains(name.as_str())
                        || s.locals.iter().any(|(n, d)| n == name && *d < name_offset))
            }) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: base_line + idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{name}` is a ref — comparing it without `.value` compares the Ref object, not the inner value. Use `{name}.value`."
                ),
                severity: Severity::Error,
                span: None,
            });
            break;
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
    fn flags_bare_ref_in_condition() {
        let sfc = "<script setup>\nconst x = ref(0)\nif (x > 0) {}\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_dot_value() {
        let sfc = "<script setup>\nconst x = ref(0)\nif (x.value > 0) {}\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn ignores_non_ref() {
        let sfc = "<script setup>\nconst x = 0\nif (x > 0) {}\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_function_param_shadowing_ref() {
        // varletjs/varlet repro (#6922): outer `const index = ref(0)`, shadowed
        // by the `clampIndex(index)` parameter. The `index < 0` / `index >=`
        // comparisons inside the body are the plain number param, not the ref.
        let sfc = "<script setup>\nconst index = ref(0)\nfunction clampIndex(index) {\n  if (index < 0) { return length.value + index }\n  if (index >= length.value) { return index - length.value }\n}\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_arrow_param_shadowing_ref() {
        // An arrow param `count` shadows the outer ref inside the block body, so
        // the `count > 0` comparison is the plain param value, not the ref.
        let sfc = "<script setup>\nconst count = ref(0)\nconst handler = (count) => {\n  if (count > 0) { return count }\n}\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_destructured_param_shadowing_ref() {
        // An array-destructured `watch` callback param `[index, inner]` binds the
        // outer-ref name `inner`; inside the body `inner === null` is the
        // destructured plain value — proving destructured-param tracking applies.
        let sfc = "<script setup>\nconst inner = ref(null)\nwatch([a, b], ([index, inner]) => {\n  if (inner === null) { return a }\n  return b\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_unshadowed_bare_ref() {
        // No shadowing param: the top-scope `if (visible)` is a genuine bare-ref
        // misuse and must still flag.
        let sfc = "<script setup>\nconst visible = ref(false)\nif (visible) {}\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_ref_misuse_in_function_without_param_shadow() {
        // The function param is `flag`, not `visible`, so the bare `visible`
        // comparison inside the body is still the outer ref and must flag.
        let sfc = "<script setup>\nconst visible = ref(false)\nfunction check(flag) {\n  if (visible) { return flag }\n}\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_local_const_shadow_in_function() {
        // A local `const placeholder` declared before the comparison shadows the
        // outer ref; `placeholder === 'x'` after it is the plain local value.
        let sfc = "<script setup>\nconst placeholder = ref(0)\nfunction f() {\n  const placeholder = props.placeholder\n  if (placeholder === 'x') { return 1 }\n}\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_bare_ref_before_local_redeclaration() {
        // The comparison precedes the local `const placeholder`, so it is still
        // the outer ref (position-aware shadowing) and must flag.
        let sfc = "<script setup>\nconst placeholder = ref(0)\nfunction f() {\n  if (placeholder === 'x') { return 1 }\n  const placeholder = 2\n}\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_unparenthesized_arrow_param_shadowing_ref() {
        // lyswhut/lx-music-desktop repro (#7703): an unparenthesized single-param
        // arrow `offset => { … }` shadows the outer `const offset = ref(0)`, so
        // `offset == originOffset.value` inside is the plain param, not the ref.
        let sfc = "<script setup>\nconst offset = ref(0)\nconst originOffset = ref(0)\nconst updateLyric = offset => {\n  if (offset == originOffset.value) { removeLyric() }\n}\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_local_let_in_unparenthesized_arrow_shadowing_ref() {
        // Same repro (#7703): inside `getOffset = lrc => { … }` a local
        // `let offset` shadows the outer ref; `if (offset)` after it is the plain
        // local value.
        let sfc = "<script setup>\nconst offset = ref(0)\nconst getOffset = lrc => {\n  let offset = compute(lrc)\n  if (offset) { return offset }\n  return offset\n}\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_bare_ref_outside_unparenthesized_arrow_body() {
        // The arrow shadows `offset` only inside its body; the top-scope
        // `if (offset)` after it is still the outer ref and must flag.
        let sfc = "<script setup>\nconst offset = ref(0)\nconst updateLyric = offset => {\n  return offset\n}\nif (offset) {}\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }
}
