//! jsx-no-undef OXC backend — walk every `JSXOpeningElement` and flag
//! PascalCase tag identifiers that don't resolve to any symbol in the file.

use std::collections::HashSet;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXElementName;

pub struct Check;

fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let defined: HashSet<String> = scoping.symbol_names().map(|s| s.to_string()).collect();

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::JSXOpeningElement(opening) = node.kind() else { continue };
            let (name, span_start) = match &opening.name {
                JSXElementName::IdentifierReference(ident) => {
                    (ident.name.as_str(), ident.span.start as usize)
                }
                JSXElementName::Identifier(ident) => {
                    (ident.name.as_str(), ident.span.start as usize)
                }
                _ => continue,
            };

            if !starts_with_uppercase(name) {
                continue;
            }

            if defined.contains(name) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("`{name}` is not defined."),
                severity: Severity::Error,
                span: None,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }


    #[test]
    fn flags_undefined_component() {
        let src = r#"
function App() {
  return <MyComponent />;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("MyComponent"));
    }


    #[test]
    fn allows_imported_component() {
        let src = r#"
import { Button } from './Button';
function App() {
  return <Button />;
}
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_default_imported_component() {
        let src = r#"
import Modal from './Modal';
function App() {
  return <Modal />;
}
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_locally_declared_component() {
        let src = r#"
function App() {
  const Card = (props: any) => <div />;
  return <Card />;
}
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_top_level_function_component() {
        let src = r#"
function MyComponent() { return <div />; }
function App() {
  return <MyComponent />;
}
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_lowercase_html_intrinsics() {
        let src = r#"
function App() {
  return <div><span /></div>;
}
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_fragment() {
        let src = r#"
function App() {
  return <></>;
}
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_member_expression_tag() {
        let src = r#"
function App() {
  return <React.Fragment>hi</React.Fragment>;
}
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn flags_multiple_undefined_components() {
        let src = r#"
function App() {
  return <Foo><Bar /></Foo>;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 2);
    }
}
