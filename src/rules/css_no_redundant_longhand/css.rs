use crate::diagnostic::{Diagnostic, Severity};

const GROUPS: &[(&str, [&str; 4])] = &[
    (
        "margin",
        ["margin-top", "margin-right", "margin-bottom", "margin-left"],
    ),
    (
        "padding",
        [
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
        ],
    ),
    (
        "border-width",
        [
            "border-top-width",
            "border-right-width",
            "border-bottom-width",
            "border-left-width",
        ],
    ),
    (
        "border-style",
        [
            "border-top-style",
            "border-right-style",
            "border-bottom-style",
            "border-left-style",
        ],
    ),
    (
        "border-color",
        [
            "border-top-color",
            "border-right-color",
            "border-bottom-color",
            "border-left-color",
        ],
    ),
    (
        "border-radius",
        [
            "border-top-left-radius",
            "border-top-right-radius",
            "border-bottom-right-radius",
            "border-bottom-left-radius",
        ],
    ),
];

crate::ast_check! { on ["block"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let decls: Vec<_> = node
        .children(&mut c)
        .filter(|n| n.kind() == "declaration")
        .collect();
    let mut entries: Vec<(String, tree_sitter::Node)> = Vec::new();
    for decl in &decls {
        let mut dc = decl.walk();
        if let Some(prop) = decl.children(&mut dc).find(|n| n.kind() == "property_name") {
            let name = prop.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
            entries.push((name, *decl));
        }
    }
    for (shorthand, longhands) in GROUPS {
        let mut all_present = true;
        let mut first_idx: Option<usize> = None;
        for lh in longhands {
            let pos = entries.iter().position(|(n, _)| n == lh);
            match pos {
                Some(i) => {
                    if first_idx.map_or(true, |fi| i < fi) {
                        first_idx = Some(i);
                    }
                }
                None => {
                    all_present = false;
                    break;
                }
            }
        }
        if !all_present { continue; }
        let Some(i) = first_idx else { continue; };
        let target = entries[i].1;
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &target,
            super::META.id,
            format!("All longhands present; use the `{shorthand}` shorthand."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_css(s, &Check)
    }

    #[test]
    fn flags_all_margin_longhands() {
        let css = ".a { margin-top: 0; margin-right: 0; margin-bottom: 0; margin-left: 0; }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_partial_longhands() {
        let css = ".a { margin-top: 0; margin-bottom: 0; }";
        assert!(run(css).is_empty());
    }
}
