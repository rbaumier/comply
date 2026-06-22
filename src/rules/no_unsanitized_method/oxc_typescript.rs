//! no-unsanitized-method OXC backend — flag unsafe HTML-injection method calls
//! whose HTML argument is not a static string literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Returns the 0-based argument index carrying the HTML payload for the given
/// method name, or `None` if the method is not targeted.
fn html_arg_index(method: &str) -> Option<usize> {
    match method {
        "insertAdjacentHTML" => Some(1),
        "write" | "writeln" | "setHTMLUnsafe" | "createContextualFragment" => Some(0),
        _ => None,
    }
}

/// True when `expr` is a safe, fully-static string expression.
fn is_static_string(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => tpl.expressions.is_empty(),
        _ => false,
    }
}

/// True when `expr` resolves to a DOM `document` receiver: a bare `document`
/// identifier, or a member access whose final property is `document` or
/// `contentDocument` (e.g. `window.document`, `iframe.contentDocument`).
fn is_document_like_receiver(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(ident) => ident.name == "document",
        Expression::StaticMemberExpression(member) => {
            matches!(member.property.name.as_str(), "document" | "contentDocument")
        }
        _ => false,
    }
}

/// Playwright/Puppeteer methods that serialize a function argument and execute
/// it inside a controlled automation browser context, not the application DOM.
const BROWSER_INJECTION_METHODS: &[&str] = &[
    "evaluate",
    "evaluateHandle",
    "evaluateOnNewDocument",
    "addInitScript",
    "$eval",
    "$$eval",
];

/// True when `node` sits inside a function/arrow that is a direct argument of a
/// browser-injection call — `page.evaluate(() => { document.write(html) })`,
/// `frame.addInitScript(...)`, `page.$eval(...)`, etc. The callback is
/// serialized and run in the automation browser, so the HTML written there is
/// not the application's XSS sink.
fn is_inside_browser_injection_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        if matches!(
            ancestor.kind(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) && let AstKind::CallExpression(call) = nodes.parent_node(ancestor.id()).kind()
            && let Expression::StaticMemberExpression(member) = &call.callee
            && BROWSER_INJECTION_METHODS.contains(&member.property.name.as_str())
        {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "insertAdjacentHTML",
            "setHTMLUnsafe",
            "createContextualFragment",
            "writeln",
            "write",
        ])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be a member expression.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        let Some(idx) = html_arg_index(method) else {
            return;
        };

        // `write`/`writeln` collide with Node.js `Writable` streams, so only
        // flag them on a `document`-like receiver. The DOM-specific methods
        // (`insertAdjacentHTML`, `setHTMLUnsafe`, `createContextualFragment`)
        // have no such collision and fire on any receiver.
        if matches!(method, "write" | "writeln") && !is_document_like_receiver(&member.object) {
            return;
        }

        // Calls inside a Playwright/Puppeteer browser-injection callback run in
        // a controlled automation browser, not the application DOM, so the HTML
        // written there is not the XSS sink this rule targets.
        if is_inside_browser_injection_callback(node, semantic) {
            return;
        }

        let Some(arg) = call.arguments.get(idx) else {
            return;
        };
        let arg_expr = arg.as_expression();
        let Some(arg_expr) = arg_expr else {
            return;
        };
        if is_static_string(arg_expr) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Calling `{method}` with a non-literal HTML argument is an XSS vector — avoid dynamic HTML injection, or sanitize input first."
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
    fn flags_document_write_concat() {
        assert_eq!(run_on("document.write('<p>' + name + '</p>');").len(), 1);
    }

    #[test]
    fn flags_document_writeln_variable() {
        assert_eq!(run_on("document.writeln(x);").len(), 1);
    }

    #[test]
    fn flags_window_document_write() {
        assert_eq!(run_on("window.document.write(x);").len(), 1);
    }

    #[test]
    fn flags_contentdocument_write() {
        assert_eq!(run_on("iframe.contentDocument.write(x);").len(), 1);
    }

    #[test]
    fn flags_insert_adjacent_html_any_receiver() {
        assert_eq!(run_on("el.insertAdjacentHTML('beforeend', html);").len(), 1);
    }

    #[test]
    fn allows_stream_write() {
        assert!(run_on("stream.write(output);").is_empty());
    }

    #[test]
    fn allows_nested_stdout_write() {
        assert!(run_on("proc.stdout.write(x);").is_empty());
    }

    #[test]
    fn allows_this_member_write() {
        assert!(run_on("this._writeTo.write(y);").is_empty());
    }

    #[test]
    fn allows_document_write_literal() {
        assert!(run_on("document.write('<p>static</p>');").is_empty());
    }

    #[test]
    fn allows_document_write_in_page_evaluate_callback() {
        assert!(
            run_on(
                "page.evaluate(({ html }) => {\n  document.open();\n  document.write(html);\n  document.close();\n}, { html });"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_document_write_in_add_init_script_callback() {
        assert!(run_on("page.addInitScript(() => { document.write(content); });").is_empty());
    }

    #[test]
    fn allows_document_write_in_eval_helper_callback() {
        assert!(run_on("page.$eval('#sel', () => { document.write(markup); });").is_empty());
    }

    #[test]
    fn flags_top_level_document_write_outside_injection_callback() {
        assert_eq!(run_on("document.write(userInput);").len(), 1);
    }

    #[test]
    fn flags_document_write_in_non_injection_callback() {
        assert_eq!(
            run_on("setTimeout(() => { document.write(userInput); }, 0);").len(),
            1
        );
    }

    #[test]
    fn flags_document_write_in_non_injection_member_callback() {
        assert_eq!(
            run_on("widget.render(() => { document.write(userInput); });").len(),
            1
        );
    }
}
