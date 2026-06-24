//! a11y-aria-role — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, is_vue_file};

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

/// Collect the role strings to validate for one element's attribute list.
///
/// A static `role="dialog"` yields the literal value verbatim. A dynamic
/// binding (`:role="expr"` / `v-bind:role="expr"`) carries a JS expression, not
/// a role name, so its source text must never be validated as-is. Instead the
/// string-literals reachable inside the expression are extracted and each is
/// returned for validation: a bare literal, every string-literal branch of a
/// ternary, and every string-literal operand of `||` / `??` chains. `undefined`
/// / `null` branches (which remove the attribute) and any sub-expression that
/// is not a string literal (identifier, call, member access) are skipped, so
/// they never produce a diagnostic.
fn role_candidates(attrs: &str) -> Vec<String> {
    if is_static_role(attrs)
        && let Some(value) = attr_value(attrs, "role")
    {
        return vec![value.to_string()];
    }
    if let Some(expr) = attr_value(attrs, ":role").or_else(|| attr_value(attrs, "v-bind:role")) {
        return extract_string_literals(expr);
    }
    Vec::new()
}

/// True when the element carries a static `role="..."` rather than a dynamic
/// `:role` / `v-bind:role` binding. Guards against `attr_value`'s substring
/// match treating the expression of a dynamic binding as a literal role.
fn is_static_role(attrs: &str) -> bool {
    let Some(pos) = attrs.find("role=") else {
        return false;
    };
    // The char immediately before `role=` decides the form: the `:` of `:role`
    // or `v-bind:role` marks a dynamic binding; anything else (whitespace, or
    // the start of the string) is the static attribute.
    !attrs[..pos].ends_with(':')
}

/// Extract every string literal reachable in a bound `:role` expression as a
/// role candidate. Returns one entry per non-empty string literal found in a
/// bare literal, a ternary's branches, or `||` / `??` operand chains.
/// Non-literal operands, `undefined` / `null`, and the empty string `''` (all
/// of which leave no role on the element) are skipped — no candidate emitted.
fn extract_string_literals(expr: &str) -> Vec<String> {
    split_top_level(expr)
        .into_iter()
        .filter_map(|part| as_string_literal(part.trim()))
        .filter(|role| !role.is_empty())
        .collect()
}

/// Split an expression into the operand slices that may carry a role literal:
/// the branches of a ternary and the operands of `||` / `??` chains, honouring
/// quotes so separators inside string literals don't split. Non-literal
/// operands are filtered out later by `as_string_literal`.
fn split_top_level(expr: &str) -> Vec<&str> {
    let bytes = expr.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut in_string: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = in_string {
            if b == q {
                in_string = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' | b'\'' | b'`' => in_string = Some(b),
            b'?' if bytes.get(i + 1) == Some(&b'?') => {
                parts.push(&expr[start..i]);
                i += 2;
                start = i;
                continue;
            }
            b'|' if bytes.get(i + 1) == Some(&b'|') => {
                parts.push(&expr[start..i]);
                i += 2;
                start = i;
                continue;
            }
            b'?' | b':' => {
                parts.push(&expr[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(&expr[start..]);
    parts
}

/// Return the inner value of a bare single- or double-quoted string literal, or
/// `None` for anything that is not exactly one quoted literal (identifiers,
/// calls, member access, template literals, `undefined`, `null`).
fn as_string_literal(part: &str) -> Option<String> {
    let bytes = part.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let quote = bytes[0];
    if (quote != b'"' && quote != b'\'') || bytes[bytes.len() - 1] != quote {
        return None;
    }
    let inner = &part[1..part.len() - 1];
    // A second matching quote inside means this isn't a single literal.
    if inner.contains(quote as char) {
        return None;
    }
    Some(inner.to_string())
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            for role in role_candidates(elem.attrs) {
                if !VALID_ROLES.contains(&role.as_str()) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: elem.line,
                        column: 1,
                        rule_id: "a11y-aria-role".into(),
                        message: format!(
                            "Invalid ARIA role `{role}`. Use a valid WAI-ARIA role."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
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

    #[test]
    fn skips_dynamic_ternary_of_valid_roles() {
        // #4874: a dynamic binding carries an expression, not a role name. Both
        // ternary branches are valid roles, so nothing is flagged.
        let source = "<template>\n  \
            <dialog :role=\"alert ? 'alertdialog' : 'dialog'\"></dialog>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn skips_dynamic_ternary_with_undefined_branch() {
        // #4874: `undefined` removes the attribute; the literal branch is valid.
        let source =
            "<template>\n  <div :role=\"selectable ? 'combobox' : undefined\"></div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn skips_dynamic_ternary_two_valid_roles() {
        let source =
            "<template>\n  <div :role=\"selectable ? 'listbox' : 'menu'\"></div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_invalid_literal_in_dynamic_ternary() {
        // An extracted string literal that is a genuinely invalid role is flagged.
        let source =
            "<template>\n  <div :role=\"x ? 'badrole' : 'dialog'\"></div>\n</template>";
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("badrole"));
    }

    #[test]
    fn skips_dynamic_non_literal_expression() {
        // An identifier/member expression can't be statically validated → skip.
        let source = "<template>\n  <div :role=\"someRole\"></div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn validates_bare_dynamic_string_literal() {
        let source = "<template>\n  <div :role=\"'banana'\"></div>\n</template>";
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("banana"));
    }

    #[test]
    fn skips_dynamic_ternary_with_empty_string_branch() {
        // An empty-string branch leaves no role, like `undefined`; don't flag it.
        let source =
            "<template>\n  <div :role=\"editable ? 'textbox' : ''\"></div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_static_invalid_role_with_v_bind_form() {
        // v-bind:role full form is also treated as dynamic.
        let source =
            "<template>\n  <div v-bind:role=\"sel ? 'listbox' : 'menu'\"></div>\n</template>";
        assert!(run(source).is_empty());
    }
}
