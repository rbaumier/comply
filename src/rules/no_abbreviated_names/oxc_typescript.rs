//! no-abbreviated-names OxcCheck backend — reject common abbreviations
//! in identifiers.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use super::abbreviations::{build_banned_list, matches_banned};

// better-result API: Result.err(value), result.isErr()
const ALLOWED_METHOD_NAMES: &[&str] = &["err", "isErr"];

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
            severity: Severity::Error,
            span: None,
        });
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
    fn allows_org_domain_term() {
        // Regression for issue #977: `org` is the canonical GitHub-API /
        // multi-tenant SaaS term (`orgId`, `/orgs/{org}`).
        assert!(run_on("const org = 1;").is_empty());
        assert!(run_on("const orgId = 1;").is_empty());
    }

    #[test]
    fn allows_desc_term() {
        // Regression for issue #1017: `desc` is the canonical abbreviation
        // for a descriptor (VirtIO/USB/PCIe) and the SQL `ORDER BY … DESC`
        // keyword — suggesting 'description' is frequently wrong.
        assert!(run_on("const desc = getDescriptor();").is_empty());
        assert!(run_on("const descSize = 1;").is_empty());
    }

    #[test]
    fn allows_pwd_print_working_directory_term() {
        // Regression for issue #1484: `pwd` means "print working directory"
        // in shell/filesystem code and "password" in URL/auth code — two
        // canonical expansions, so `pwd` is exempt entirely (like `desc`).
        assert!(run_on("const filePwd = getCwd();").is_empty());
        assert!(run_on("const pwd = process.cwd();").is_empty());
    }

    #[test]
    fn still_flags_other_abbreviations_after_pwd_removal() {
        // Removing `pwd` must not weaken detection of genuine abbreviations.
        let diags = run_on("const usrId = 1;");
        assert!(diags.iter().any(|d| d.message.contains("usr")));
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
