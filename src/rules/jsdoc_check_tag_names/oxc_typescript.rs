//! jsdoc/check-tag-names OxcCheck backend — scan comments for unknown tags.
//! Tags containing an uppercase letter (custom convention tags like `@publicApi`,
//! decorator references like `@Module`) are not flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::scan_blocks;
use std::sync::Arc;

pub struct Check;

const KNOWN_TAGS: &[&str] = &[
    "abstract",
    "access",
    "alias",
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
    // `@desc` is the documented JSDoc alias for `@description`.
    "desc",
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
    // TypeScript 5.5 JSDoc tag for type-only imports in `.js` files.
    "import",
    "inheritdoc",
    "inheritDoc",
    "inner",
    "instance",
    "interface",
    "internal",
    // JSX compiler pragmas recognized by TypeScript and Babel, not JSDoc tags.
    "jsx",
    "jsxFrag",
    "jsxImportSource",
    "jsxRuntime",
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
    // TypeScript JSDoc tag for documenting function overloads in `.js` files.
    "overload",
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

fn is_known(name: &str) -> bool {
    KNOWN_TAGS.iter().any(|k| k.eq_ignore_ascii_case(name))
}

fn suggest(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "return" => Some("returns"),
        "arg" | "argument" | "parameter" => Some("param"),
        "exemple" => Some("example"),
        "thrown" | "throw" => Some("throws"),
        "yield" => Some("yields"),
        "emit" | "fire" => Some("emits"),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let text = &ctx.source[start..end];
            if !text.starts_with("/**") {
                continue;
            }
            let (line_offset, _) = byte_offset_to_line_col(ctx.source, start);

            for block in scan_blocks(text) {
                for tag in block.tags() {
                    if is_known(&tag.name) {
                        continue;
                    }
                    // Standard JSDoc tags are all lowercase, so a typo of one is
                    // too. A tag containing an uppercase letter is an intentional
                    // custom convention tag (camelCase `@publicApi`, `@usageNotes`)
                    // or a decorator reference in an example (`@Module`), never a
                    // misspelling — leave it alone.
                    if tag.name.chars().any(|c| c.is_ascii_uppercase()) {
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
                        path: Arc::clone(&ctx.path_arc),
                        line: tag.line + line_offset - 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message,
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn allows_custom_convention_tags_issue_1016() {
        // NestJS @publicApi / @usageNotes — camelCase custom tags.
        assert!(run("/**\n * @publicApi\n */\n").is_empty());
        assert!(run("/**\n * @usageNotes\n * notes\n */\n").is_empty());
    }

    #[test]
    fn allows_decorator_reference_in_example_issue_1016() {
        // A decorator reference inside a JSDoc example is PascalCase.
        let src = "/**\n * @example\n * @Module({\n *   imports: [],\n * })\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_jsx_compiler_pragmas_issue_1406() {
        // JSX compiler pragmas recognized by TypeScript/Babel, not JSDoc tags.
        assert!(run("/** @jsx jsx */\n").is_empty());
        assert!(run("/** @jsxRuntime classic */\n").is_empty());
        assert!(run("/** @jsxImportSource @emotion/react */\n").is_empty());
        assert!(run("/** @jsxFrag jsx.Fragment */\n").is_empty());
    }

    #[test]
    fn allows_typescript_import_and_overload_tags_issue_1414() {
        // TypeScript 5.5 JSDoc tags for type-only imports and function overloads.
        assert!(run("/** @import { AST } from 'svelte/compiler' */\n").is_empty());
        let src = "/**\n * @template Output\n * @overload\n * @param {() => Output} fn\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_desc_alias_issue_1425() {
        // `@desc` is the documented JSDoc alias for `@description`.
        let src = "/**\n * @desc The gutter between columns.\n * @type {number}\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_lowercase_typos() {
        // Genuine misspellings of standard tags are all lowercase.
        assert_eq!(run("/**\n * @return thing\n */\n").len(), 1);
        assert_eq!(run("/**\n * @arg x\n */\n").len(), 1);
        assert_eq!(run("/**\n * @bogus foo\n */\n").len(), 1);
    }
}
