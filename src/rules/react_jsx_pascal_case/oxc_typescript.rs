//! react-jsx-pascal-case oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXElementName, JSXMemberExpression, JSXMemberExpressionObject};
use oxc_span::GetSpan;
use std::sync::Arc;

/// `unstable_`/`experimental_` is an established React convention for
/// experimental public APIs (e.g. `React.unstable_Activity`,
/// `<Checkbox.unstable_BubbleInput />`). The component name after the prefix is
/// still PascalCase, so strip a single leading experimental prefix before the
/// check rather than rejecting the segment for containing `_`.
fn strip_experimental_prefix(segment: &str) -> &str {
    for prefix in ["unstable_", "experimental_"] {
        if let Some(rest) = segment.strip_prefix(prefix) {
            return rest;
        }
    }
    segment
}

fn is_pascal_case(name: &str) -> bool {
    for raw_segment in name.split('.') {
        let segment = strip_experimental_prefix(raw_segment);
        if segment.is_empty() {
            return false;
        }
        let first = segment.chars().next().unwrap();
        if !first.is_ascii_uppercase() {
            return false;
        }
        if segment.contains('_') || segment.contains('-') {
            return false;
        }
    }
    true
}

/// Component names sometimes carry an underscore-delimited namespace/visibility
/// marker after a PascalCase base — the shadcn-ui re-export convention
/// (`Input_Shadcn_`, `SelectItem_Shadcn_`) disambiguates a wrapped component
/// from an identically-named local one. The base is PascalCase; the underscore
/// suffix is the discriminator. Accept the whole name when the segment before
/// the first `_` is genuine mixed-case PascalCase. Requiring a lowercase letter
/// in the base keeps `SCREAMING_SNAKE_CASE` names (`MY_COMPONENT`) flagged while
/// admitting real component bases (`Input`, `SelectItem`).
fn has_pascal_case_base_with_underscore_suffix(name: &str) -> bool {
    let Some((base, _suffix)) = name.split_once('_') else {
        return false;
    };
    is_pascal_case(base) && base.chars().any(|c| c.is_ascii_lowercase())
}

/// `SCREAMING_PREFIX_PascalCase` is an established React-ecosystem namespace
/// convention: an ALL_CAPS prefix groups a library's internal components under a
/// recognizable marker (Mantine React Table's `MRT_TableFooterRow`, `UI_Button`,
/// `DS_Card`). Upstream `react/jsx-pascal-case` covers it via `allowAllCaps`.
/// Accept a single all-caps prefix (`[A-Z][A-Z0-9]+`, length >= 2) followed by
/// `_` and a PascalCase tail. The tail must be genuine mixed-case PascalCase, so
/// a lowercase tail (`MRT_table`) or a bare prefix (`MRT_`) stays flagged, and
/// pure `SCREAMING_SNAKE_CASE` (`MY_COMPONENT`) is not admitted.
fn is_screaming_prefix_pascal_case(name: &str) -> bool {
    let Some((prefix, tail)) = name.split_once('_') else {
        return false;
    };
    let is_screaming_prefix = prefix.len() >= 2
        && prefix.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && prefix.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    is_screaming_prefix && is_pascal_case(tail) && tail.chars().any(|c| c.is_ascii_lowercase())
}

fn is_intrinsic(name: &str) -> bool {
    let first = name.chars().next().unwrap_or('a');
    first.is_ascii_lowercase()
}

/// `Provider`/`Consumer`/`displayName` are the canonical accessors on a
/// React/SolidJS context object (`<counterContext.Provider>`,
/// `<authContext.Consumer>`). The object a context is created into is
/// conventionally camelCase (`counterContext`, `authContext`, `themeContext`),
/// not a component, so a camelCase root paired with one of these properties is
/// the context-provider idiom rather than a naming violation. Non-context
/// properties on a camelCase root (`table.Foo`) stay flagged.
fn is_context_accessor(prop: &str) -> bool {
    matches!(prop, "Provider" | "Consumer" | "displayName")
}

