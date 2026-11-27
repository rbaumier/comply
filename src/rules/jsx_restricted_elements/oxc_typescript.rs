//! jsx-restricted-elements oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::tailwind_class_strings::element_class_strings;
use oxc_ast::AstKind;
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeValue, JSXChild, JSXElement, JSXElementName, JSXOpeningElement,
};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::GetSpan;
use regex::Regex;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(&self, semantic: &'a Semantic<'a>, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let tables = ctx.config.table_list(super::META.id, "entries", ctx.lang);
        if tables.is_empty() {
            return Vec::new();
        }
        let entries: Vec<Entry> = tables.into_iter().enumerate().map(compile_entry).collect();
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::JSXElement(element) = node.kind() else {
                continue;
            };
            for entry in entries
                .iter()
                .filter(|entry| entry.matches(element, semantic))
            {
                let Some(default_message) =
                    entry.violation(element, node.id(), semantic, ctx.source)
                else {
                    continue;
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, element.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: entry.message.clone().unwrap_or(default_message),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryToml {
    tag: Option<String>,
    element: Option<String>,
    #[serde(default)]
    when_attr: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    unless_attr: Vec<String>,
    when_class: Option<String>,
    #[serde(default)]
    unless_class: Vec<String>,
    when_text: Option<String>,
    #[serde(rename = "use")]
    replacement: Option<String>,
    forbid_attr: Option<String>,
    require_attr: Option<String>,
    forbid_class: Option<String>,
    #[serde(default)]
    forbid_nested: bool,
    message: Option<String>,
}

/// A compiled entry. No other action set means `forbid_nested`.
struct Entry {
    tag: Option<Regex>,
    element: Option<String>,
    when_attr: BTreeMap<String, Vec<String>>,
    unless_attr: Vec<String>,
    when_class: Option<Regex>,
    unless_class: Vec<String>,
    when_text: Option<Regex>,
    replacement: Option<String>,
    forbid_attr: Option<String>,
    require_attr: Option<String>,
    forbid_class: Option<Regex>,
    message: Option<String>,
}

impl Entry {
    /// Selector plus every `when_*` / `unless_*` condition.
    fn matches<'a>(&self, element: &'a JSXElement<'a>, semantic: &'a Semantic<'a>) -> bool {
        let opening = &element.opening_element;
        let selected = match &opening.name {
            JSXElementName::Identifier(name) => {
                self.tag.as_ref().is_some_and(|re| re.is_match(&name.name))
            }
            JSXElementName::IdentifierReference(name) => {
                self.element.as_deref() == Some(name.name.as_str())
            }
            _ => false,
        };
        if !selected {
            return false;
        }
        let attrs_wanted = self.when_attr.iter().all(|(name, values)| {
            literal_attr(opening, name).is_some_and(|value| values.iter().any(|v| v == value))
        });
        let spread = has_spread(opening);
        let attr_exempt = self
            .unless_attr
            .iter()
            .any(|attr| spread || has_attr(opening, attr));
        if !attrs_wanted || attr_exempt {
            return false;
        }
        let strings = element_class_strings(opening, semantic);
        if let Some(pattern) = &self.when_class
            && !strings
                .iter()
                .filter(|string| string.branch.is_none())
                .flat_map(|string| &string.classes)
                .any(|class| pattern.is_match(class.utility))
        {
            return false;
        }
        if self.unless_class.iter().any(|wanted| {
            spread
                || strings
                    .iter()
                    .flat_map(|string| &string.classes)
                    .any(|class| class.utility == wanted)
        }) {
            return false;
        }
        self.when_text.as_ref().is_none_or(|pattern| {
            let text: String = element
                .children
                .iter()
                .filter_map(|child| match child {
                    JSXChild::Text(text) => Some(text.value.as_str()),
                    _ => None,
                })
                .collect();
            pattern.is_match(text.trim())
        })
    }

    /// The action's default message when the matched element breaks it.
    fn violation<'a>(
        &self,
        element: &'a JSXElement<'a>,
        id: NodeId,
        semantic: &'a Semantic<'a>,
        source: &str,
    ) -> Option<String> {
        let opening = &element.opening_element;
        let name = opening.name.span().source_text(source);
        if let Some(replacement) = &self.replacement {
            return Some(format!("Use `<{replacement}>` instead of `<{name}>`."));
        }
        if let Some(attr) = &self.forbid_attr {
            return has_attr(opening, attr)
                .then(|| format!("`{attr}` is not allowed on `<{name}>`."));
        }
        if let Some(attr) = &self.require_attr {
            return (!has_attr(opening, attr) && !has_spread(opening))
                .then(|| format!("`<{name}>` requires `{attr}`."));
        }
        if let Some(pattern) = &self.forbid_class {
            return element_class_strings(opening, semantic)
                .iter()
                .flat_map(|string| &string.classes)
                .find(|class| pattern.is_match(class.utility))
                .map(|class| format!("Class `{}` is not allowed on `<{name}>`.", class.raw));
        }
        semantic
            .nodes()
            .ancestor_kinds(id)
            .any(|kind| matches!(kind, AstKind::JSXElement(outer) if self.matches(outer, semantic)))
            .then(|| format!("`<{name}>` must not be nested in another matching element."))
    }
}

fn has_spread(opening: &JSXOpeningElement) -> bool {
    opening
        .attributes
        .iter()
        .any(|item| item.as_spread().is_some())
}

fn has_attr(opening: &JSXOpeningElement, name: &str) -> bool {
    opening
        .attributes
        .iter()
        .filter_map(JSXAttributeItem::as_attribute)
        .any(|attr| attr.is_identifier(name))
}

/// A string-literal attribute value; dynamic values never match.
fn literal_attr<'a>(opening: &'a JSXOpeningElement<'a>, name: &str) -> Option<&'a str> {
    let attr = opening
        .attributes
        .iter()
        .filter_map(JSXAttributeItem::as_attribute)
        .find(|attr| attr.is_identifier(name))?;
    match &attr.value {
        Some(JSXAttributeValue::StringLiteral(literal)) => Some(literal.value.as_str()),
        _ => None,
    }
}

fn compile_entry((index, table): (usize, &toml::Table)) -> Entry {
    let at = format!("[rules.\"{}\"] entries[{index}]", super::META.id);
    let entry: EntryToml = toml::Value::Table(table.clone())
        .try_into()
        .unwrap_or_else(|err| panic!("{at}: {err}"));
    if entry.tag.is_some() == entry.element.is_some() {
        panic!("{at}: set exactly one of `tag` or `element`");
    }
    let actions = [
        entry.replacement.is_some(),
        entry.forbid_attr.is_some(),
        entry.require_attr.is_some(),
        entry.forbid_class.is_some(),
        entry.forbid_nested,
    ];
    if actions.iter().filter(|set| **set).count() != 1 {
        panic!(
            "{at}: set exactly one of `use`, `forbid_attr`, `require_attr`, `forbid_class` or `forbid_nested = true`"
        );
    }
    let regex = |key: &str, source: Option<String>| {
        source.map(|source| {
            Regex::new(&source).unwrap_or_else(|err| panic!("{at}.{key}: invalid regex: {err}"))
        })
    };
    Entry {
        tag: regex("tag", entry.tag),
        element: entry.element,
        when_attr: entry.when_attr,
        unless_attr: entry.unless_attr,
        when_class: regex("when_class", entry.when_class),
        unless_class: entry.unless_class,
        when_text: regex("when_text", entry.when_text),
        replacement: entry.replacement,
        forbid_attr: entry.forbid_attr,
        require_attr: entry.require_attr,
        forbid_class: regex("forbid_class", entry.forbid_class),
        message: entry.message,
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::diagnostic::Diagnostic;
    use crate::files::Language;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn config_from(toml: &str) -> Config {
        let project_dir = TempDir::new().expect("tempdir");
        fs::write(project_dir.path().join("comply.toml"), toml).expect("write cfg");
        Config::load_from(project_dir.path()).expect("load cfg")
    }

    /// Lint `src` as `t.tsx` through the engine under `entries`.
    fn run_with_config(src: &str, entries: &str) -> Vec<Diagnostic> {
        let config = config_from(entries);
        crate::engine::lint_in_memory(Path::new("t.tsx"), Language::Tsx, src, &config, None)
            .into_iter()
            .filter(|d| d.rule_id.as_ref() == super::super::META.id)
            .collect()
    }

    const SELECT: &str = r#"
[[rules.jsx-restricted-elements.entries]]
tag = "select"
use = "Select"
"#;

    #[test]
    fn use_flags_intrinsic_element() {
        let findings = run_with_config("const x = <select />;", SELECT);
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0].message.contains("Select"),
            "{}",
            findings[0].message
        );
        assert!(run_with_config("const x = <Select />;", SELECT).is_empty());
    }

    const TEXT_INPUT: &str = r#"
[[rules.jsx-restricted-elements.entries]]
tag = "input"
when_attr = { type = ["text"] }
unless_attr = ["hidden"]
unless_class = ["sr-only"]
use = "Input"
"#;

    #[test]
    fn text_input_entry() {
        let count = |src: &str| run_with_config(src, TEXT_INPUT).len();
        assert_eq!(count(r#"const x = <input type="text" />;"#), 1);
        assert_eq!(count(r#"const x = <input type="text" hidden />;"#), 0);
        assert_eq!(
            count(r#"const x = <input type="text" className="sr-only" />;"#),
            0
        );
        assert_eq!(count(r#"const x = <input type="email" />;"#), 0);
        assert_eq!(count("const x = <input type={kind} />;"), 0);
        assert_eq!(count("const x = <input {...p} />;"), 0);
    }

    const REQUIRE_ATTR: &str = r#"
[[rules.jsx-restricted-elements.entries]]
element = "Button"
when_attr = { type = ["submit"] }
require_attr = "isLoading"
"#;

    #[test]
    fn require_attr_trusts_spread() {
        let count = |src: &str| run_with_config(src, REQUIRE_ATTR).len();
        assert_eq!(count(r#"const x = <Button type="submit" />;"#), 1);
        assert_eq!(count(r#"const x = <Button type="submit" isLoading />;"#), 0);
        assert_eq!(
            count(r#"const x = <Button type="submit" {...props} />;"#),
            0
        );
        assert_eq!(count(r#"const x = <Button type="button" />;"#), 0);
        assert_eq!(count(r#"const x = <Ui.Button type="submit" />;"#), 0);
    }

    const FORBID_CLASS: &str = r#"
[[rules.jsx-restricted-elements.entries]]
tag = "^h[1-6]$"
forbid_class = "^underline$"
"#;

    #[test]
    fn forbid_class_reads_class_strings() {
        let count = |src: &str| run_with_config(src, FORBID_CLASS).len();
        assert_eq!(count(r#"const x = <h2 className={cn("underline")} />;"#), 1);
        assert_eq!(
            count(r#"const x = <h2 className={cn(ok && "hover:underline")} />;"#),
            1
        );
        assert_eq!(
            count(r#"const c = "underline"; const x = <h2 className={c} />;"#),
            1
        );
        assert_eq!(
            count(r#"const x = <h2 className="underline-offset-2" />;"#),
            0
        );
        assert_eq!(count(r#"const x = <p className="underline" />;"#), 0);
    }

    const FORBID_NESTED: &str = r#"
[[rules.jsx-restricted-elements.entries]]
tag = "div"
when_class = "^rounded-card$"
forbid_nested = true
"#;

    #[test]
    fn forbid_nested_flags_inner_card_only() {
        let count = |src: &str| run_with_config(src, FORBID_NESTED).len();
        assert_eq!(
            count(
                r#"const x = <div className="rounded-card"><div className="rounded-card" /></div>;"#
            ),
            1
        );
        assert_eq!(
            count(
                r#"const x = <div><div className="rounded-card" /><div className="rounded-card" /></div>;"#
            ),
            0
        );
        assert_eq!(
            count(
                r#"const x = <div className={ok ? "rounded-card" : ""}><div className="rounded-card" /></div>;"#
            ),
            0
        );
    }

    const WHEN_TEXT: &str = r#"
[[rules.jsx-restricted-elements.entries]]
element = "Button"
when_text = "^Copier\\b"
use = "CopyButton"
"#;

    #[test]
    fn when_text_reads_direct_text() {
        let count = |src: &str| run_with_config(src, WHEN_TEXT).len();
        assert_eq!(count("const x = <Button>Copier le lien</Button>;"), 1);
        assert_eq!(count("const x = <Button><span>Copier</span></Button>;"), 0);
        assert_eq!(count("const x = <Button>Partager</Button>;"), 0);
    }

    const FORBID_ATTR: &str = r#"
[[rules.jsx-restricted-elements.entries]]
tag = "button"
forbid_attr = "title"
message = "Use Tooltip."
"#;

    #[test]
    fn forbid_attr_carries_entry_message() {
        let findings = run_with_config(r#"const x = <button title="x" />;"#, FORBID_ATTR);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].message, "Use Tooltip.");
        assert!(run_with_config("const x = <button {...p} />;", FORBID_ATTR).is_empty());
    }

    #[test]
    fn no_entries_emits_nothing() {
        assert!(run_with_config("const x = <select />;", "").is_empty());
    }

    const TWO_ACTIONS: &str = r#"
[[rules.jsx-restricted-elements.entries]]
tag = "select"
use = "Select"

[[rules.jsx-restricted-elements.entries]]
tag = "button"
use = "Button"
forbid_attr = "title"
"#;

    /// Called directly: the engine catches a rule's panic per file.
    #[test]
    #[should_panic(expected = "entries[1]")]
    fn two_actions_name_entry() {
        let config = config_from(TWO_ACTIONS);
        let path = Path::new("t.tsx");
        let src = "const x = <select />;";
        let ctx = crate::rules::backend::CheckCtx {
            path,
            path_arc: std::sync::Arc::from(path),
            source: src,
            config: &config,
            project: crate::project::default_static_project_ctx(),
            file: crate::rules::file_ctx::default_static_file_ctx(),
            lang: Language::Tsx,
        };
        crate::oxc_helpers::with_oxc_parse(src, path, |semantic| {
            crate::rules::backend::OxcCheck::run_on_semantic(&super::Check, semantic, &ctx)
        });
    }
}
