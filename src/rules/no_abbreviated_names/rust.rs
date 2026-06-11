//! no-abbreviated-names backend for Rust.
//!
//! Same dictionary as the TypeScript impl, applied to Rust identifiers.
//! Splits snake_case words (Rust convention) and checks each against a
//! banned abbreviation list.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

// Match the TypeScript list: only flag GENUINELY obscure abbreviations,
// not ecosystem idioms. `cfg`, `ctx`, `idx`, `err`, `fmt`, `ret`, `val`,
// `num`, `str`, `obj`, `arr`, `req`, `res`, `msg`, `auth`, `db`, `dict`
// are all part of the Rust vocabulary (cfg attributes, std::fmt, io
// context, iteration index, …) and flagging them only adds noise. The
// list targets abbreviations a reader genuinely has to guess about.
//
// `addr` is intentionally NOT on the list — `std::net::SocketAddr`,
// `peer_addr()`, `local_addr()`, and `bind_addr` are standard Rust API.
// `org` is likewise exempt: it is the canonical domain term of the GitHub
// API (`GET /orgs/{org}`, `org_member`), Kubernetes labels, and
// multi-tenant SaaS schemas (`org_id`) — not an abbreviation a reader
// has to guess about.
const DEFAULT_BANNED: &[(&str, &str)] = &[
    ("acct", "account"),
    ("usr", "user"),
    ("btn", "button"),
    ("pwd", "password"),
    ("cnt", "count"),
    ("desc", "description"),
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["identifier"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let allowed = ctx.config.string_list("no-abbreviated-names", "allowed", ctx.lang);
        let extra = ctx.config.string_list("no-abbreviated-names", "banned", ctx.lang);
        let merged = build_banned_list(&extra);
        let source_bytes = ctx.source.as_bytes();
        let Some(parent) = node.parent() else {
            return;
        };
        if !matches!(
            parent.kind(),
            "let_declaration" | "parameter" | "function_item" | "const_item" | "static_item"
        ) {
            return;
        }
        let Ok(name) = node.utf8_text(source_bytes) else {
            return;
        };
        let Some((abbr, full)) = matches_banned(name, &merged) else {
            return;
        };
        if allowed.iter().any(|a| a == &abbr) {
            return;
        }
        if abbr == "pwd" && binding_type_mentions_passwd(parent, source_bytes) {
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

/// `pwd` is the canonical POSIX name for a `struct passwd` binding
/// (`libc::passwd` holds uid/gid/home dir/shell — a user-database entry,
/// not a password). When the binding's type annotation mentions `passwd`
/// (e.g. `let mut pwd: MaybeUninit<libc::passwd>`), renaming to
/// `password` would be misleading, so the identifier is exempt.
fn binding_type_mentions_passwd(binding: tree_sitter::Node, source: &[u8]) -> bool {
    binding
        .child_by_field_name("type")
        .and_then(|type_node| type_node.utf8_text(source).ok())
        .is_some_and(|type_text| type_text.contains("passwd"))
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
    for word in name.split('_') {
        let lower = word.to_ascii_lowercase();
        if let Some(pair) = banned.iter().find(|(abbr, _)| lower == *abbr) {
            return Some(pair.clone());
        }
    }
    None
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_snake_case_abbreviation() {
        let diags = run_on("fn f() { let user_acct = 1; }");
        assert!(diags.iter().any(|d| d.message.contains("acct")));
    }

    #[test]
    fn flags_bare_abbreviation() {
        let diags = run_on("fn f() { let btn = 1; }");
        assert!(diags.iter().any(|d| d.message.contains("btn")));
    }

    #[test]
    fn allows_full_words() {
        assert!(run_on("fn f() { let user_account = 1; }").is_empty());
        assert!(run_on("fn f() { let request_context = 1; }").is_empty());
    }

    #[test]
    fn allows_rust_ecosystem_idioms() {
        // cfg, ctx, idx, err, fmt, ret, val, num, str, obj, arr, req,
        // res, msg, auth, db, dict — all part of the Rust vocabulary
        // and intentionally NOT flagged.
        assert!(run_on("fn f(ctx: &Context) {}").is_empty());
        assert!(run_on("fn f(idx: usize) {}").is_empty());
        assert!(run_on("fn f() { let cfg = 1; }").is_empty());
        assert!(run_on("fn f(err: Error) {}").is_empty());
        assert!(run_on("fn f() { let fmt = 1; }").is_empty());
        // `addr` is standard for SocketAddr in Rust networking code.
        assert!(run_on("fn f(addr: &SocketAddr) {}").is_empty());
        assert!(run_on("fn f() { let addr = socket.local_addr()?; }").is_empty());
    }

    #[test]
    fn flags_param_abbreviation() {
        let diags = run_on("fn f(usr_id: usize) {}");
        assert!(diags.iter().any(|d| d.message.contains("usr")));
    }

    #[test]
    fn does_not_flag_word_containing_abbreviation_letters() {
        // 'account' contains 'acct' letters but isn't the abbreviation.
        assert!(run_on("fn f() { let accountant = 1; }").is_empty());
    }

    #[test]
    fn allows_org_domain_term() {
        // Regression for issue #977: `org` is the canonical GitHub-API /
        // multi-tenant SaaS term (`org_id`, `/orgs/{org}`), not an
        // abbreviation a reader has to guess about.
        assert!(run_on("fn f() { let org = get(); }").is_empty());
        assert!(run_on("fn f() { let org_id = 1; }").is_empty());
        assert!(run_on("fn f(org: &Org) {}").is_empty());
    }

    #[test]
    fn allows_pwd_bound_to_posix_passwd_struct() {
        // Regression for issue #977: a `libc::passwd` binding is a POSIX
        // user-database entry, not a password — `pwd` is the canonical
        // name and `password` would be misleading.
        assert!(run_on(
            "fn f() { let mut pwd: std::mem::MaybeUninit<libc::passwd> = \
             std::mem::MaybeUninit::uninit(); }"
        )
        .is_empty());
        assert!(run_on("fn f() { let pwd: libc::passwd = entry; }").is_empty());
    }

    #[test]
    fn still_flags_pwd_without_passwd_type() {
        let diags = run_on("fn f() { let pwd = \"secret\"; }");
        assert!(diags.iter().any(|d| d.message.contains("pwd")));
        let diags = run_on("fn f() { let mut pwd = \"secret\"; }");
        assert!(diags.iter().any(|d| d.message.contains("pwd")));
    }
}
