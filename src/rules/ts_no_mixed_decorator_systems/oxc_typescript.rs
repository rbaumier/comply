//! OXC backend for ts-no-mixed-decorator-systems.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["reflect-metadata"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut has_reflect_metadata = false;
        let mut first_decorator_span: Option<oxc_span::Span> = None;

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::ImportDeclaration(import) => {
                    let src = import.source.value.as_str();
                    if src == "reflect-metadata" {
                        has_reflect_metadata = true;
                    }
                }
                AstKind::Class(class) => {
                    if first_decorator_span.is_none() && !class.decorators.is_empty() {
                        first_decorator_span = Some(class.decorators[0].span);
                    }
                }
                AstKind::MethodDefinition(method) => {
                    if first_decorator_span.is_none() && !method.decorators.is_empty() {
                        first_decorator_span = Some(method.decorators[0].span);
                    }
                }
                AstKind::PropertyDefinition(prop) => {
                    if first_decorator_span.is_none() && !prop.decorators.is_empty() {
                        first_decorator_span = Some(prop.decorators[0].span);
                    }
                }
                _ => {}
            }
        }

        if !has_reflect_metadata {
            return Vec::new();
        }
        let Some(span) = first_decorator_span else {
            return Vec::new();
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "File mixes decorators with a `reflect-metadata` import — standard and experimental decorator systems cannot coexist.".into(),
            severity: Severity::Error,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_mixed_decorators_with_reflect_metadata() {
        let src = "import 'reflect-metadata';\n@Injectable() class Svc {}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_decorators_without_reflect_metadata() {
        let src = "@Injectable() class Svc {}";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_reflect_metadata_without_decorators() {
        let src = "import 'reflect-metadata';\nconst x = 1;";
        assert!(run(src).is_empty());
    }
}
