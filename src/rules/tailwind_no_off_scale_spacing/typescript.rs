use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value};
use std::sync::LazyLock;

const SPACING_PREFIXES: &[&str] = &[
    "p-", "px-", "py-", "pt-", "pb-", "pl-", "pr-", "ps-", "pe-", "m-", "mx-", "my-", "mt-", "mb-",
    "ml-", "mr-", "ms-", "me-", "gap-", "gap-x-", "gap-y-", "space-x-", "space-y-",
];

/// Prefixes ordered longest-first so the most specific match wins: `gap-x-8`
/// must strip `gap-x-` (leaving `8`), not the shorter `gap-` (leaving `x-8`).
static PREFIXES_BY_LEN_DESC: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut prefixes = SPACING_PREFIXES.to_vec();
    prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
    prefixes
});

/// On-scale values. 0, 0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4, 6, 8, 10, 12, 14, 16,
/// 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60, 64, 72, 80, 96 are the
/// canonical Tailwind spacing steps mapped to the 4pt grid. Odd multiples
/// (5, 7, 9, 11, 13, 15) break the grid and almost always come from
/// pixel-pushing.
const ON_SCALE: &[&str] = &[
    "0", "px", "0.5", "1", "1.5", "2", "2.5", "3", "3.5", "4", "6", "8", "10", "12", "14", "16",
    "20", "24", "28", "32", "36", "40", "44", "48", "52", "56", "60", "64", "72", "80", "96",
];

fn is_off_scale(base: &str) -> bool {
    for prefix in PREFIXES_BY_LEN_DESC.iter() {
        let Some(rest) = base.strip_prefix(prefix) else {
            continue;
        };
        // Skip negative, auto, full, and arbitrary values — they're either
        // fine (`auto`, `full`) or covered by a separate rule (arbitrary).
        if rest.starts_with('-') || rest == "auto" || rest == "full" || rest.starts_with('[') {
            return false;
        }
        return !ON_SCALE.contains(&rest);
    }
    false
}

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let name = jsx_attribute_name(node, source).unwrap_or("");
    if name != "className" && name != "class" { return; }
    let Some(value) = jsx_attribute_string_value(node, source) else { return };

    let off = value.split_whitespace().any(|tok| {
        let base = tok.rsplit(':').next().unwrap_or(tok);
        is_off_scale(base)
    });
    if !off { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Off-scale spacing value — snap to the 4pt grid (…4, 6, 8, 10, 12, 16…) to stay on the design system.".into(),
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
    fn flags_p_5() {
        assert_eq!(
            run(r#"export const A = () => <div className="p-5" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_mb_7() {
        assert_eq!(
            run(r#"export const A = () => <div className="mb-7" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_gap_9() {
        assert_eq!(
            run(r#"export const A = () => <div className="gap-9" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_on_scale() {
        assert!(run(r#"export const A = () => <div className="p-4 mb-6 gap-8" />;"#).is_empty());
    }

    #[test]
    fn allows_half_step() {
        assert!(run(r#"export const A = () => <div className="p-2.5" />;"#).is_empty());
    }

    #[test]
    fn allows_on_scale_axis_gaps() {
        // Regression for rbaumier/comply#4194 — `gap-x-8`/`gap-y-4` must strip
        // the longest matching prefix (`gap-x-`/`gap-y-`), not the shorter
        // `gap-`, which would read the axis letter as part of an off-scale value.
        assert!(
            run(
                r#"export const A = () => <dl className="grid grid-cols-2 gap-x-8 gap-y-4 px-6 pb-6 sm:grid-cols-3 lg:grid-cols-4" />;"#
            )
            .is_empty()
        );
        assert!(run(r#"export const A = () => <div className="space-x-4 space-y-2" />;"#).is_empty());
    }

    #[test]
    fn flags_off_scale_axis_gaps() {
        assert_eq!(run(r#"export const A = () => <div className="gap-x-5" />;"#).len(), 1);
        assert_eq!(run(r#"export const A = () => <div className="gap-y-5" />;"#).len(), 1);
    }
}
