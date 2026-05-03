use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn extract_jsdoc_body(line: &str) -> String {
    let s = line.trim_start_matches("/**").trim_end_matches("*/").trim();
    s.to_lowercase()
}

fn get_next_symbol_name(lines: &[&str], from: usize) -> Option<String> {
    for line in lines.iter().take(lines.len().min(from + 3)).skip(from) {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("function ") {
            return extract_ident(rest);
        }
        for kw in &[
            "const ",
            "let ",
            "var ",
            "export const ",
            "export let ",
            "export function ",
        ] {
            if let Some(rest) = t.strip_prefix(kw) {
                return extract_ident(rest);
            }
        }
        if let Some(rest) = t.strip_prefix("class ") {
            return extract_ident(rest);
        }
        if let Some(rest) = t.strip_prefix("export class ") {
            return extract_ident(rest);
        }
        if let Some(paren) = t.find('(') {
            let candidate = t[..paren].trim();
            if !candidate.is_empty() && candidate.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Some(candidate.to_lowercase());
            }
        }
    }
    None
}

fn extract_ident(s: &str) -> Option<String> {
    let ident: String = s
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if ident.is_empty() {
        None
    } else {
        Some(ident.to_lowercase())
    }
}

fn is_trivial_description(body: &str, name: &str) -> bool {
    if body.is_empty() || name.is_empty() {
        return false;
    }
    let normalized = body
        .replace("the ", "")
        .replace("a ", "")
        .replace("an ", "")
        .replace("this ", "")
        .replace("function", "")
        .replace("method", "")
        .replace("class", "")
        .replace("variable", "")
        .replace(".", "")
        .trim()
        .to_lowercase();
    normalized == *name || normalized.is_empty()
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
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
            let raw = &ctx.source[comment.span.start as usize..comment.span.end as usize];
            let trimmed = raw.trim();
            // Only single-line JSDoc: /** ... */ on one line.
            if !(trimmed.starts_with("/**") && trimmed.ends_with("*/")) {
                continue;
            }
            if trimmed.contains('\n') {
                continue;
            }

            let body = extract_jsdoc_body(trimmed);
            let (start_line, _) = byte_offset_to_line_col(ctx.source, comment.span.start as usize);

            let lines: Vec<&str> = ctx.source.lines().collect();
            let Some(name) = get_next_symbol_name(&lines, start_line) else { continue };

            if !is_trivial_description(&body, &name) {
                continue;
            }

            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: start_line,
                column: 1,
                rule_id: super::META.id.into(),
                message: "JSDoc description merely repeats the symbol name without adding useful information.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}
