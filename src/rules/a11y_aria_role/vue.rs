//! a11y-aria-role — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, is_vue_file};

const VALID_ROLES: &[&str] = &[
    "alert", "alertdialog", "application", "article", "banner", "button",
    "cell", "checkbox", "columnheader", "combobox", "complementary",
    "contentinfo", "definition", "dialog", "directory", "document", "feed",
    "figure", "form", "grid", "gridcell", "group", "heading", "img", "link",
    "list", "listbox", "listitem", "log", "main", "marquee", "math", "menu",
    "menubar", "menuitem", "menuitemcheckbox", "menuitemradio", "navigation",
    "none", "note", "option", "presentation", "progressbar", "radio",
    "radiogroup", "region", "row", "rowgroup", "rowheader", "scrollbar",
    "search", "searchbox", "separator", "slider", "spinbutton", "status",
    "switch", "tab", "table", "tablist", "tabpanel", "term", "textbox",
    "timer", "toolbar", "tooltip", "tree", "treegrid", "treeitem",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if let Some(role) = attr_value(elem.attrs, "role")
                && !VALID_ROLES.contains(&role)
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-aria-role".into(),
                    message: format!("Invalid ARIA role `{role}`. Use a valid WAI-ARIA role."),
                    severity: Severity::Error,
                    span: None,
                });
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
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_vue_template() {
        let source = "<template>\n  <div role=\"banana\"></div>\n</template>";
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("banana"));
    }

    #[test]
    fn allows_valid_role() {
        let source = "<template>\n  <div role=\"button\"></div>\n</template>";
        assert!(run(source).is_empty());
    }
}
