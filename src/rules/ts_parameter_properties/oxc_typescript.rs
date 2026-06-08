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
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_public_parameter_property() {
        let diags = run_on("class Foo { constructor(public name: string) {} }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("name"));
    }


    #[test]
    fn flags_readonly_parameter_property() {
        let diags = run_on("class Foo { constructor(readonly id: number) {} }");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_normal_parameter() {
        assert!(run_on("class Foo { constructor(name: string) {} }").is_empty());
    }


    #[test]
    fn allows_parameter_property_in_decorated_class() {
        let src = "@Injectable()\nclass Foo { constructor(private readonly service: Service) {} }";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_parameter_property_in_exported_decorated_class() {
        let src = "@Controller()\nexport class Foo { constructor(private readonly service: Service) {} }";
        assert!(run_on(src).is_empty());
    }
}
