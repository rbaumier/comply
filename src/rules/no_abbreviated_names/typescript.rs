//! no-abbreviated-names backend — reject common abbreviations in identifiers.
//!
//! Why: `acct` / `usr` / `btn` / `cfg` saves 2 keystrokes at declaration
//! and costs every future reader a moment of decoding. Modern editors
//! auto-complete full words — there's no tradeoff, tech debt.
//!
//! Detection: walk every `identifier` / `property_identifier` node, split
//! into camelCase/snake_case words, and flag any word that matches the
//! banned abbreviation list exactly.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

// Keep this list short and focused on GENUINELY obscure abbreviations
// that no reader recognizes without guessing. Common ecosystem idioms
// (cfg, ctx, idx, err, fmt, ret, val, num, str, obj, arr, req, res,
// msg, auth, db, dict) are NOT on the list: every working programmer
// reads them at sight and expanding them to `config`/`context`/`index`
// only adds typing overhead. The rule targets the 2-keystroke-savings
// names that look like leetcode solution variables.
// better-result API: Result.err(value), result.isErr()
const ALLOWED_METHOD_NAMES: &[&str] = &["err", "isErr"];

const DEFAULT_BANNED: &[(&str, &str)] = &[
    ("acct", "account"),
    ("usr", "user"),
    ("btn", "button"),
    ("pwd", "password"),
    ("cnt", "count"),
    ("desc", "description"),
    ("addr", "address"),
    ("org", "organization"),
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["identifier", "property_identifier"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Ok(name) = node.utf8_text(source_bytes) else {
            return;
        };
        if node.kind() == "property_identifier" && ALLOWED_METHOD_NAMES.contains(&name) {
            return;
        }
        let allowed = ctx.config.string_list("no-abbreviated-names", "allowed", ctx.lang);
        let extra = ctx.config.string_list("no-abbreviated-names", "banned", ctx.lang);
        let merged = build_banned_list(&extra);
        let Some((abbr, full)) = matches_banned(name, &merged) else {
            return;
        };
        if allowed.iter().any(|a| a == &abbr) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-abbreviated-names".into(),
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
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
        // ctx, idx, cfg, err, fmt, ret, val, num, str, obj, arr, req,
        // res, msg, auth, db, dict are all idiomatic short names that
        // every working programmer reads at sight. The rule deliberately
        // doesn't flag them.
        assert!(run_on("function f(ctx: any) {}").is_empty());
        assert!(run_on("function f(idx: number) {}").is_empty());
        assert!(run_on("const cfg = {};").is_empty());
        assert!(run_on("function f(err: Error) {}").is_empty());
        assert!(run_on("function f(req: Request, res: Response) {}").is_empty());
    }

    #[test]
    fn does_not_flag_word_containing_abbreviation_letters() {
        // 'account' contains 'acct' letters but isn't the abbreviation.
        assert!(run_on("const accountant = 1;").is_empty());
    }
}
