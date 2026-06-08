//! jsdoc/require-tags — require `@param` / `@returns` when relevant.
//!
//! The upstream eslint rule is fully config-driven (which tags to require is
//! user input). Comply has no config surface for this, so we apply a sensible
//! default: a JSDoc block attached to an *exported* function must include
//! `@param` for each parameter and `@returns` when the signature reveals a
//! non-void return type. Non-exported helpers are skipped to avoid noise.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, following_code, has_tag, parse_tags};

fn exported_function_signature(code: &str) -> Option<String> {
    // Join first 4 lines to accommodate multi-line signatures.
    let sig: String = code.lines().take(4).collect::<Vec<_>>().join(" ");
    let t = sig.trim();
    if !(t.starts_with("export function ")
        || t.starts_with("export async function ")
        || t.starts_with("export default function ")
        || t.starts_with("export default async function ")
        || t.starts_with("export const ")
        || t.starts_with("export let "))
    {
        return None;
    }
    Some(sig)
}

fn signature_has_params(sig: &str) -> bool {
    let open = match sig.find('(') {
        Some(i) => i,
        None => return false,
    };
    let close = sig[open..].find(')').map(|i| open + i).unwrap_or(sig.len());
    let between = &sig[open + 1..close];
    !between.trim().is_empty()
}

fn signature_has_non_void_return(sig: &str) -> bool {
    // Look for `: Type` after the closing paren but before `{` or `=>`.
    let after_paren = match sig.find(')') {
        Some(i) => &sig[i + 1..],
        None => return false,
    };
    let end = after_paren
        .find('{')
        .or_else(|| after_paren.find("=>"))
        .unwrap_or(after_paren.len());
    let ret_section = after_paren[..end].trim();
    let ret = match ret_section.strip_prefix(':') {
        Some(r) => r.trim(),
        None => return false,
    };
    !(ret.is_empty()
        || ret == "void"
        || ret == "Promise<void>"
        || ret.starts_with("void ")
        || ret.starts_with("Promise<void>"))
}

crate::ast_check! { on ["comment"] prefilter = ["/**"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in find_jsdoc_blocks(text) {
        let code = following_code(ctx.source, text);
        let sig = match exported_function_signature(code) {
            Some(s) => s,
            None => continue,
        };
        let tags = parse_tags(&block.content);

        if signature_has_params(&sig) && !has_tag(&tags, "param") {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: block.start_line + 1 + line_offset,
                column: 1,
                rule_id: "jsdoc/require-tags".into(),
                message: "Exported function has parameters but no `@param` tags in its JSDoc.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        if signature_has_non_void_return(&sig)
            && !has_tag(&tags, "returns")
            && !has_tag(&tags, "return")
        {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: block.start_line + 1 + line_offset,
                column: 1,
                rule_id: "jsdoc/require-tags".into(),
                message: "Exported function returns a value but JSDoc has no `@returns` tag.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
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
    fn flags_exported_fn_missing_param() {
        let src = "/**\n * does stuff\n */\nexport function f(x: number): void { console.log(x); }";
        let d = run(src);
        assert!(d.iter().any(|x| x.message.contains("@param")));
    }

    #[test]
    fn flags_exported_fn_missing_returns() {
        let src = "/**\n * does stuff\n */\nexport function f(): number { return 1; }";
        let d = run(src);
        assert!(d.iter().any(|x| x.message.contains("@returns")));
    }

    #[test]
    fn allows_exported_fn_with_param_and_returns() {
        let src = "/**\n * @param x - input\n * @returns output\n */\nexport function f(x: number): number { return x; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_exported_fn() {
        let src = "/**\n * internal\n */\nfunction f(x: number): number { return x; }";
        assert!(run(src).is_empty());
    }
}
