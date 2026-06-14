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
    // JSDoc3-era inheritance tag, an alias of `@augments`/`@extends`.
    "inherits",
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
    // TypeDoc/TSDoc tag for supplemental documentation beyond the description.
    "remarks",
    "requires",
    // `@return` is the documented JSDoc/TypeDoc singular alias of `@returns`.
    "return",
    "returns",
    "satisfies",
    // TypeDoc/TSDoc tag marking a class as not intended to be subclassed.
    "sealed",
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
                    // A `/` in the token is not valid JSDoc tag syntax: `@scope/pkg`
                    // is a scoped npm package reference in prose (`@ngrx/entity`,
                    // `@angular/core`), not a tag.
                    if tag.name.contains('/') {
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
    fn allows_typedoc_tags_issue_1735() {
        // `@remarks` is a standard TypeDoc/TSDoc tag (graphql-js src/type/schema.ts).
        let src = "/**\n * Description.\n * @remarks\n * This function is called when the schema is first created.\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
        // `@sealed` is the all-lowercase TypeDoc/TSDoc tag the issue also names;
        // `@typeParam`/`@defaultValue` carry an uppercase letter and are exempt already.
        assert!(run("/**\n * @sealed\n */\n").is_empty());
    }

    #[test]
    fn allows_return_alias_issue_2283() {
        // `@return` is the documented JSDoc singular alias of `@returns`
        // (Angular DevKit schematics, ngrx/platform use it throughout).
        let src = "/**\n * @return all nodes of kind, or [] if none is found\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_inherits_alias_issue_2326() {
        // `@inherits` is the JSDoc3-era inheritance tag (alias of `@augments`/
        // `@extends`); mongoose uses it 48 times to document the prototype chain.
        let src = "/**\n * The options defined on a SchemaNumber.\n * @inherits SchemaTypeOptions\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
        // A genuine typo of the tag stays flagged.
        assert_eq!(run("/**\n * @inhertis Foo\n */\n").len(), 1);
    }

    #[test]
    fn allows_api_visibility_marker_issue_2325() {
        // `@api` is the JSDoc3/Closure visibility marker (`@api public`/
        // `@api private`); mongoose uses it 1043 times to mark its public surface.
        let src = "/**\n * @api public\n */\nclass SchemaNumberOptions {}\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
        // The bare tag (no argument) is accepted too.
        assert!(run("/**\n * @api\n */\n").is_empty());
        // A genuine typo of the tag stays flagged.
        assert_eq!(run("/**\n * @nonsensetag foo\n */\n").len(), 1);
    }

    #[test]
    fn allows_scoped_package_references_in_prose_issue_2281() {
        // Scoped npm package names in JSDoc prose (`@ngrx/entity`, `@angular/core`)
        // are not JSDoc tags — a `/` after the first word is not valid tag syntax
        // (ngrx/platform documents reducers this way).
        let src = "/**\n * @ngrx/entity provides a predefined interface for handling\n * a structured dictionary of records.\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
        let src = "/**\n * meta-reducer. This returns all providers for an @angular/core\n * based application.\n */\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_lowercase_typos() {
        // Genuine misspellings of standard tags are all lowercase.
        assert_eq!(run("/**\n * @retrun thing\n */\n").len(), 1);
        assert_eq!(run("/**\n * @arg x\n */\n").len(), 1);
        assert_eq!(run("/**\n * @bogus foo\n */\n").len(), 1);
    }
}
