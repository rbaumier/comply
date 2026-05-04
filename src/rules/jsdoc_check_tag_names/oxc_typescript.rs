//! jsdoc/check-tag-names OxcCheck backend — scan comments for unknown tags.

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
        "desc" => Some("description"),
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
