//! jsdoc/check-tag-names — flag unknown JSDoc tag names.
//!
//! The tag whitelist mirrors eslint-plugin-jsdoc's default set
//! (jsdoc + closure + typescript modes merged) so users don't need
//! to configure it. Unknown tags are almost always typos
//! (`@return` → `@returns`, `@arg` → `@param`, `@desc` →
//! `@description`) that the documentation tooling will silently
//! ignore — catching them at lint time prevents invisible rot.
//! `@return` (singular) is accepted as the documented alias of `@returns`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_helpers::scan_blocks;

/// Recognised tags — union of the default sets from eslint-plugin-jsdoc
/// (`jsdoc`, `closure`, `typescript`). Kept sorted for readability.
const KNOWN_TAGS: &[&str] = &[
    "abstract",
    "access",
    "alias",
    // JSDoc3/Closure visibility marker (`@api public`/`@api private`); used
    // pervasively across mature Node.js libraries (mongoose, express, koa).
    "api",
    "async",
    "augments",
    "author",
    "borrows",
    "callback",
    "category",
    "class",
    "classdesc",
    "const",
    "constant",
    "constructs",
    "copyright",
    "default",
    "defaultvalue",
    "deprecated",
    "description",
    "emits",
    "enum",
    "event",
    "example",
    "exception",
    "experimental",
    "exports",
    "extends",
    "external",
    "file",
    "fileoverview",
    "fires",
    "function",
    "func",
    "generator",
    "global",
    "hideconstructor",
    "host",
    "ignore",
    "implements",
    "inheritdoc",
    "inheritDoc",
    // JSDoc3-era inheritance tag, an alias of `@augments`/`@extends`.
    "inherits",
    "inner",
    "instance",
    "interface",
    "internal",
    "kind",
    "lends",
    "license",
    "link",
    "listens",
    "member",
    "memberof",
    "method",
    "mixes",
    "mixin",
    "module",
    "name",
    "namespace",
    "nosideeffects",
    "override",
    "overview",
    "package",
    "param",
    "preserve",
    "private",
    "prop",
    "property",
    "protected",
    "public",
    "readonly",
    "record",
    "requires",
    // `@return` is the documented JSDoc/TypeDoc singular alias of `@returns`.
    "return",
    "returns",
    "satisfies",
    "see",
    "since",
    "static",
    "summary",
    "template",
    "this",
    "throws",
    "todo",
    "tutorial",
    "type",
    "typedef",
    "variation",
    "version",
    "virtual",
    "yields",
];

crate::ast_check! { on ["comment"] prefilter = ["/**"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in scan_blocks(text) {
        for tag in block.tags() {
            if is_known(&tag.name) {
                continue;
            }
            let suggestion = suggest(&tag.name);
            let message = match suggestion {
                Some(s) => format!(
                    "Unknown JSDoc tag `@{}` — did you mean `@{}`?",
                    tag.name, s
                ),
                None => format!(
                    "Unknown JSDoc tag `@{}` — use a canonical tag name.",
                    tag.name
                ),
            };
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: tag.line + line_offset,
                column: 1,
                rule_id: super::META.id.into(),
                message,
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn is_known(name: &str) -> bool {
    // Case-insensitive match — JSDoc tags are conventionally
    // lowercase but `@inheritDoc` (camel) is also accepted.
    KNOWN_TAGS.iter().any(|k| k.eq_ignore_ascii_case(name))
}

/// Return a best-guess canonical tag name for a small set of
/// well-known typos. Returning `None` means no suggestion.
fn suggest(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "arg" | "argument" | "parameter" => Some("param"),
        "desc" => Some("description"),
        "exemple" => Some("example"),
        "thrown" | "throw" => Some("throws"),
        "yield" => Some("yields"),
        "emit" | "fire" => Some("emits"),
        _ => None,
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_unknown_tag() {
        let src = "/**\n * @bogus foo\n */\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("@bogus"));
    }

    #[test]
    fn suggests_canonical_for_common_typos() {
        let src = "/**\n * @arg thing\n */\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("@param"));
    }

    #[test]
    fn allows_return_alias_issue_2283() {
        // `@return` is the documented JSDoc singular alias of `@returns`.
        let src = "/**\n * @return all nodes of kind, or [] if none is found\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_api_visibility_marker_issue_2325() {
        // `@api` is the JSDoc3/Closure visibility marker (`@api public`).
        assert!(run("/**\n * @api public\n */\n").is_empty());
        // A genuine typo of the tag stays flagged.
        assert_eq!(run("/**\n * @nonsensetag foo\n */\n").len(), 1);
    }

    #[test]
    fn allows_known_tags() {
        let src = r#"
/**
 * Summary.
 * @param x
 * @returns y
 * @throws Error
 * @deprecated
 */
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn accepts_camel_inheritdoc() {
        let src = "/**\n * @inheritDoc\n */\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn accepts_inherits_alias_issue_2326() {
        // `@inherits` is the JSDoc3-era inheritance tag (alias of `@augments`).
        assert!(run("/**\n * @inherits SchemaTypeOptions\n */\n").is_empty());
        // A genuine typo of the tag stays flagged.
        assert_eq!(run("/**\n * @inhertis Foo\n */\n").len(), 1);
    }
}
