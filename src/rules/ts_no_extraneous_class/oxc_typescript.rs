//! ts-no-extraneous-class OxcCheck backend — flag classes that are empty,
//! contain only a constructor, or contain only static members.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, MethodDefinitionKind};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else { return };

        // Skip classes that extend a superclass.
        if class.super_class.is_some() {
            return;
        }

        // Skip decorated classes.
        if !class.decorators.is_empty() {
            return;
        }
        // Also check parent for decorator (export @Decorator class Foo {}).
        if let Some(parent) = semantic.nodes().ancestors(node.id()).nth(1)
            && let AstKind::ExportDefaultDeclaration(_) | AstKind::ExportNamedDeclaration(_) =
                parent.kind()
            {
                // Check the source text before class for `@`.
                let class_start = class.span.start as usize;
                if class_start > 0 {
                    let before = ctx.source[..class_start].trim_end();
                    if before.ends_with(')') || before.ends_with('}') {
                        // Possible decorator — check more carefully.
                        let last_line = before.lines().last().unwrap_or("");
                        if last_line.trim_start().starts_with('@') {
                            return;
                        }
                    }
                    if before.lines().last().is_some_and(|l| l.trim_start().starts_with('@')) {
                        return;
                    }
                }
            }

        let body = &class.body;
        let members: Vec<_> = body
            .body
            .iter()
            .filter(|m| !matches!(m, ClassElement::StaticBlock(_)))
            .collect();

        if members.is_empty() {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, class.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Unexpected empty class.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        let mut only_constructor = true;
        let mut only_static = true;

        for member in &members {
            match member {
                ClassElement::MethodDefinition(method) => {
                    if method.kind == MethodDefinitionKind::Constructor {
                        // Check for parameter properties.
                        for param in &method.value.params.items {
                            if param.accessibility.is_some() {
                                only_constructor = false;
                                only_static = false;
                            }
                        }
                    } else {
                        only_constructor = false;
                        if !method.r#static {
                            only_static = false;
                        }
                    }
                }
                ClassElement::PropertyDefinition(prop) => {
                    only_constructor = false;
                    if !prop.r#static {
                        only_static = false;
                    }
                }
                ClassElement::AccessorProperty(prop) => {
                    only_constructor = false;
                    if !prop.r#static {
                        only_static = false;
                    }
                }
                ClassElement::TSIndexSignature(_) => {
                    only_constructor = false;
                    only_static = false;
                }
                ClassElement::StaticBlock(_) => {}
            }
            if !only_constructor && !only_static {
                break;
            }
        }

        let msg = if only_constructor {
            "Unexpected class with only a constructor."
        } else if only_static {
            "Unexpected class with only static properties."
        } else {
            return;
        };
        let (line, column) =
            byte_offset_to_line_col(ctx.source, class.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: msg.into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_empty_class() {
        let diags = run_on("class Empty {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("empty"));
    }


    #[test]
    fn flags_only_static() {
        let diags = run_on("class Utils { static foo() {} }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("static"));
    }


    #[test]
    fn allows_class_with_extends() {
        assert!(run_on("class Foo extends Bar {}").is_empty());
    }


    #[test]
    fn allows_class_with_instance_method() {
        assert!(run_on("class Foo { bar() {} }").is_empty());
    }


    #[test]
    fn allows_decorated_empty_class() {
        assert!(run_on("@Component\nclass Foo {}").is_empty());
    }


    #[test]
    fn allows_exported_decorated_empty_class() {
        assert!(run_on("@Module({})\nexport class AppModule {}").is_empty());
    }
}
