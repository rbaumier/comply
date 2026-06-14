use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TemplateLiteral(tpl) = node.kind() else {
            return;
        };
        // Only check simple template literals (no expressions / substitutions).
        if !tpl.expressions.is_empty() {
            return;
        }
        if tpl.quasis.len() != 1 {
            return;
        }
        let body = tpl.quasis[0].value.raw.as_str();
        if !body.contains('\n') {
            return;
        }
        let min_indent = ctx.config.threshold("template-indent", "min_indent", ctx.lang);
        let Some(indent) = common_leading_whitespace(body) else {
            return;
        };
        if indent < min_indent {
            return;
        }
        let parent = semantic.nodes().parent_node(node.id());
        // Tagged templates delegate indentation management to the tag function.
        if matches!(parent.kind(), AstKind::TaggedTemplateExpression(_)) {
            return;
        }
        // Angular `@Component({ template: `...` })` inline templates inherit the
        // class-body indentation by construction; the template compiler strips
        // it, so it is not excess whitespace the author can remove.
        if is_angular_component_template(parent, semantic, ctx.source) {
            return;
        }
        // Rule-tester test cases embed code-under-test as template literals
        // whose indentation reflects the structure of the tested source, not
        // excess whitespace inherited from the surrounding scope.
        if is_rule_tester_snippet(parent, semantic) {
            return;
        }
        // Snapshot assertions intentionally preserve indentation for readability.
        if let AstKind::CallExpression(call) = parent.kind() {
            if let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee {
                let method = member.property.name.as_str();
                if matches!(
                    method,
                    "toMatchSnapshot"
                        | "toMatchInlineSnapshot"
                        | "toThrowErrorMatchingInlineSnapshot"
                ) {
                    return;
                }
            }
        }
        // A method call on the template that strips the indentation at runtime
        // (`.trim()`, `.replaceAll(/\n */gm, "")`, …) already handles it: the
        // template is the member's object and the member is invoked.
        if let AstKind::StaticMemberExpression(member) = parent.kind() {
            if strips_indentation(member.property.name.as_str()) {
                let grandparent = semantic.nodes().parent_node(parent.id());
                if matches!(grandparent.kind(), AstKind::CallExpression(_)) {
                    return;
                }
            }
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, tpl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Template literal has {indent} spaces of common leading indentation \
                 inherited from the surrounding code — strip it or use a dedent helper."
            ),
            severity: super::META.severity,
            span: Some((tpl.span.start as usize, (tpl.span.end - tpl.span.start) as usize)),
        });
    }
}

/// Whether the template literal is the code-under-test of a rule-tester case.
///
/// ESLint / typescript-eslint rule testers express the tested source inline as
/// a template literal: the value of a `code`/`output` property, or a bare entry
/// in a `valid`/`invalid` array. Its indentation mirrors the structure of the
/// source being tested, so stripping it would corrupt the test input.
fn is_rule_tester_snippet<'a>(
    parent: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    match parent.kind() {
        // `{ code: \`...\` }` / `{ output: \`...\` }`
        AstKind::ObjectProperty(prop) => {
            matches!(object_property_key(&prop.key), Some("code" | "output"))
        }
        // `{ valid: [\`...\`] }` / `{ invalid: [\`...\`] }`
        AstKind::ArrayExpression(_) => {
            let grandparent = semantic.nodes().parent_node(parent.id());
            if let AstKind::ObjectProperty(prop) = grandparent.kind() {
                matches!(object_property_key(&prop.key), Some("valid" | "invalid"))
            } else {
                false
            }
        }
        _ => false,
    }
}

/// The static name of an object-property key, or `None` for computed keys.
fn object_property_key<'a>(key: &'a oxc_ast::ast::PropertyKey<'a>) -> Option<&'a str> {
    use oxc_ast::ast::PropertyKey;
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// Whether the template literal is the value of an Angular `@Component`
/// inline `template` property.
///
/// Walks `TemplateLiteral` → `ObjectProperty { key: template }`
/// → `ObjectExpression` → `CallExpression { callee: Component }`
/// → `Decorator`. Such templates inherit the surrounding class-body
/// indentation by construction and the Angular compiler strips it, so the
/// indentation is not excess whitespace the author can remove.
fn is_angular_component_template<'a>(
    parent: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    use oxc_ast::ast::Expression;

    let AstKind::ObjectProperty(prop) = parent.kind() else {
        return false;
    };
    if object_property_key(&prop.key) != Some("template") {
        return false;
    }
    if !crate::oxc_helpers::source_contains(source, "@Component") {
        return false;
    }
    let object = semantic.nodes().parent_node(parent.id());
    if !matches!(object.kind(), AstKind::ObjectExpression(_)) {
        return false;
    }
    let call = semantic.nodes().parent_node(object.id());
    let AstKind::CallExpression(call_expr) = call.kind() else {
        return false;
    };
    if !matches!(&call_expr.callee, Expression::Identifier(id) if id.name.as_str() == "Component") {
        return false;
    }
    matches!(
        semantic.nodes().parent_node(call.id()).kind(),
        AstKind::Decorator(_)
    )
}

