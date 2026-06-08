//! Flag raw Tailwind color utilities inside JSX `className` values.
//!
//! Matches classes of the shape `<prefix>-<color>-<shade>` where
//! `<color>` is one of the built-in Tailwind palette names and
//! `<shade>` is a 2–3 digit number (50, 100, …, 950). Prefixes
//! covered: `bg`, `text`, `border`, `ring`, `fill`, `stroke`,
//! `from`, `to`, `via`, `divide`, `outline`, `accent`, `caret`,
//! `placeholder`, `shadow`, `decoration`.

use crate::diagnostic::{Diagnostic, Severity};

const COLOR_PREFIXES: &[&str] = &[
    "bg",
    "text",
    "border",
    "ring",
    "fill",
    "stroke",
    "from",
    "to",
    "via",
    "divide",
    "outline",
    "accent",
    "caret",
    "placeholder",
    "shadow",
    "decoration",
];

const COLORS: &[&str] = &[
    "slate", "gray", "zinc", "neutral", "stone", "red", "orange", "amber", "yellow", "lime",
    "green", "emerald", "teal", "cyan", "sky", "blue", "indigo", "violet", "purple", "fuchsia",
    "pink", "rose",
];

fn is_raw_color_class(class: &str) -> bool {
    // Strip a responsive/state prefix like `hover:`, `md:`, `dark:` — we only
    // look at the final utility segment. `dark:` overrides are handled by a
    // sibling rule.
    let utility = class.rsplit(':').next().unwrap_or(class);

    // Strip a leading `!` important modifier and `-` negative modifier.
    let utility = utility.trim_start_matches('!').trim_start_matches('-');

    let mut parts = utility.split('-');
    let Some(prefix) = parts.next() else {
        return false;
    };
    if !COLOR_PREFIXES.contains(&prefix) {
        return false;
    }
    let Some(color) = parts.next() else {
        return false;
    };
    if !COLORS.contains(&color) {
        return false;
    }
    let Some(shade) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    shade.len() >= 2 && shade.len() <= 3 && shade.chars().all(|c| c.is_ascii_digit())
}

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    if crate::rules::jsx::jsx_attribute_name(node, source) != Some("className") {
        return;
    }
    let Some(value) = crate::rules::jsx::jsx_attribute_string_value(node, source) else {
        return;
    };
    if value.split_ascii_whitespace().any(is_raw_color_class) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`className` uses raw Tailwind colors — switch to shadcn semantic tokens (`bg-primary`, `text-muted-foreground`, …).".into(),
            Severity::Warning,
        ));
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_bg_blue_500() {
        assert_eq!(
            run(r#"const x = <div className="bg-blue-500">x</div>;"#).len(),
            1
        );
    }

    #[test]
    fn flags_text_gray_600() {
        assert_eq!(
            run(r#"const x = <span className="text-gray-600">x</span>;"#).len(),
            1
        );
    }

    #[test]
    fn flags_mixed_with_other_utilities() {
        assert_eq!(
            run(r#"const x = <div className="p-4 bg-red-100 rounded">x</div>;"#).len(),
            1
        );
    }

    #[test]
    fn allows_semantic_tokens() {
        assert!(
            run(r#"const x = <div className="bg-primary text-muted-foreground">x</div>;"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_non_color_utilities() {
        assert!(run(r#"const x = <div className="p-4 rounded-md flex">x</div>;"#).is_empty());
    }
}