/// The leftmost identifier of a JSX member expression (`Table` in `Table.td`,
/// `Foo` in `Foo.Bar.Baz`). A `this` root (`<this.Orange />`) has no identifier
/// name and yields `None`.
fn member_root_name<'a>(member: &'a JSXMemberExpression<'a>) -> Option<&'a str> {
    match &member.object {
        JSXMemberExpressionObject::IdentifierReference(id) => Some(id.name.as_str()),
        JSXMemberExpressionObject::MemberExpression(inner) => member_root_name(inner),
        JSXMemberExpressionObject::ThisExpression(_) => None,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        // For a member expression (`Table.td`, `Form.Item`), the PascalCase
        // requirement applies only to the leftmost/root identifier — the
        // property accessor is the compound-component sub-name and is exempt. A
        // lowercase root (`table.Foo`) is a real violation, not an intrinsic
        // host element, so the member-expression root skips the intrinsic
        // exemption. Plain identifiers and namespaced names keep the full check.
        let tag = match &opening.name {
            JSXElementName::Identifier(id) => {
                let tag = id.name.as_str();
                if is_intrinsic(tag) {
                    return;
                }
                tag.to_string()
            }
            JSXElementName::IdentifierReference(id) => {
                let tag = id.name.as_str();
                if is_intrinsic(tag) {
                    return;
                }
                tag.to_string()
            }
            JSXElementName::MemberExpression(member) => {
                // `Namespace.htmlElement` (e.g. `Primitive.div`, `Table.td`) is a
                // valid compound-component / styled-components namespace and is
                // never flagged on its sub-name.
                if is_intrinsic(member.property.name.as_str()) {
                    return;
                }
                // `<counterContext.Provider>` / `<authContext.Consumer>` — a
                // context accessor sits on a camelCase context object, not a
                // component, so the camelCase root is the idiom, not a violation.
                if is_context_accessor(member.property.name.as_str()) {
                    return;
                }
                // A `this`-rooted member (`<this.Orange />`) has no component
                // identifier to validate.
                let Some(root) = member_root_name(member) else {
                    return;
                };
                root.to_string()
            }
            JSXElementName::NamespacedName(ns) => {
                let tag = format!("{}:{}", ns.namespace.name, ns.name.name);
                if is_intrinsic(&tag) {
                    return;
                }
                tag
            }
            _ => return,
        };

        if !is_pascal_case(&tag)
            && !has_pascal_case_base_with_underscore_suffix(&tag)
            && !is_screaming_prefix_pascal_case(&tag)
        {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.name.span().start as usize);

            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Component `{tag}` is not PascalCase — rename to PascalCase."
                ),
                severity: Severity::Warning,
                span: None,
            });
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    // Issue #1986: `Namespace.htmlElement` member-expression components (Radix-UI /
    // styled-components) whose final segment is a lowercase intrinsic HTML element
    // are valid and must not be flagged.
    #[test]
    fn allows_namespaced_intrinsic_div() {
        assert!(run("const x = <Primitive.div />;").is_empty());
    }

    #[test]
    fn allows_namespaced_intrinsic_button() {
        assert!(run("const x = <Primitive.button />;").is_empty());
    }

    #[test]
    fn allows_namespaced_intrinsic_span() {
        assert!(run("const x = <Primitive.span />;").is_empty());
    }

    #[test]
    fn allows_namespaced_pascal() {
        // `Foo.Bar` (PascalCase final segment) was already valid — must not regress.
        assert!(run("const x = <Foo.Bar />;").is_empty());
    }

    #[test]
    fn flags_non_pascal_case_component() {
        // A genuinely bad component name (not a member expression) still fires.
        assert_eq!(run("const x = <MY_COMPONENT />;").len(), 1);
    }

    #[test]
    fn allows_plain_intrinsic() {
        assert!(run("const x = <div>hello</div>;").is_empty());
    }

    // Issue #1987: `unstable_`/`experimental_` is an established React convention
    // for experimental public APIs (e.g. `<Checkbox.unstable_BubbleInput />`). The
    // underlying component name stays PascalCase, so these must not be flagged.
    #[test]
    fn allows_unstable_member_bubble_input() {
        assert!(run("const x = <Checkbox.unstable_BubbleInput />;").is_empty());
    }

    #[test]
    fn allows_unstable_member_provider() {
        assert!(run("const x = <Checkbox.unstable_Provider />;").is_empty());
    }

    #[test]
    fn allows_unstable_member_trigger() {
        assert!(run("const x = <Checkbox.unstable_Trigger />;").is_empty());
    }

    #[test]
    fn is_pascal_case_accepts_experimental_prefixes() {
        // Direct coverage of the prefix-stripping in `is_pascal_case`, independent
        // of the dispatch guards: the experimental prefix is stripped, then the
        // remainder is checked for PascalCase.
        assert!(is_pascal_case("Checkbox.unstable_BubbleInput"));
        assert!(is_pascal_case("Checkbox.experimental_Trigger"));
        // A non-experimental underscore in an uppercase segment still fails.
        assert!(!is_pascal_case("Foo.Bad_Name"));
        // The prefix must be a literal `unstable_`/`experimental_`; an uppercase
        // look-alike is not the convention and still fails.
        assert!(!is_pascal_case("Unstable_Thing"));
    }

    // Issue #1370: compound components accessed via dot notation use lowercase
    // sub-names that mirror native HTML elements (`Table.td`, `Table.th`,
    // `Table.tr`). Only the root (`Table`) must be PascalCase; the sub-name is
    // exempt.
    #[test]
    fn allows_compound_component_td() {
        assert!(run("const x = <Table.td>x</Table.td>;").is_empty());
    }

    #[test]
    fn allows_compound_component_th() {
        assert!(run("const x = <Table.th>x</Table.th>;").is_empty());
    }

    #[test]
    fn allows_compound_component_tr() {
        assert!(run("const x = <Table.tr>x</Table.tr>;").is_empty());
    }

    #[test]
    fn allows_compound_component_pascal_subname() {
        // `Form.Item` — non-intrinsic PascalCase sub-name, PascalCase root.
        assert!(run("const x = <Form.Item>x</Form.Item>;").is_empty());
    }

    // Positive space: a member expression whose ROOT is not PascalCase still
    // fires on the root, even when the sub-name is fine.
    #[test]
    fn flags_lowercase_root_member_expression() {
        assert_eq!(run("const x = <table.Foo>x</table.Foo>;").len(), 1);
    }

    #[test]
    fn flags_camel_root_member_expression() {
        assert_eq!(run("const x = <myObj.Bar>x</myObj.Bar>;").len(), 1);
    }

    // Issue #3209: `<counterContext.Provider>` is the canonical context-provider
    // idiom in React and SolidJS — the camelCase root is a context object, not a
    // component, so `.Provider`/`.Consumer`/`.displayName` accessors on a
    // camelCase root must not be flagged. A non-context property on a camelCase
    // root (`table.Foo`) stays flagged (covered above).
    #[test]
    fn allows_camel_root_context_provider() {
        assert!(
            run("const x = <counterContext.Provider value={x}>{c}</counterContext.Provider>;")
                .is_empty()
        );
    }

    #[test]
    fn allows_camel_root_context_consumer() {
        assert!(run("const x = <authContext.Consumer>{c}</authContext.Consumer>;").is_empty());
    }

    #[test]
    fn allows_pascal_root_context_provider() {
        // A PascalCase root with a context accessor was already valid — must not
        // regress now that the accessor is explicitly exempt.
        assert!(run("const x = <Tabs.Provider>x</Tabs.Provider>;").is_empty());
    }

    #[test]
    fn flags_camel_root_non_context_property() {
        // `table.Foo` — camelCase root, property `Foo` is not a context accessor,
        // so this is a real naming violation and must still fire. This proves the
        // context exemption is narrow (property-scoped), not a blanket camelCase
        // root pass.
        assert_eq!(run("const x = <table.Foo />;").len(), 1);
    }

    // Issue #1352: the shadcn-ui re-export convention appends an
    // underscore-delimited namespace marker (`_Shadcn_`) to a PascalCase base to
    // disambiguate a wrapped component from an identically-named local one. The
    // base is PascalCase, so these must not be flagged.
    #[test]
    fn allows_underscore_suffix_self_closing() {
        assert!(run("const x = <Input_Shadcn_ className=\"y\" />;").is_empty());
    }

    #[test]
    fn allows_underscore_suffix_with_children() {
        assert!(run("const x = <SelectItem_Shadcn_>x</SelectItem_Shadcn_>;").is_empty());
    }

    // Negative space: a name whose base (before the first `_`) is not valid
    // PascalCase is not a namespace-marker convention and still fires. A leading
    // underscore leaves an empty base (`_DataTable`), so the relaxation does not
    // apply. (`is_intrinsic` already exempts lowercase-first names like
    // `foo_Bar`/`myComponent` as host elements before this point — the helper's
    // lowercase-base rejection is covered directly in the unit test below.)
    #[test]
    fn flags_leading_underscore_empty_base() {
        assert_eq!(run("const x = <_DataTable />;").len(), 1);
    }

    // Issue #2182: `SCREAMING_PREFIX_PascalCase` is a React-ecosystem namespace
    // convention (Mantine React Table's `MRT_` prefix). An ALL_CAPS prefix +
    // `_` + PascalCase tail must not be flagged.
    #[test]
    fn allows_screaming_prefix_mrt_table_footer_row() {
        assert!(run("const x = <MRT_TableFooterRow />;").is_empty());
    }

    #[test]
    fn allows_screaming_prefix_ui_button() {
        assert!(run("const x = <UI_Button />;").is_empty());
    }

    #[test]
    fn allows_screaming_prefix_ds_card() {
        assert!(run("const x = <DS_Card>x</DS_Card>;").is_empty());
    }

    // Negative space for the SCREAMING_PREFIX relaxation: a bare prefix with no
    // PascalCase tail and a lowercase tail are not the convention and still fire.
    #[test]
    fn flags_bare_screaming_prefix_no_tail() {
        assert_eq!(run("const x = <MRT_ />;").len(), 1);
    }

    #[test]
    fn flags_screaming_prefix_lowercase_tail() {
        assert_eq!(run("const x = <MRT_table />;").len(), 1);
    }

    // `myComponent` (camel) is exempted as an intrinsic before the check; a
    // member-root camel/kebab still fires (covered above). An all-lowercase
    // snake name (`my_component`) is treated as intrinsic (lowercase-first) and
    // is not this rule's concern — kebab/camel/lowercase-snake are covered by the
    // intrinsic exemption and existing tests. Pure SCREAMING_SNAKE_CASE stays
    // flagged.
    #[test]
    fn flags_pure_screaming_snake_case() {
        assert_eq!(run("const x = <MY_COMPONENT />;").len(), 1);
    }

    #[test]
    fn is_screaming_prefix_pascal_case_decisions() {
        // ALL_CAPS prefix + PascalCase tail → accepted.
        assert!(is_screaming_prefix_pascal_case("MRT_TableFooterRow"));
        assert!(is_screaming_prefix_pascal_case("UI_Button"));
        assert!(is_screaming_prefix_pascal_case("DS_Card"));
        // Digits in the prefix are allowed (still all-caps namespace).
        assert!(is_screaming_prefix_pascal_case("V2_Widget"));
        // Bare prefix (empty tail) → rejected.
        assert!(!is_screaming_prefix_pascal_case("MRT_"));
        // Lowercase tail → rejected.
        assert!(!is_screaming_prefix_pascal_case("MRT_table"));
        // Pure SCREAMING_SNAKE_CASE (uppercase tail) → rejected.
        assert!(!is_screaming_prefix_pascal_case("MY_COMPONENT"));
        // Single-char prefix is too short to be a namespace marker → rejected.
        assert!(!is_screaming_prefix_pascal_case("A_Button"));
        // Mixed-case prefix is not an ALL_CAPS namespace → rejected (handled by
        // the underscore-suffix convention instead).
        assert!(!is_screaming_prefix_pascal_case("Input_Shadcn_"));
        // No underscore → not this convention.
        assert!(!is_screaming_prefix_pascal_case("MyComponent"));
    }

    #[test]
    fn has_pascal_case_base_with_underscore_suffix_decisions() {
        // PascalCase base + underscore marker → accepted.
        assert!(has_pascal_case_base_with_underscore_suffix("Input_Shadcn_"));
        assert!(has_pascal_case_base_with_underscore_suffix("SelectItem_Shadcn_"));
        assert!(has_pascal_case_base_with_underscore_suffix("Alert_Shadcn_"));
        // Lowercase base → rejected.
        assert!(!has_pascal_case_base_with_underscore_suffix("foo_Bar"));
        // SCREAMING_SNAKE_CASE base (no lowercase) → rejected.
        assert!(!has_pascal_case_base_with_underscore_suffix("MY_COMPONENT"));
        // Leading underscore (empty base) → rejected.
        assert!(!has_pascal_case_base_with_underscore_suffix("_DataTable"));
        // No underscore → not this convention (handled by `is_pascal_case`).
        assert!(!has_pascal_case_base_with_underscore_suffix("Input"));
    }
}
