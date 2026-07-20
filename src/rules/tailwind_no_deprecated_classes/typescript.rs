//! tailwind-no-deprecated-classes — flag deprecated Tailwind utility
//! classes that were removed or renamed in v3/v4.
//!
//! Runs only in projects that use Tailwind CSS; UnoCSS/Windi presets keep
//! classes like `flex-shrink-0` as first-class utilities, so the v3/v4
//! deprecation rename applies only to real Tailwind projects.
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). For each `class`/`className` attribute, splits
//! the value on whitespace, strips Tailwind variant prefixes (`hover:`,
//! `md:`) and the `!` important modifier, and reports any token whose
//! base form matches the deprecation table.

use crate::diagnostic::{Diagnostic, Severity};

/// Deprecated class → recommended replacement.
const DEPRECATED: &[(&str, &str)] = &[
    ("flex-grow-0", "grow-0"),
    ("flex-grow", "grow"),
    ("flex-shrink-0", "shrink-0"),
    ("flex-shrink", "shrink"),
    ("overflow-ellipsis", "text-ellipsis"),
    ("decoration-slice", "box-decoration-slice"),
    ("decoration-clone", "box-decoration-clone"),
];

fn replacement_for(class: &str) -> Option<&'static str> {
    DEPRECATED
        .iter()
        .find(|(dep, _)| *dep == class)
        .map(|(_, repl)| *repl)
}

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
    for class in class_str.split_whitespace() {
        let base = class.rsplit(':').next().unwrap_or(class);
        let base = base.strip_prefix('!').unwrap_or(base);
        if let Some(replacement) = replacement_for(base) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!("Deprecated Tailwind class `{base}` — use `{replacement}` instead."),
                Severity::Error,
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

    fn run(src: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::empty_with_tailwind();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", &project, file)
    }

    #[test]
    fn silent_on_unocss_presetwind3_vue_without_tailwind() {
        // Regression for rbaumier/comply#7900 — UnoCSS `presetWind3` targets
        // Tailwind v3 semantics where `flex-shrink-0` is a first-class utility.
        // The v3/v4 deprecation rename applies only to real Tailwind projects,
        // so the rule must stay silent on a UnoCSS/Windi project.
        let source = r#"<template>
  <div class="flex-1 flex-shrink-0" />
</template>"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, source, "t.vue").is_empty());
    }

    #[test]
    fn flags_flex_grow_0() {
        let diags = run(r#"const x = <div className="flex-grow-0" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("grow-0"));
    }

    #[test]
    fn flags_overflow_ellipsis() {
        let diags = run(r#"const x = <div className="truncate overflow-ellipsis" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("text-ellipsis"));
    }

    #[test]
    fn flags_decoration_clone() {
        let diags = run(r#"const x = <div className="decoration-clone" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("box-decoration-clone"));
    }

    #[test]
    fn flags_with_variant() {
        let diags = run(r#"const x = <div className="hover:flex-shrink" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("shrink"));
    }

    #[test]
    fn allows_current_classes() {
        assert!(run(r#"const x = <div className="grow shrink p-4 text-ellipsis" />;"#).is_empty());
    }

    #[test]
    fn flags_in_class_attr() {
        let diags = run(r#"const x = <div class="flex-shrink-0" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("shrink-0"));
    }

    #[test]
    fn allows_overflow_clip() {
        // overflow-clip is a valid Tailwind utility (overflow: clip), not deprecated.
        // It is distinct from text-clip (text-overflow: clip).
        assert!(run(r#"const x = <div className="overflow-clip" />;"#).is_empty());
    }
}
