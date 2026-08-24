//! no-abbreviated-names backend for Rust.
//!
//! Only the identifier a declaration introduces is checked, never a mention of
//! a name declared elsewhere: a type imported from a crate you do not own is
//! not yours to rename.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

use super::abbreviations::{build_banned_list, matches_banned};

/// Parent node kinds that introduce a name, each paired with the field holding
/// it. The field is what separates a declaration from a mention: in
/// `fn handle(user: UsrProfile)` the parameter's `pattern` is the declaration
/// and its `type` is a mention of a name declared elsewhere.
const DECLARATION_NAME_FIELDS: &[(&str, &str)] = &[
    ("let_declaration", "pattern"),
    ("parameter", "pattern"),
    ("function_item", "name"),
    ("const_item", "name"),
    ("static_item", "name"),
    ("struct_item", "name"),
    ("enum_item", "name"),
    ("union_item", "name"),
    ("trait_item", "name"),
    ("type_item", "name"),
    ("field_declaration", "name"),
    ("enum_variant", "name"),
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["identifier", "type_identifier", "field_identifier"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_declared_name(node) {
            return;
        }
        let Ok(name) = node.utf8_text(ctx.source.as_bytes()) else {
            return;
        };
        let extra = ctx.config.string_list("no-abbreviated-names", "banned", ctx.lang);
        let Some((abbreviation, full)) = matches_banned(name, &build_banned_list(&extra)) else {
            return;
        };
        let allowed = ctx.config.string_list("no-abbreviated-names", "allowed", ctx.lang);
        if allowed.contains(&abbreviation) {
            return;
        }
        let position = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: position.row + 1,
            column: position.column + 1,
            rule_id: super::META.id.into(),
            message: format!(
                "Identifier '{name}' contains abbreviation '{abbreviation}' — \
                 use the full word '{full}'. Editors auto-complete; \
                 readers don't."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when `node` is the identifier a declaration introduces, rather than a
/// mention of a name declared elsewhere.
fn is_declared_name(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let Some((_, field)) =
        DECLARATION_NAME_FIELDS.iter().find(|(kind, _)| *kind == parent.kind())
    else {
        return false;
    };
    parent.child_by_field_name(field).map(|name| name.id()) == Some(node.id())
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
    fn allows_desc_descriptor_term() {
        // Regression for issue #1017: `desc` is the canonical abbreviation
        // for a descriptor in virtualization/device-driver code (VirtIO
        // `Descriptor` in Firecracker) and the SQL `ORDER BY … DESC`
        // keyword — suggesting 'description' is frequently wrong.
        assert!(run_on("fn f() { let desc_size = std::mem::size_of::<virtio::Descriptor>(); }")
            .is_empty());
        assert!(run_on("fn f() { let desc = queue.pop_first_descriptor().unwrap(); }").is_empty());
    }

    #[test]
    fn allows_pwd_print_working_directory_term() {
        // Regression for issue #1484: in shell/filesystem code `pwd` means
        // "print working directory" (the `pwd(1)` command, `$PWD`), not
        // "password" — and as a POSIX `struct passwd` binding (#977) it is
        // a user-database entry. With two canonical expansions, `pwd` is
        // exempt entirely (like `desc`).
        assert!(run_on("fn f() { let mut file_pwd = get(); }").is_empty());
        assert!(run_on("fn engine_state_with_pwd() {}").is_empty());
        assert!(run_on("fn pwd_points_to_symlink_to_directory() {}").is_empty());
        assert!(run_on("fn f() { let pwd: libc::passwd = entry; }").is_empty());
    }

    #[test]
    fn still_flags_other_abbreviations() {
        // Removing `pwd` must not weaken detection of genuine abbreviations.
        let diags = run_on("fn f() { let user_acct = 1; }");
        assert!(diags.iter().any(|d| d.message.contains("acct")));
        let diags = run_on("fn f() { let btn = 1; }");
        assert!(diags.iter().any(|d| d.message.contains("btn")));
    }

    #[test]
    fn flags_pascal_case_type_declarations() {
        for source in [
            "struct UsrProfile { id: u32 }",
            "enum UsrKind { One }",
            "trait UsrRepository {}",
            "type UsrId = u32;",
            "union UsrPayload { id: u32 }",
        ] {
            let diags = run_on(source);
            assert!(diags.iter().any(|d| d.message.contains("usr")), "missed: {source}");
        }
    }

    #[test]
    fn flags_struct_field_and_enum_variant() {
        let diags = run_on("struct Profile { usr_id: u32 }");
        assert!(diags.iter().any(|d| d.message.contains("usr")), "unexpected: {diags:?}");
        let diags = run_on("enum Event { BtnPressed }");
        assert!(diags.iter().any(|d| d.message.contains("btn")), "unexpected: {diags:?}");
    }

    #[test]
    fn flags_screaming_snake_constant() {
        let diags = run_on("const MAX_USR_COUNT: usize = 1;");
        assert!(diags.iter().any(|d| d.message.contains("usr")), "unexpected: {diags:?}");
    }

    #[test]
    fn no_fp_on_a_type_mentioned_rather_than_declared() {
        // A type declared elsewhere — an imported one especially — is not
        // ours to rename, so only its declaration site is flagged.
        for source in [
            "fn f(profile: UsrProfile) {}",
            "fn f() -> UsrProfile { todo!() }",
            "struct Profile { owner: UsrProfile }",
            "fn f() { let profile: UsrProfile = load(); }",
            "use vendor::UsrProfile;",
            "impl UsrProfile {}",
        ] {
            assert!(run_on(source).is_empty(), "unexpected firing on: {source}");
        }
    }

    #[test]
    fn does_not_flag_a_pascal_case_word_containing_abbreviation_letters() {
        // A segment matches whole, so a word that merely starts with the
        // letters of an abbreviation stays clean.
        assert!(run_on("struct Accountant { counter: usize }").is_empty());
        assert!(run_on("struct OrganDonor { story: String }").is_empty());
    }
}