/// Whether a method call on the template literal removes its leading
/// whitespace at runtime, so the inherited indentation never reaches output.
fn strips_indentation(method: &str) -> bool {
    matches!(
        method,
        "trim" | "trimStart" | "trimEnd" | "replace" | "replaceAll"
    )
}

/// Compute the minimum leading-whitespace count across the template's
/// non-empty content lines.
///
/// Skips the first physical line (opens immediately after the backtick)
/// and the last physical line (just whitespace before the closing backtick).
fn common_leading_whitespace(body: &str) -> Option<usize> {
    let lines: Vec<&str> = body.split('\n').collect();
    if lines.len() < 3 {
        return None;
    }
    let mut min_ws = usize::MAX;
    let mut has_content = false;
    for line in &lines[1..lines.len() - 1] {
        if line.trim().is_empty() {
            continue;
        }
        has_content = true;
        let leading = line.len() - line.trim_start().len();
        min_ws = min_ws.min(leading);
    }
    if has_content {
        Some(min_ws)
    } else {
        None
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_indented_template() {
        let src = r#"
function foo() {
    const html = `
        <div>
            <p>Hello</p>
        </div>
    `;
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("common leading indentation"));
    }

    #[test]
    fn allows_template_without_excess_indent() {
        let src = r#"
const html = `
<div>
  <p>Hello</p>
</div>
`;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_line_template() {
        assert!(run("const x = `hello world`;").is_empty());
    }

    #[test]
    fn allows_template_with_minimal_indent() {
        let src = "const x = `\n  a\n  b\n`;\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_deeply_nested_template() {
        let src = r#"
if (true) {
    if (true) {
        const sql = `
            SELECT *
            FROM users
            WHERE id = 1
        `;
    }
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_backtick_in_string_literal() {
        let src = "const s = '`\n        line\n        line\n`';";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_dedent_tagged_template() {
        let src = "dedenter`\n    line 1\n    line 2\n`;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_template_stripped_by_replace_all() {
        let src = r#"
it('html-pretty', () => {
    const div = document.createElement("div")
    div.innerHTML = `
        <form>
            <label for="email">Email Address</label>
            <input name="email" />
        </form>
    `.replaceAll(/\n */gm, "")
})
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_template_stripped_by_trim() {
        let src = r#"
function foo() {
    const html = `
        <div>
            <p>Hello</p>
        </div>
    `.trim();
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_template_with_non_stripping_method() {
        let src = r#"
function foo() {
    const html = `
        <div>
            <p>Hello</p>
        </div>
    `.split("\n");
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_snapshot_inline_assertion() {
        let src = r#"
expect(() => validate(x)).toThrowErrorMatchingInlineSnapshot(`
    [Error:
      Must be a string
    ]
`);
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_rule_tester_code_property() {
        let src = r#"
ruleTester.run('prefer-optional-chain', rule, {
    invalid: [
        {
            code: `
                if (foo) {
                    (foo || {}).bar;
                }
            `,
            errors: [{ messageId: 'preferOptionalChain' }],
        },
    ],
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_rule_tester_output_property() {
        let src = r#"
ruleTester.run('rule', rule, {
    invalid: [
        {
            code: `let x = 1`,
            output: `
                const x = 1;
            `,
        },
    ],
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_rule_tester_bare_valid_case() {
        let src = r#"
ruleTester.run('rule', rule, {
    valid: [
        `
            const x = {
                a: 1,
            };
        `,
    ],
    invalid: [],
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_angular_component_inline_template() {
        let src = r#"
import { Component } from '@angular/core';

@Component({
  selector: 'ngrx-root',
  standalone: true,
  template: `
    <h1>Welcome {{ title }} {{ val() }}</h1>

    <a routerLink="/feature">Load Feature</a>

    <router-outlet></router-outlet>
  `,
})
export class AppComponent {}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_template_property_in_plain_component_call() {
        // A `Component(...)` call that is not a decorator is not an Angular
        // inline template — the indented template literal still fires.
        let src = r#"
const config = Component({
    template: `
        <div>
            <p>Hello</p>
        </div>
    `,
});
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("common leading indentation"));
    }

    #[test]
    fn flags_indented_template_in_non_tester_property() {
        let src = r#"
function render() {
    return {
        html: `
            <div>
                <p>Hello</p>
            </div>
        `,
    };
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("common leading indentation"));
    }
}
