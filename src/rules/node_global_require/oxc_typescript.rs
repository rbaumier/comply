//! node-global-require oxc backend — require() must be at module top level.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Image, font, and media extensions that the Metro/Expo bundler resolves
/// statically when passed to `require()` (the documented React Native pattern).
const STATIC_ASSET_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg", ".ttf", ".otf", ".woff", ".woff2",
    ".mp4", ".webm", ".mov", ".m4v", ".mp3", ".wav", ".aac", ".m4a",
];

fn is_static_asset_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    STATIC_ASSET_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["require"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "require" {
            return;
        }

        // React Native / Metro bundle static assets via `require("./img.png")`
        // inside JSX — these are bundler-managed asset references, not CommonJS
        // module loads, and the documented pattern requires them inline. Exempt
        // string-literal arguments pointing at a known static-asset extension.
        if let Some(oxc_ast::ast::Argument::StringLiteral(lit)) = call.arguments.first()
            && is_static_asset_path(lit.value.as_str())
        {
            return;
        }

        // Walk ancestors: require is OK if all ancestors are top-level.
        let mut in_function = false;
        let mut first = true;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if first {
                first = false;
                continue;
            }
            match ancestor.kind() {
                AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::MethodDefinition(_)
                | AstKind::IfStatement(_)
                | AstKind::ForStatement(_)
                | AstKind::ForInStatement(_)
                | AstKind::WhileStatement(_)
                | AstKind::TryStatement(_)
                | AstKind::SwitchStatement(_) => {
                    in_function = true;
                    break;
                }
                _ => {}
            }
        }

        if !in_function {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected `require()`. Move it to the top-level module scope.".into(),
            severity: Severity::Warning,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn allows_image_asset_require_in_jsx() {
        let d = run(
            r#"const x = <Image source={require("@/assets/images/partial-react-logo.png")} />;"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_font_asset_require() {
        assert!(run(r#"function f() { return require("./assets/fonts/Inter.ttf"); }"#).is_empty());
    }

    #[test]
    fn flags_module_require_in_function() {
        let d = run(r#"function init() { const fs = require("fs"); return fs; }"#);
        assert_eq!(d.len(), 1);
    }
}
