//! ts-parameter-properties OxcCheck backend — flag constructor parameters
//! that use accessibility modifiers to implicitly declare class properties.

use std::sync::Arc;

use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else {
            return;
        };

        // Skip decorated classes (e.g. @Injectable, @Controller).
        if !class.decorators.is_empty() {
            return;
        }

        let Some(_body) = &class.body.body.first() else {
            return;
        };

        // Walk the class body to find a constructor.
        for element in &class.body.body {
            let oxc_ast::ast::ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            if method.kind != oxc_ast::ast::MethodDefinitionKind::Constructor {
                continue;
            }

            // Check each parameter for accessibility modifier or readonly.
            for param in &method.value.params.items {
                let has_modifier = param.accessibility.is_some() || param.readonly;
                if !has_modifier {
                    continue;
                }

                // Skip parameters carrying a decorator (e.g. @Inject, @Optional)
                // — framework dependency injection relies on parameter properties.
                if !param.decorators.is_empty() {
                    continue;
                }

                let param_name = &ctx.source
                    [param.pattern.span().start as usize..param.pattern.span().end as usize];
                // Extract just the name (strip type annotation).
                let name = param_name.split(':').next().unwrap_or(param_name).trim();

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, param.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "ts-parameter-properties".into(),
                    message: format!(
                        "Property `{name}` should be declared as a class property."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_by_id("ts-parameter-properties", src, "t.ts")
    }

    #[test]
    fn flags_plain_parameter_property() {
        let diags = run("class Foo { constructor(private readonly bar: Bar) {} }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bar"));
    }

    #[test]
    fn allows_parameter_property_in_decorated_class() {
        let src =
            "@Injectable()\nexport class CatsService {\n  constructor(private readonly repo: CatsRepository) {}\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_decorated_parameter_property() {
        let src =
            "class HeroController {\n  constructor(@Inject('HERO_PACKAGE') private readonly client: ClientGrpc) {}\n}";
        assert!(run(src).is_empty());
    }
}
