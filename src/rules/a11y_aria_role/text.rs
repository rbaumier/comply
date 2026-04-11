use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const VALID_ROLES: &[&str] = &[
    "alert",
    "alertdialog",
    "application",
    "article",
    "banner",
    "button",
    "cell",
    "checkbox",
    "columnheader",
    "combobox",
    "complementary",
    "contentinfo",
    "definition",
    "dialog",
    "directory",
    "document",
    "feed",
    "figure",
    "form",
    "grid",
    "gridcell",
    "group",
    "heading",
    "img",
    "link",
    "list",
    "listbox",
    "listitem",
    "log",
    "main",
    "marquee",
    "math",
    "menu",
    "menubar",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "navigation",
    "none",
    "note",
    "option",
    "presentation",
    "progressbar",
    "radio",
    "radiogroup",
    "region",
    "row",
    "rowgroup",
    "rowheader",
    "scrollbar",
    "search",
    "searchbox",
    "separator",
    "slider",
    "spinbutton",
    "status",
    "switch",
    "tab",
    "table",
    "tablist",
    "tabpanel",
    "term",
    "textbox",
    "timer",
    "toolbar",
    "tooltip",
    "tree",
    "treegrid",
    "treeitem",
];

fn is_jsx_file(ctx: &CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "tsx" || ext == "jsx"
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx_file(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Match role="value"
            let mut start = 0;
            while let Some(pos) = line[start..].find("role=\"") {
                let abs = start + pos + 6; // skip past role="
                if let Some(end) = line[abs..].find('"') {
                    let role = &line[abs..abs + end];
                    if !VALID_ROLES.contains(&role) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: start + pos + 1,
                            rule_id: "a11y-aria-role".into(),
                            message: format!("Invalid ARIA role `{role}`. Use a valid WAI-ARIA role."),
                            severity: Severity::Error,
                        });
                    }
                }
                start = abs;
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.tsx"), source))
    }

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.ts"), source))
    }

    #[test]
    fn flags_invalid_role() {
        let d = run(r#"<div role="banana">"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("banana"));
    }

    #[test]
    fn allows_valid_roles() {
        assert!(run(r#"<div role="button">"#).is_empty());
        assert!(run(r#"<nav role="navigation">"#).is_empty());
    }

    #[test]
    fn ignores_non_jsx_files() {
        assert!(run_ts(r#"<div role="banana">"#).is_empty());
    }
}
