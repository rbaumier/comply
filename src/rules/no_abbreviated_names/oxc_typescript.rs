//! no-abbreviated-names OxcCheck backend — reject common abbreviations
//! in identifiers.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

// better-result API: Result.err(value), result.isErr()
const ALLOWED_METHOD_NAMES: &[&str] = &["err", "isErr"];

// `addr` is intentionally NOT on the list — `std::net::SocketAddr`,
// `peer_addr()`, `local_addr()`, and `bind_addr` are standard Rust API.
const DEFAULT_BANNED: &[(&str, &str)] = &[
    ("acct", "account"),
    ("usr", "user"),
    ("btn", "button"),
    ("pwd", "password"),
    ("cnt", "count"),
    ("desc", "description"),
    ("org", "organization"),
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BindingIdentifier, AstType::StaticMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (name, offset) = match node.kind() {
            oxc_ast::AstKind::BindingIdentifier(id) => (id.name.as_str(), id.span.start),
            oxc_ast::AstKind::StaticMemberExpression(expr) => {
                let prop = expr.property.name.as_str();
                if ALLOWED_METHOD_NAMES.contains(&prop) {
                    return;
                }
                (prop, expr.property.span.start)
            }
            _ => return,
        };

        let allowed = ctx
            .config
            .string_list("no-abbreviated-names", "allowed", ctx.lang);
        let extra = ctx
            .config
            .string_list("no-abbreviated-names", "banned", ctx.lang);
        let merged = build_banned_list(&extra);
        let Some((abbr, full)) = matches_banned(name, &merged) else {
            return;
        };
        if allowed.iter().any(|a| a == &abbr) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Identifier '{name}' contains abbreviation '{abbr}' — \
                 use the full word '{full}'. Editors auto-complete; \
                 readers don't."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn build_banned_list(extra: &[String]) -> Vec<(String, String)> {
    let mut list: Vec<(String, String)> = DEFAULT_BANNED
        .iter()
        .map(|(a, f)| ((*a).to_owned(), (*f).to_owned()))
        .collect();
    for entry in extra {
        if let Some((abbr, full)) = entry.split_once(':') {
            let abbr = abbr.trim().to_lowercase();
            let full = full.trim().to_owned();
            if !list.iter().any(|(a, _)| *a == abbr) {
                list.push((abbr, full));
            }
        }
    }
    list
}

fn matches_banned(name: &str, banned: &[(String, String)]) -> Option<(String, String)> {
    for word in split_words(name) {
        let lower = word.to_ascii_lowercase();
        if let Some(pair) = banned.iter().find(|(abbr, _)| lower == *abbr) {
            return Some(pair.clone());
        }
    }
    None
}

/// Split a camelCase / snake_case identifier into its constituent words.
fn split_words(name: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let bytes = name.as_bytes();
    let mut start = 0;
    for i in 1..bytes.len() {
        let prev_is_lower = bytes[i - 1].is_ascii_lowercase();
        let curr_is_upper = bytes[i].is_ascii_uppercase();
        let curr_is_underscore = bytes[i] == b'_';
        if (prev_is_lower && curr_is_upper) || curr_is_underscore {
            words.push(&name[start..i]);
            start = if curr_is_underscore { i + 1 } else { i };
        }
    }
    if start < bytes.len() {
        words.push(&name[start..]);
    }
    words
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_camelcase_abbreviation() {
        let diags = run_on("function f(usrId: number) {}");
        assert!(diags.iter().any(|d| d.message.contains("usr")));
    }

    #[test]
    fn flags_snake_case_abbreviation() {
        let diags = run_on("const user_acct = 1;");
        assert!(diags.iter().any(|d| d.message.contains("acct")));
    }

    #[test]
    fn flags_full_abbreviation_as_name() {
        let diags = run_on("const btn = {};");
        assert!(diags.iter().any(|d| d.message.contains("btn")));
    }

    #[test]
    fn allows_full_words() {
        assert!(run_on("const userAccount = 1;").is_empty());
        assert!(run_on("const requestContext = 1;").is_empty());
    }

    #[test]
    fn allows_ecosystem_idioms() {
        assert!(run_on("function f(ctx: any) {}").is_empty());
        assert!(run_on("function f(idx: number) {}").is_empty());
        assert!(run_on("const cfg = {};").is_empty());
        assert!(run_on("function f(err: Error) {}").is_empty());
        assert!(run_on("function f(req: Request, res: Response) {}").is_empty());
        // `addr` is standard in networking/socket code.
        assert!(run_on("function f(addr: SocketAddr) {}").is_empty());
        assert!(run_on("const toAddr = destination.parse();").is_empty());
    }

    #[test]
    fn does_not_flag_word_containing_abbreviation_letters() {
        assert!(run_on("const accountant = 1;").is_empty());
    }

    #[test]
    fn no_fp_on_call_site_of_abbreviated_function() {
        // insertBtn is declared elsewhere; calling it should not fire.
        let diags = run_on("insertBtn(db);");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn no_fp_on_identifier_reference_passed_as_argument() {
        let diags = run_on("doSomething(usrHelper);");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn still_flags_declaration_of_abbreviated_function() {
        let diags = run_on("function insertBtn(db: unknown) {}");
        assert!(diags.iter().any(|d| d.message.contains("btn")));
    }
}
