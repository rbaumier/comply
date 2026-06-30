//! vue-no-ref-as-operand text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_shadow_scope::collect_shadow_scopes;
use rustc_hash::FxHashSet;

#[derive(Debug)]
pub struct Check;

/// Collect identifiers bound to a `ref(...)` / `shallowRef(...)` /
/// `computed(...)` call in the source. Heuristic: look for
/// `const X = ref(...)` patterns.
fn collect_ref_bindings(source: &str) -> FxHashSet<String> {
    let mut bindings = FxHashSet::default();
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
        // Blank every SFC block that is not `<script>`/`<template>` — custom
        // blocks (`<docs>`, `<i18n>`, `<config>`, …) and `<style>` hold
        // documentation / i18n JSON / CSS, not executable code, so a ref name
        // appearing there is prose and must not be analyzed. Non-SFC sources
        // (plain script) pass through unchanged. The mask is byte-length- and
        // newline-preserving, so all offset-based checks below stay valid.
        let code_only = crate::rules::vue_sfc::mask_non_code_blocks(ctx.source);
        let bindings = collect_ref_bindings(&code_only);
        if bindings.is_empty() {
            return Vec::new();
        }
        // Then blank every `<!-- ... -->` HTML comment, every `//` and
        // `/* ... */` (incl. `/** ... */` JSDoc) JS/TS comment, so a ref name
        // that appears only in prose — template comment or `<script>` doc
        // comment — can't be matched as an operand. `mask_comments` skips string
        // literals, so a comment marker inside a string is left intact. All
        // masks are byte-length-preserving, so all offset-based checks below
        // stay valid.
        let scan_source = crate::oxc_helpers::mask_comments(
            &crate::rules::vue_template_helpers::mask_html_comments(&code_only),
        );
        let shadow_scopes = collect_shadow_scopes(ctx.source);
        // Vue 3 auto-unwraps top-level refs/computed inside `<template>` expressions,
        // so a bare ref name there is correct (`.value` would be wrong). Suppress any
        // match whose offset falls inside the root `<template>` block.
        let template_range =
            crate::rules::vue_template_helpers::extract_template(ctx.source).map(|t| {
                let start = t.as_ptr() as usize - ctx.source.as_ptr() as usize;
                start..start + t.len()
            });
        let mut diagnostics = Vec::new();
        // Look for `<name> + ` / `<name> ===` / `<name>++` / `<name>--`
        // patterns where the binding is used like a primitive.
        for name in &bindings {
            for (i, _) in scan_source.match_indices(name.as_str()) {
                // A same-named function/arrow parameter, or a local
                // `const`/`let`/`var` declared earlier in the body, shadows the
                // outer ref inside that scope; the bare name there is the plain
                // local value, not the ref, so it is not a misuse. The local
                // shadows only for usages textually after its declaration.
                if shadow_scopes.iter().any(|s| {
                    s.body.contains(&i)
                        && (s.params.contains(name)
                            || s.locals.iter().any(|(n, d)| n == name && *d < i))
                }) {
                    continue;
                }
                if template_range.as_ref().is_some_and(|r| r.contains(&i)) {
                    continue;
                }
                // Word boundary on left.
                let prev_ok = i == 0
                    || scan_source.as_bytes()[i - 1].is_ascii_whitespace()
                    || matches!(
                        scan_source.as_bytes()[i - 1],
                        b'(' | b'[' | b'{' | b',' | b';' | b'=' | b'+' | b'-' | b'!'
                    );
                if !prev_ok {
                    continue;
                }
                let end = i + name.len();
                if end >= scan_source.len() {
                    continue;
                }
                let after = &scan_source[end..];
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

    #[test]
    fn allows_ref_as_operand_inside_template() {
        // Vue auto-unwraps `activeIndex` in the template; the bare name is correct.
        let src = "<script setup lang=\"ts\">\nconst activeIndex = ref(0)\n</script>\n<template>\n  <div :class=\"{ 'x': activeIndex === index }\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_computed_as_operand_inside_template() {
        let src = "<script setup lang=\"ts\">\nconst count = computed(() => 0)\n</script>\n<template>\n  <div v-if=\"count > 1\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_local_const_shadowing_ref() {
        // The nuxt/image repro: an outer computed ref `placeholder`, shadowed by
        // a local `const placeholder` inside the callback. The comparison-operand
        // usage after the local decl is the plain value, not the ref.
        let src = "const placeholder = computed(() => {\n  const placeholder = props.placeholder === '' ? [10, 10] : props.placeholder\n  if (placeholder === 'string') { return placeholder }\n  return false\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_local_let_shadowing_ref() {
        let src = "const count = computed(() => { let count = props.count; return count + 1; })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_ref_misuse_without_local_redeclaration() {
        // `count` is the outer ref inside `f` — no local redeclaration shadows it.
        let src = "const count = ref(0);\nfunction f() { return count + 1; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ref_misuse_before_local_redeclaration() {
        // The usage precedes the local `const count`, so it is still the outer
        // ref (position-aware shadowing): the misuse must still flag.
        let src = "const count = ref(0);\nfunction f() {\n  const x = count + 1;\n  const count = 2;\n  return x + count;\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ref_misuse_despite_destructuring_local_decl() {
        // A destructuring `const { count }` binds no bare `count` identifier the
        // scanner tracks, so the bare `count` operand stays the outer ref.
        let src = "const count = ref(0);\nfunction f() {\n  const { other } = obj;\n  return count + 1;\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ref_name_before_html_comment_close() {
        // vue-echarts repro shape: a ref name sits right before an `<!-- ... -->`
        // comment's closing `-->`, which the scanner misread as a `--` decrement.
        // No extractable root template here, so `template_range` suppression does
        // not apply — only masking the comment prevents the false positive.
        let src = "<!-- disable html -->\n<script setup lang=\"ts\">\nconst html = ref(\"\");\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_genuine_ref_misuse_alongside_comment() {
        // A real `count + 1` misuse in `<script>` must still flag even when the
        // ref name also appears in a masked HTML comment whose `-->` is masked.
        let src = "<!-- count -->\n<script setup lang=\"ts\">\nconst count = ref(0)\nconst doubled = count + 1\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ref_name_in_jsdoc_block_comment() {
        // radix-vue repro: `boundary` is a `computed`, but its only occurrence
        // outside the declaration is English prose ending a `/** … */` JSDoc
        // line. The next line's ` * ` continuation marker was misread as a `* `
        // multiply operand. Masking the JS/TS comment removes the match.
        let src = concat!(
            "<script lang=\"ts\">\n",
            "export interface PopperContentProps {\n",
            "  /**\n",
            "   * keep the content in the boundary\n",
            "   * regardless of the trigger position.\n",
            "   */\n",
            "  sticky?: 'partial' | 'always'\n",
            "}\n",
            "const boundary = computed(() => 0)\n",
            "</script>",
        );
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ref_name_in_line_comment() {
        // A ref name in a `//` line comment, used in an operand-shaped phrase,
        // must not be flagged.
        let src = "<script setup lang=\"ts\">\nconst count = ref(0)\n// count + 1 is documented below\nconst doubled = count.value + 1\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_genuine_ref_misuse_alongside_jsdoc_comment() {
        // A real `count + 1` misuse must still flag even when the ref name also
        // appears as prose inside a `/** … */` JSDoc comment.
        let src = "<script setup lang=\"ts\">\n/** count + 1 explained */\nconst count = ref(0)\nconst doubled = count + 1\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_comparison_operand_in_fragment_template_with_nested_template() {
        // vue-flow repro: `<script setup>` (no `lang`) precedes a multi-root
        // (fragment) `<template>` whose first child wraps a nested
        // `<template v-for>`. The `result > 0` comparison in a later sibling's
        // `:style` binding is auto-unwrapped in the template and must not flag.
        let src = "<script setup>\nconst result = computed(() => 0)\n</script>\n\n<template>\n  <div>\n    <template v-for=\"(v, i) in items\" :key=\"i\">\n      <span>{{ v }}</span>\n    </template>\n  </div>\n  <span :style=\"{ color: result > 0 ? 'a' : 'b' }\">{{ result }}</span>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_comparison_operand_in_template_with_scoped_slot() {
        // A nested scoped-slot `<template #name>` must not confuse the
        // root-template detection: the `count < limit` comparison in the
        // template stays exempt.
        let src = "<script setup>\nconst count = ref(0)\nconst limit = ref(10)\n</script>\n<template>\n  <Foo>\n    <template #header>\n      <span>head</span>\n    </template>\n  </Foo>\n  <div v-if=\"count < limit\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_ref_misuse_in_script_with_template_present() {
        // The misuse is in `<script>`, where `.value` IS required; the template
        // skip must not over-suppress script-context misuse.
        let src = "<script setup lang=\"ts\">\nconst count = ref(0)\nconst doubled = count + 1\n</script>\n<template>\n  <div />\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ref_name_in_docs_custom_block() {
        // ant-design-vue repro: the ref name appears as YAML/Markdown prose in a
        // `<docs>` custom block (`en-US: set ... maxTagTextLength`). That is not
        // code — masking non-script/template blocks removes the match.
        let src = concat!(
            "<docs>\n",
            "---\n",
            "title:\n",
            "  en-US: set maxTagCount or maxTagTextLength\n",
            "---\n",
            "</docs>\n",
            "<script setup lang=\"ts\">\n",
            "const maxTagTextLength = ref(10);\n",
            "</script>\n",
            "<template>\n",
            "  <a-button @click=\"maxTagTextLength++\">x</a-button>\n",
            "</template>",
        );
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ref_name_in_i18n_custom_block() {
        // An `<i18n>` block whose text places the ref name in an operand-shaped
        // position (` count ++`, preceded by whitespace so the left boundary
        // matches) would be flagged if scanned. Masking the block removes it;
        // the genuine usage in `<script>` correctly uses `.value`.
        let src = concat!(
            "<i18n>\n",
            "increment by count ++ steps\n",
            "</i18n>\n",
            "<script setup lang=\"ts\">\n",
            "const count = ref(0)\n",
            "const doubled = count.value + 1\n",
            "</script>",
        );
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_genuine_ref_misuse_alongside_custom_block() {
        // A real `count + 1` misuse in `<script setup>` must still flag exactly
        // once even when the same ref name also appears in an operand-shaped
        // phrase (` count ++`) inside a `<docs>` custom block: the masked prose
        // is ignored, the script misuse is not.
        let src = concat!(
            "<docs>\n",
            "incremented as count ++ here\n",
            "</docs>\n",
            "<script setup lang=\"ts\">\n",
            "const count = ref(0)\n",
            "const doubled = count + 1\n",
            "</script>",
        );
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_array_destructured_param_shadowing_ref() {
        // varletjs/varlet repro: an outer `const inner = ref(null)`, shadowed by
        // the array-destructured `watch` callback params
        // `([index, inner], [oldIndex, oldInner])`. Inside the callback `inner`
        // is the destructured plain value, not the ref, so `inner === oldInner`
        // must not be flagged.
        let src = "const inner = ref(null);\nwatch([activeItemIndex, () => props.isInner], ([index, inner], [oldIndex, oldInner]) => {\n  const isSame = inner === oldInner;\n  const x = inner === oldIndex ? a : b;\n});";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_object_destructured_param_shadowing_ref() {
        // An outer ref `inner`, shadowed by an object-destructured arrow param
        // `{ inner }`. The bare `inner` operand inside is the destructured plain
        // value, not the ref, so the comparison must not be flagged.
        let src = "const inner = ref(false);\nconst f = ({ inner }) => { const same = inner === other; };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_renamed_object_destructured_param_shadowing_ref() {
        // A renamed object-destructured param `{ isInner: inner }` binds the
        // outer-ref name `inner` (the value, not the key). The bare `inner`
        // comparison inside must not be flagged.
        let src = "const inner = ref(false);\nconst f = ({ isInner: inner }) => { return inner === flag; };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_ref_misuse_when_destructured_param_binds_other_name() {
        // The destructured param `{ isInner: flag }` binds `flag`, NOT `inner`,
        // so the bare `inner` operand inside the callback is still the outer ref
        // and the misuse must flag. (Default-value expressions like `= inner`
        // are likewise not treated as bindings.)
        let src = "const inner = ref(false);\nconst f = ({ isInner: flag = inner }) => { return inner === flag; };";
        assert_eq!(run(src).len(), 1);
    }
}
