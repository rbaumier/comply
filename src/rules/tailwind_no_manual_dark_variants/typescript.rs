use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value};

const RAW_COLORS: &[&str] = &[
    "white", "black", "slate", "gray", "zinc", "neutral", "stone", "red", "orange", "amber",
    "yellow", "lime", "green", "emerald", "teal", "cyan", "sky", "blue", "indigo", "violet",
    "purple", "fuchsia", "pink", "rose",
];

const COLOR_PREFIXES: &[&str] = &["bg-", "text-", "border-", "ring-", "fill-", "stroke-"];

fn is_raw_color_base(base: &str) -> bool {
    for prefix in COLOR_PREFIXES {
        let Some(rest) = base.strip_prefix(prefix) else {
            continue;
        };
        if RAW_COLORS.contains(&rest) {
            return true;
        }
        if let Some((color, shade)) = rest.rsplit_once('-')
            && RAW_COLORS.contains(&color)
            && shade.chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let name = jsx_attribute_name(node, source).unwrap_or("");
    if name != "className" && name != "class" { return; }
    let Some(value) = jsx_attribute_string_value(node, source) else { return };

    // Flag any token of the form `dark:<color-utility>` whose base is a raw
    // palette color (e.g. `dark:bg-zinc-900`, `dark:text-white`).
    let has_dark_raw = value.split_whitespace().any(|tok| {
        let Some(rest) = tok.strip_prefix("dark:") else { return false };
        is_raw_color_base(rest)
    });
    if !has_dark_raw { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Manual `dark:` variant with a raw palette color — use a semantic token (bg-background, text-foreground, …) that already resolves per theme.".into(),
        Severity::Warning,
    ));
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
    fn flags_dark_bg_raw() {
        assert_eq!(
            run(r#"export const A = () => <div className="bg-white dark:bg-zinc-900" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_dark_text_raw() {
        assert_eq!(
            run(r#"export const A = () => <div className="dark:text-gray-100" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_semantic_token() {
        assert!(
            run(r#"export const A = () => <div className="bg-background text-foreground" />;"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_dark_on_semantic_token() {
        // `dark:bg-muted` is fine — `muted` is a token, not a raw color.
        assert!(
            run(r#"export const A = () => <div className="bg-card dark:bg-muted" />;"#).is_empty()
        );
    }
}
