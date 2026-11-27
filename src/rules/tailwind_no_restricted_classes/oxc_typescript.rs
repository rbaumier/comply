//! tailwind-no-restricted-classes oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::tailwind_class_strings::{Class, ClassString, class_strings};
use regex::Regex;
use serde::Deserialize;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Substrings: `class` covers `classnames`, `tw` covers `twMerge`/`twJoin`.
        Some(&["class", "cn", "cva", "clsx", "tw", "clx"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let blocklist = ctx.config.string_list(super::META.id, "classes", ctx.lang);
        let tables = ctx.config.table_list(super::META.id, "entries", ctx.lang);
        if blocklist.is_empty() && tables.is_empty() {
            return Vec::new();
        }
        let entries: Vec<Entry> = tables.into_iter().enumerate().map(compile_entry).collect();
        let mut diagnostics = Vec::new();
        for string in class_strings(semantic) {
            let mut report = |message: String| {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, string.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message,
                    severity: Severity::Error,
                    span: None,
                });
            };
            for class in &string.classes {
                if let Some(blocked) = blocklist.iter().find(|b| b.as_str() == class.utility) {
                    report(format!(
                        "Class `{blocked}` is on the project blocklist — use the \
                         design-system equivalent or remove."
                    ));
                }
            }
            for entry in entries.iter().filter(|entry| entry.applies_to(&string)) {
                let hits = match &entry.matcher {
                    Matcher::Pattern(pattern) => string
                        .classes
                        .iter()
                        .filter(|class| pattern.is_match(class.utility) && entry.counts(class))
                        .count(),
                    Matcher::AllOf(patterns) => usize::from(patterns.iter().all(|pattern| {
                        string
                            .classes
                            .iter()
                            .any(|class| pattern.is_match(class.utility) && entry.counts(class))
                    })),
                };
                for _ in 0..hits {
                    report(entry.message.clone());
                }
            }
        }
        diagnostics
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryToml {
    pattern: Option<String>,
    all_of: Option<Vec<String>>,
    #[serde(default)]
    allow: Vec<String>,
    allow_regex: Option<String>,
    #[serde(default)]
    variant: Vec<String>,
    #[serde(default)]
    unless_variant: Vec<String>,
    context: Option<Context>,
    requires: Option<String>,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Context {
    IterationCallback,
    Conditional,
}

enum Matcher {
    Pattern(Regex),
    AllOf(Vec<Regex>),
}

struct Entry {
    matcher: Matcher,
    allow: Vec<String>,
    allow_regex: Option<Regex>,
    variant: Vec<String>,
    unless_variant: Vec<String>,
    context: Option<Context>,
    requires: Option<Regex>,
    message: String,
}

impl Entry {
    /// Class-level conditions: allow-lists and variants.
    fn counts(&self, class: &Class) -> bool {
        let under = |variants: &[String]| {
            class
                .variants
                .iter()
                .any(|v| variants.iter().any(|wanted| v.starts_with(wanted.as_str())))
        };
        let allowed = self.allow.iter().any(|a| a == class.utility)
            || self
                .allow_regex
                .as_ref()
                .is_some_and(|re| re.is_match(class.utility));
        let variant_ok = self.variant.is_empty() || under(&self.variant);
        !allowed && variant_ok && !under(&self.unless_variant)
    }

    /// String-level conditions: context and `requires`, read with variants.
    fn applies_to(&self, string: &ClassString) -> bool {
        let in_context = match self.context {
            None => true,
            Some(Context::IterationCallback) => string.in_iteration_callback,
            Some(Context::Conditional) => string.branch.is_some(),
        };
        in_context
            && !self
                .requires
                .as_ref()
                .is_some_and(|re| string.classes.iter().any(|class| re.is_match(class.raw)))
    }
}

fn compile_entry((index, table): (usize, &toml::Table)) -> Entry {
    let at = format!("[rules.\"{}\"] entries[{index}]", super::META.id);
    let entry: EntryToml = toml::Value::Table(table.clone())
        .try_into()
        .unwrap_or_else(|err| panic!("{at}: {err}"));
    let regex = |key: &str, source: &str| {
        Regex::new(source).unwrap_or_else(|err| panic!("{at}.{key}: invalid regex: {err}"))
    };
    let matcher = match (entry.pattern, entry.all_of) {
        (Some(pattern), None) => Matcher::Pattern(regex("pattern", &pattern)),
        (None, Some(all_of)) => Matcher::AllOf(
            all_of
                .iter()
                .map(|source| regex("all_of", source))
                .collect(),
        ),
        _ => panic!("{at}: set exactly one of `pattern` or `all_of`"),
    };
    Entry {
        matcher,
        allow: entry.allow,
        allow_regex: entry
            .allow_regex
            .map(|source| regex("allow_regex", &source)),
        variant: entry.variant,
        unless_variant: entry.unless_variant,
        context: entry.context,
        requires: entry.requires.map(|source| regex("requires", &source)),
        message: entry.message,
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
    use crate::config::Config;
    use crate::files::Language;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    /// Run under the default config — no `classes` configured.
    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    fn config_from(toml: &str) -> Config {
        let project_dir = TempDir::new().expect("tempdir");
        fs::write(
            project_dir.path().join("comply.toml"),
            format!("[rules.tailwind-no-restricted-classes]\n{toml}\n"),
        )
        .expect("write cfg");
        Config::load_from(project_dir.path()).expect("load cfg")
    }

    /// Lint `src` as `t.tsx` through the engine, prefilter included.
    fn run_with_config(src: &str, toml: &str) -> Vec<Diagnostic> {
        let config = config_from(toml);
        crate::engine::lint_in_memory(Path::new("t.tsx"), Language::Tsx, src, &config, None)
            .into_iter()
            .filter(|d| d.rule_id.as_ref() == super::super::META.id)
            .collect()
    }

    #[test]
    fn no_op_without_configured_blocklist() {
        // bg-white / text-white / text-black are standard Tailwind utilities;
        // under the default (empty) blocklist the rule must stay silent.
        assert!(run(r#"const x = <div className="bg-white dark:bg-zinc-900" />;"#).is_empty());
        assert!(run(r#"const x = <div className="text-white" />;"#).is_empty());
        assert!(run(r#"const x = <div className="text-black mt-4" />;"#).is_empty());
        assert!(run(r#"const x = <div className="space-x-px" />;"#).is_empty());
    }

    #[test]
    fn flags_configured_class() {
        let src = r#"const x = <div className="text-black mt-4" />;"#;
        assert_eq!(run_with_config(src, r#"classes = ["text-black"]"#).len(), 1);
    }

    #[test]
    fn flags_configured_class_through_variant_prefix() {
        let src = r#"const x = <div className="hover:bg-white" />;"#;
        assert_eq!(run_with_config(src, r#"classes = ["bg-white"]"#).len(), 1);
    }

    #[test]
    fn ignores_unconfigured_class_when_blocklist_set() {
        let src = r#"const x = <div className="text-red-500 mt-4" />;"#;
        assert!(run_with_config(src, r#"classes = ["text-black"]"#).is_empty());
    }

    #[test]
    fn classes_flag_every_class_string() {
        let sources = [
            r#"cn("bg-x");"#,
            r#"cva("bg-x");"#,
            r#"export const a = clsx("bg-x");"#,
            r#"const x = <div className={`bg-x ${y}`} />;"#,
            r#"const c = "bg-x"; cn(c);"#,
        ];
        for src in sources {
            assert_eq!(
                run_with_config(src, r#"classes = ["bg-x"]"#).len(),
                1,
                "{src}"
            );
        }
    }

    /// Called directly: the engine catches a rule's panic per file.
    #[test]
    #[should_panic(expected = "entries[1].pattern")]
    fn invalid_regex_names_entry() {
        let toml = r#"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^ok$"
message = "a"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "("
message = "b"
"#;
        let config = config_from(toml);
        let path = Path::new("t.tsx");
        let src = r#"cn("x");"#;
        let ctx = CheckCtx {
            path,
            path_arc: Arc::from(path),
            source: src,
            config: &config,
            project: crate::project::default_static_project_ctx(),
            file: crate::rules::file_ctx::default_static_file_ctx(),
            lang: Language::Tsx,
        };
        crate::oxc_helpers::with_oxc_parse(src, path, |semantic| {
            Check.run_on_semantic(semantic, &ctx)
        });
    }
    #[test]
    fn pattern_flags_off_scale_spacing() {
        let toml = r#"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^(p|mt|px)-(\\[.+\\]|\\d+\\.(25|75))$"
message = "off scale"
"#;
        let src = r#"cn("p-[13px] mt-1.25 px-4 !p-[13px] [&:hover]:mt-1.25");"#;
        assert_eq!(run_with_config(src, toml).len(), 4);
        assert!(run_with_config(r#"cn("px-4");"#, toml).is_empty());
    }

    #[test]
    fn allow_exempts_listed_utility() {
        let src = r#"cn("shadow-floating shadow-lg");"#;
        let allow = r#"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^shadow"
allow = ["shadow-floating"]
message = "m"
"#;
        let allow_regex = r#"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^shadow"
allow_regex = "^shadow-floating$"
message = "m"
"#;
        assert_eq!(run_with_config(src, allow).len(), 1);
        assert_eq!(run_with_config(src, allow_regex).len(), 1);
    }

    #[test]
    fn unless_variant_exempts_by_prefix() {
        let toml = r#"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^ring-"
unless_variant = ["focus"]
message = "m"
"#;
        let src = r#"cn("focus-visible:ring-primary ring-primary");"#;
        assert_eq!(run_with_config(src, toml).len(), 1);
    }

    #[test]
    fn hover_in_map_callback_only() {
        let toml = r#"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^bg-muted$"
variant = ["hover"]
context = "iteration-callback"
message = "m"
"#;
        let in_map = r#"xs.map((x) => cn("hover:bg-muted"));"#;
        let in_flat_map = r#"xs.flatMap((x) => cn("hover:bg-muted"));"#;
        let outside = r#"cn("hover:bg-muted");"#;
        let no_variant = r#"xs.map((x) => cn("bg-muted"));"#;
        assert_eq!(run_with_config(in_map, toml).len(), 1);
        assert_eq!(run_with_config(in_flat_map, toml).len(), 1);
        assert!(run_with_config(outside, toml).is_empty());
        assert!(run_with_config(no_variant, toml).is_empty());
    }

    #[test]
    fn all_of_and_requires() {
        let all_of = r#"
[[rules.tailwind-no-restricted-classes.entries]]
all_of = ["^text-caption$", "^h-6$", "^rounded-full$"]
message = "m"
"#;
        assert_eq!(
            run_with_config(r#"cn("text-caption h-6 rounded-full");"#, all_of).len(),
            1
        );
        assert!(run_with_config(r#"cn("text-caption h-6");"#, all_of).is_empty());

        let requires = r#"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^text-display$"
requires = "^tabular-nums$"
message = "m"
"#;
        assert_eq!(run_with_config(r#"cn("text-display");"#, requires).len(), 1);
        assert!(run_with_config(r#"cn("text-display tabular-nums");"#, requires).is_empty());
    }

    #[test]
    fn motion_entry_combines_conditions() {
        let toml = r#"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^animate-"
unless_variant = ["motion-safe", "action-playing"]
requires = "^motion-reduce:"
message = "m"
"#;
        assert_eq!(run_with_config(r#"cn("animate-spin");"#, toml).len(), 1);
        assert!(run_with_config(r#"cn("motion-safe:animate-spin");"#, toml).is_empty());
        assert!(
            run_with_config(r#"cn("animate-spin motion-reduce:animate-none");"#, toml).is_empty()
        );
    }

    #[test]
    fn finding_carries_entry_message() {
        let toml = r#"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^bg-a$"
message = "first message"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^bg-b$"
message = "second message"
"#;
        let messages: Vec<String> = run_with_config(r#"cn("bg-a bg-b");"#, toml)
            .into_iter()
            .map(|d| d.message)
            .collect();
        assert_eq!(messages, ["first message", "second message"]);
    }

    #[test]
    fn string_outside_class_context_is_ignored() {
        let toml = r#"classes = ["bg-x"]"#;
        assert!(run_with_config(r#"const l = "bg-x"; log(l);"#, toml).is_empty());
        assert!(run_with_config(r#"t("bg-x");"#, toml).is_empty());
        let parameter = r#"const items = [{ label: "bg-x", render: (c) => <i className={c} /> }];"#;
        assert!(run_with_config(parameter, toml).is_empty());
        let destructured = r#"const { a } = { a: "bg-y", b: "bg-x" }; cn(a);"#;
        assert!(run_with_config(destructured, toml).is_empty());
    }

    #[test]
    fn conditional_context_flags_branch_only() {
        let toml = r#"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^font-(semibold|bold)$"
context = "conditional"
message = "m"
"#;
        let src = r#"cn(active && "font-bold");
            const a = <i className={ok ? "font-semibold" : ""} />;"#;
        assert_eq!(run_with_config(src, toml).len(), 2);
        assert!(run_with_config(r#"cn("font-bold");"#, toml).is_empty());
    }

    #[test]
    fn natalia_scroll_and_weight_entries() {
        let toml = r#"
[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^overflow(-y)?-(auto|scroll)$"
requires = "^overscroll(-y)?-(contain|none)$"
message = "scroll"

[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^font-(semibold|bold)$"
variant = ["hover", "focus", "active", "aria-", "data-", "group-", "peer-", "has-"]
message = "weight Use WeightReservedText."

[[rules.tailwind-no-restricted-classes.entries]]
pattern = "^font-(semibold|bold)$"
context = "conditional"
message = "weight"
"#;
        let flagged = [
            r#"const a = <i className="overflow-y-auto" />;"#,
            r#"const a = <i className="hover:font-semibold" />;"#,
            r#"cn(active && "font-bold");"#,
        ];
        for src in flagged {
            assert_eq!(run_with_config(src, toml).len(), 1, "{src}");
            assert!(run_with_config(src, "").is_empty(), "{src}");
        }
        let hover = run_with_config(r#"const a = <i className="hover:font-semibold" />;"#, toml);
        assert!(hover[0].message.ends_with("Use WeightReservedText."));
        assert_eq!(
            run_with_config(r#"cn(active && "hover:font-bold");"#, toml).len(),
            2
        );
        let clean = r#"const a = <i className="overflow-y-auto overscroll-contain font-semibold" />;
            cn("md:font-bold");"#;
        assert!(run_with_config(clean, toml).is_empty());
    }
}
