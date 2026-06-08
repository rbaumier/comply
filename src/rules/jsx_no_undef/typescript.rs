//! jsx-no-undef backend — walk every `JSXOpeningElement` via oxc_semantic,
//! and flag PascalCase tag identifiers that don't resolve to any symbol
//! declared in the file (imports, locals, top-level declarations).
//!
//! Lowercase tag names are HTML intrinsics and are never flagged. Tags
//! written as member expressions (`<Foo.Bar />`), namespaced names
//! (`<svg:rect />`), `this` expressions and fragments (`<></>`) are
//! skipped — they are out of scope for this rule.

use rustc_hash::FxHashSet;

use oxc_ast::AstKind;
use oxc_ast::ast::JSXElementName;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{source_type_for_path, with_semantic};
use crate::rules::backend::CheckCtx;

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_type = source_type_for_path(ctx.path);
        with_semantic(ctx.source, source_type, |semantic| {
            let scoping = semantic.scoping();
            let defined: FxHashSet<String> = scoping.symbol_names().map(|s| s.to_string()).collect();

            let mut diagnostics = Vec::new();
            for node in semantic.nodes().iter() {
                let AstKind::JSXOpeningElement(opening) = node.kind() else {
                    continue;
                };
                let (name, span_start) = match &opening.name {
                    JSXElementName::IdentifierReference(ident) => {
                        (ident.name.as_str(), ident.span.start as usize)
                    }
                    JSXElementName::Identifier(ident) => {
                        (ident.name.as_str(), ident.span.start as usize)
                    }
                    // Member expressions, namespaced names, `this` and
                    // fragments are out of scope for this rule.
                    _ => continue,
                };

                // HTML intrinsics (`<div />`) start with a lowercase
                // letter. Skip them.
                if !starts_with_uppercase(name) {
                    continue;
                }

                if defined.contains(name) {
                    continue;
                }

                let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "jsx-no-undef".into(),
                    message: format!("`{name}` is not defined."),
                    severity: Severity::Error,
                    span: None,
                });
            }

            diagnostics
        })
    }
}

fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
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
