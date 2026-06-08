use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["lodash"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else { return };

        let import_path = import.source.value.as_str();
        if import_path != "lodash" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Importing from `lodash` pulls the entire library — \
                      use `lodash/<fn>` subpath imports or `lodash-es` for tree-shaking.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_default_import() {
        assert_eq!(run(r#"import _ from 'lodash';"#).len(), 1);
    }


    #[test]
    fn flags_named_import() {
        assert_eq!(run(r#"import { map } from 'lodash';"#).len(), 1);
    }


    #[test]
    fn flags_namespace_import() {
        assert_eq!(run(r#"import * as _ from 'lodash';"#).len(), 1);
    }


    #[test]
    fn allows_subpath_import() {
        assert!(run(r#"import map from 'lodash/map';"#).is_empty());
    }


    #[test]
    fn allows_lodash_es() {
        assert!(run(r#"import { map } from 'lodash-es';"#).is_empty());
    }


    #[test]
    fn allows_lodash_es_subpath() {
        assert!(run(r#"import map from 'lodash-es/map';"#).is_empty());
    }
}
