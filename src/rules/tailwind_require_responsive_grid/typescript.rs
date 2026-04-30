use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value};

const BREAKPOINTS: &[&str] = &["sm:", "md:", "lg:", "xl:", "2xl:"];

/// Parse the column count from a `grid-cols-N` token.
fn cols_count(tok: &str) -> Option<u32> {
    tok.strip_prefix("grid-cols-")?.parse::<u32>().ok()
}

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let name = jsx_attribute_name(node, source).unwrap_or("");
    if name != "className" && name != "class" { return; }
    let Some(value) = jsx_attribute_string_value(node, source) else { return };

    let mut base_cols: Option<u32> = None;
    let mut has_responsive_cols = false;

    for tok in value.split_whitespace() {
        if let Some(bp) = BREAKPOINTS.iter().find(|bp| tok.starts_with(**bp)) {
            let after = &tok[bp.len()..];
            if after.starts_with("grid-cols-") {
                has_responsive_cols = true;
            }
            continue;
        }
        if let Some(n) = cols_count(tok) {
            base_cols = Some(n);
        }
    }

    // Only flag when the unprefixed base is >= 2 AND there's no responsive
    // alternative. `grid-cols-1 md:grid-cols-3` passes (base is 1).
    let Some(base) = base_cols else { return };
    if base < 2 { return; }
    if has_responsive_cols { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`grid-cols-2+` without a mobile-first fallback. Prefer `grid-cols-1 md:grid-cols-N` so the grid collapses on small screens.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_grid_cols_3_no_responsive() {
        assert_eq!(
            run(r#"export const A = () => <div className="grid grid-cols-3" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_grid_cols_2_no_responsive() {
        assert_eq!(
            run(r#"export const A = () => <div className="grid grid-cols-2 gap-4" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_mobile_first_pair() {
        assert!(
            run(r#"export const A = () => <div className="grid grid-cols-1 md:grid-cols-3" />;"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_only_responsive() {
        assert!(
            run(r#"export const A = () => <div className="grid md:grid-cols-3" />;"#).is_empty()
        );
    }

    #[test]
    fn allows_grid_cols_1() {
        assert!(run(r#"export const A = () => <div className="grid grid-cols-1" />;"#).is_empty());
    }
}
