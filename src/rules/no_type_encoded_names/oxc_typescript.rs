//! no-type-encoded-names — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator, AstType::FormalParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let name = match node.kind() {
            oxc_ast::AstKind::VariableDeclarator(decl) => {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = decl.id {
                    (&*id.name, id.span())
                } else {
                    return;
                }
            }
            oxc_ast::AstKind::FormalParameter(param) => {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = param.pattern {
                    (&*id.name, id.span())
                } else {
                    return;
                }
            }
            _ => return,
        };

        let (ident, span) = name;
        let Some(prefix) = super::type_prefix::matched_camel_case(ident) else {
            return;
        };
        let (line, col) = byte_offset_to_line_col(semantic.source_text(), span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: format!(
                "'{ident}' encodes a type prefix '{prefix}' — Hungarian notation is \
                 obsolete. Remove the prefix; TypeScript's type checker already \
                 knows the type."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_camel_case_hungarian() {
        assert_eq!(run("const strValue = 'x';").len(), 1);
    }

    // Regression for #279: SCREAMING_SNAKE domain constants are not Hungarian.
    #[test]
    fn allows_screaming_snake_domain_constants() {
        assert!(run("const PROMPTS_DIR = '/p';").is_empty());
        assert!(run("const PROMPT_FILE = 'p.txt';").is_empty());
    }



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_str_prefix() {
        assert_eq!(run_on("const strName = 'x';").len(), 1);
    }


    #[test]
    fn flags_arr_prefix() {
        assert_eq!(run_on("const arrItems = [];").len(), 1);
    }


    #[test]
    fn flags_bool_prefix() {
        assert_eq!(run_on("const boolReady = true;").len(), 1);
    }


    #[test]
    fn allows_descriptive_names() {
        assert!(run_on("const userName = 'x';").is_empty());
        assert!(run_on("const items = [];").is_empty());
        assert!(run_on("const isReady = true;").is_empty());
    }


    #[test]
    fn does_not_flag_word_starting_with_prefix_letters() {
        // 'string' starts with 'str' but there's no camelCase boundary.
        assert!(run_on("const string = 'x';").is_empty());
        // 'array' starts with 'arr' but 'a' is lowercase after.
        assert!(run_on("const arrayList = 1;").is_empty());
    }


    #[test]
    fn does_not_flag_descriptive_fn_callback() {
        // `fn` was previously in the prefix list; flagging `fnCallback`
        // is wrong because it's a descriptive name for a function-typed
        // variable, not Hungarian for some primitive type.
        assert!(run_on("const fnCallback = () => {};").is_empty());
    }


    #[test]
    fn does_not_flag_num_items() {
        // `num_items` / `numItems` is "number of items", not Hungarian
        // for a primitive number variable.
        assert!(run_on("const numItems = 5;").is_empty());
    }


    #[test]
    fn does_not_flag_int_count() {
        // TypeScript has no `int` type — `intCount` is descriptive.
        assert!(run_on("const intCount = 0;").is_empty());
    }


    #[test]
    fn flags_legacy_dbl_prefix() {
        // `dbl` is a legacy C/C++ Hungarian prefix for `double`.
        assert_eq!(run_on("const dblValue = 3.14;").len(), 1);
    }
}
