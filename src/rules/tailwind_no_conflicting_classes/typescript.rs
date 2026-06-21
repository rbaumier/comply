//! tailwind-no-conflicting-classes — flag mutually exclusive Tailwind
//! utility classes (e.g. `p-4 p-6`).
//!
//! Runs only in projects that use Tailwind CSS; other CSS frameworks reuse
//! prefixes like `text-` / `p-` for their own utilities, so firing without
//! Tailwind produces false positives.
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). Groups class tokens by their conflict prefix
//! (`p-`, `px-`, `bg-`, …) or by membership in the `display` group; if a
//! group has 2+ entries, it reports the conflict.

use rustc_hash::FxHashMap;

use crate::diagnostic::{Diagnostic, Severity};

use super::conflict_key;

fn jsx_class_value<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "jsx_attribute" {
        return None;
    }
    let name = crate::rules::jsx::jsx_attribute_name(node, source)?;
    if name != "className" && name != "class" {
        return None;
    }
    crate::rules::jsx::jsx_attribute_string_value(node, source)
}

fn vue_class_value<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "attribute" {
        return None;
    }
    let mut cursor = node.walk();
    let mut name: Option<&'a str> = None;
    let mut value: Option<&'a str> = None;
    for child in node.children(&mut cursor) {
        match child.kind() {
            "attribute_name" => name = child.utf8_text(source).ok(),
            "quoted_attribute_value" => {
                let mut vc = child.walk();
                value = child
                    .children(&mut vc)
                    .find(|c| c.kind() == "attribute_value")
                    .and_then(|c| c.utf8_text(source).ok());
            }
            _ => {}
        }
    }
    if name? != "class" {
        return None;
    }
    value
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !ctx.project.uses_tailwind() {
        return;
    }
    let class_str = jsx_class_value(node, source)
        .or_else(|| vue_class_value(node, source));
    let Some(class_str) = class_str else { return; };
    let classes: Vec<&str> = class_str.split_whitespace().collect();
    let mut groups: FxHashMap<&str, Vec<&str>> = FxHashMap::default();
    for class in &classes {
        if let Some(key) = conflict_key(class) {
            groups.entry(key).or_default().push(class);
        }
    }
    for (prefix, members) in &groups {
        if members.len() >= 2 {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!(
                    "Conflicting `{prefix}` classes: {} — keep only one.",
                    members.join(", "),
                ),
                Severity::Warning,
            ));
        }
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::empty_with_tailwind();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.tsx", &project, file)
    }

    fn run_vue(source: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::empty_with_tailwind();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.vue", &project, file)
    }

    #[test]
    fn silent_on_quasar_classes_without_tailwind() {
        // Regression for rbaumier/comply#4710 — `text-h6` (Quasar typography)
        // and `text-weight-bold` (Quasar font weight) are Quasar CSS utilities,
        // not Tailwind classes. The Quasar playground has no Tailwind dependency,
        // so the rule must not fire on a non-Tailwind project.
        let source = r#"<template>
  <div class="text-h6 text-weight-bold" />
</template>"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, source, "t.vue").is_empty());
    }

    #[test]
    fn silent_on_vuetify_typography_emphasis_in_vue() {
        // Regression for rbaumier/comply#4878 — `text-title-large` (Material
        // typography scale) and `text-medium-emphasis` (emphasis opacity) are
        // Vuetify utilities, not Tailwind text-color classes. They control
        // orthogonal style dimensions and must not be reported as conflicting.
        let source = r#"<template>
  <h6 class="text-title-large font-weight-regular text-medium-emphasis my-5">x</h6>
</template>"#;
        assert!(run_vue(source).is_empty());
    }

    #[test]
    fn flags_two_text_sizes_in_vue() {
        // Genuine same-property conflict (two font-sizes) still fires.
        let source = r#"<template>
  <div class="text-lg text-2xl" />
</template>"#;
        assert_eq!(run_vue(source).len(), 1);
    }

    #[test]
    fn allows_text_size_with_text_align_in_vue() {
        // Two text-* utilities on orthogonal properties (font-size + alignment)
        // do not conflict.
        let source = r#"<template>
  <div class="text-lg text-center" />
</template>"#;
        assert!(run_vue(source).is_empty());
    }

    #[test]
    fn allows_bg_cover_center_no_repeat_in_vue() {
        // Regression for rbaumier/comply#4487 — `bg-cover` (size),
        // `bg-center` (position) and `bg-no-repeat` (repeat) set distinct
        // CSS sub-properties; the idiomatic full-cover-image combo must not
        // conflict. This is the real `.vue` reproduction from the issue.
        let source = r#"<template>
  <div class="h-[270px] border-b border-base bg-cover bg-center bg-no-repeat" />
</template>"#;
        assert!(run_vue(source).is_empty());
    }

    #[test]
    fn allows_bg_clip_with_bg_color_in_vue() {
        // Regression for rbaumier/comply#5041 — `bg-white` (background-color)
        // and `bg-clip-padding` (background-clip) target distinct CSS
        // properties; `bg-clip-*` utilities exist to be combined with a
        // background-color, so the pair must not conflict. This is the real
        // `.vue` reproduction from headlessui.
        let source = r#"<template>
  <Combobox class="shadow-xs w-full overflow-hidden rounded-sm border border-black/5 bg-white bg-clip-padding" />
</template>"#;
        assert!(run_vue(source).is_empty());
    }

    #[test]
    fn allows_bg_size_with_bg_color_in_vue() {
        // A second non-conflicting `bg-` pair: `bg-cover` (background-size)
        // and `bg-red-500` (background-color) are orthogonal.
        let source = r#"<template>
  <div class="bg-cover bg-red-500" />
</template>"#;
        assert!(run_vue(source).is_empty());
    }

    #[test]
    fn flags_conflicting_bg_color_in_vue() {
        // Two background-color utilities still conflict.
        let source = r#"<template>
  <div class="bg-red-500 bg-blue-500" />
</template>"#;
        assert_eq!(run_vue(source).len(), 1);
    }

    #[test]
    fn allows_bg_cover_center_no_repeat() {
        assert!(run(r#"const x = <div className="bg-cover bg-center bg-no-repeat" />;"#).is_empty());
    }

    #[test]
    fn flags_conflicting_padding() {
        let diags = run(r#"const x = <div className="p-4 p-6" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("p-"));
    }

    #[test]
    fn flags_conflicting_text_size() {
        let diags = run(r#"const x = <div className="text-sm text-lg" />;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_conflicting_bg() {
        let diags = run(r#"const x = <div className="bg-red-500 bg-blue-500" />;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_display_conflict() {
        let diags = run(r#"const x = <div className="flex hidden" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("display"));
    }

    #[test]
    fn allows_non_conflicting() {
        assert!(run(r#"const x = <div className="p-4 mt-2 text-lg" />;"#).is_empty());
    }

    #[test]
    fn allows_text_size_with_text_wrap() {
        assert!(run(r#"const x = <div className="text-2xl text-balance" />;"#).is_empty());
    }

    #[test]
    fn allows_text_color_with_text_wrap() {
        assert!(
            run(r#"const x = <div className="text-muted-foreground text-pretty" />;"#).is_empty()
        );
    }

    #[test]
    fn allows_flex_shorthand_with_flex_direction() {
        assert!(run(r#"const x = <div className="flex-1 flex-col" />;"#).is_empty());
    }

    #[test]
    fn allows_border_side_with_border_color() {
        assert!(run(r#"const x = <div className="border-b border-border" />;"#).is_empty());
    }

    #[test]
    fn allows_text_sm_with_text_muted() {
        assert!(run(r#"const x = <div className="text-sm text-muted-foreground" />;"#).is_empty());
    }

    #[test]
    fn flags_two_text_sizes() {
        assert_eq!(
            run(r#"const x = <div className="text-sm text-2xl" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_two_flex_directions() {
        assert_eq!(
            run(r#"const x = <div className="flex-row flex-col" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_two_border_colors() {
        assert_eq!(
            run(r#"const x = <div className="border-red-500 border-blue-500" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_two_font_weights() {
        assert_eq!(
            run(r#"const x = <div className="font-bold font-light" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_arbitrary_size_with_color() {
        assert!(
            run(r#"const x = <div className="text-[10px] text-muted-foreground" />;"#).is_empty()
        );
    }

    #[test]
    fn flags_two_arbitrary_sizes() {
        assert_eq!(
            run(r#"const x = <div className="text-[10px] text-[1.5rem]" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_two_arbitrary_colors() {
        assert_eq!(
            run(r#"const x = <div className="text-[#ff0000] text-[#00ff00]" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_arbitrary_color_with_named_size() {
        assert!(run(r#"const x = <div className="text-[#ff0000] text-lg" />;"#).is_empty());
    }

    #[test]
    fn allows_gap_x_with_gap_y() {
        // Regression for rbaumier/comply#4072 — `gap-x-*` (column-gap) and
        // `gap-y-*` (row-gap) control different axes and are designed to
        // coexist, so they must not conflict.
        assert!(
            run(r#"const x = <div className="grid grid-cols-2 gap-x-8 gap-y-4 px-6 pb-6" />;"#)
                .is_empty()
        );
    }

    #[test]
    fn flags_conflicting_gap_x_same_axis() {
        let diags = run(r#"const x = <div className="gap-x-4 gap-x-8" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("gap-x-"));
    }

    #[test]
    fn flags_conflicting_gap_y_same_axis() {
        let diags = run(r#"const x = <div className="gap-y-2 gap-y-6" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("gap-y-"));
    }

    #[test]
    fn flags_conflicting_gap_shorthand() {
        let diags = run(r#"const x = <div className="gap-2 gap-6" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("gap-2, gap-6"));
    }
}
