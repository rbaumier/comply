//! OxcCheck backend for use-react-native-platform-components.
//!
//! Collects every component imported from `react-native` — through an ES import
//! (`import { ProgressBarAndroid } from "react-native"`) or a destructured
//! `require` (`const { ProgressBarAndroid } = require("react-native")`) — and
//! classifies each by the platform marker in its *source* name: `Android` for
//! Android, `IOS` for iOS. A marked component is flagged unless the file is of
//! the matching platform (its path matches a configured glob). When a file that
//! is neither platform imports both an Android and an iOS component, every such
//! import is flagged as a mixing violation instead.

use std::sync::Arc;

use globset::{Glob, GlobMatcher};
use oxc_ast::ast::{
    BindingPattern, Expression, ImportDeclarationSpecifier, PropertyKey, Statement,
};
use oxc_span::{GetSpan, Span};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

const REACT_NATIVE: &str = "react-native";

pub struct Check;

/// Which platform a component name marks, derived from a substring of its name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Platform {
    Android,
    Ios,
}

/// The platform marker of a `react-native` component's *source* name, or `None`
/// for a platform-agnostic component. Mirrors Biome: `Android` substring wins
/// over `IOS` when both appear.
fn component_platform(name: &str) -> Option<Platform> {
    if name.contains("Android") {
        Some(Platform::Android)
    } else if name.contains("IOS") {
        Some(Platform::Ios)
    } else {
        None
    }
}

/// A platform-marked `react-native` import to potentially flag: its source
/// name, platform, and the span to point the diagnostic at.
struct Marked {
    name: String,
    platform: Platform,
    span: Span,
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Program]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Every firing path requires an import from this literal specifier.
        Some(&[REACT_NATIVE])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Program(program) = node.kind() else {
            return;
        };

        let mut marked = Vec::new();
        for stmt in &program.body {
            match stmt {
                Statement::ImportDeclaration(import) => {
                    collect_import(import, &mut marked);
                }
                Statement::VariableDeclaration(decl) => {
                    for declarator in &decl.declarations {
                        collect_require(declarator, &mut marked);
                    }
                }
                _ => {}
            }
        }
        if marked.is_empty() {
            return;
        }

        let path = ctx.path.to_string_lossy();
        let is_android_file = path_matches(ctx, "android_path_patterns", &path);
        let is_ios_file = !is_android_file && path_matches(ctx, "ios_path_patterns", &path);

        // A non-platform file holding both Android and iOS components is a
        // mixing violation; every marked import is then reported as such.
        let has_android = marked.iter().any(|m| m.platform == Platform::Android);
        let has_ios = marked.iter().any(|m| m.platform == Platform::Ios);
        let is_mixing = !is_android_file && !is_ios_file && has_android && has_ios;

        for item in &marked {
            let in_matching_file = match item.platform {
                Platform::Android => is_android_file,
                Platform::Ios => is_ios_file,
            };
            if in_matching_file {
                continue;
            }
            let message = if is_mixing {
                format!(
                    "iOS and Android components cannot be mixed in the same file — `{}` is platform-specific. Split iOS and Android components into separate platform-specific files.",
                    item.name
                )
            } else {
                match item.platform {
                    Platform::Android => format!(
                        "Android component `{}` is used outside of an Android-specific file. Move this import to a file with an Android-specific suffix (e.g. `.android.js`).",
                        item.name
                    ),
                    Platform::Ios => format!(
                        "iOS component `{}` is used outside of an iOS-specific file. Move this import to a file with an iOS-specific suffix (e.g. `.ios.js`).",
                        item.name
                    ),
                }
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, item.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message,
                severity: Severity::Warning,
                span: Some((item.span.start as usize, item.span.size() as usize)),
            });
        }
    }
}

/// Collect platform-marked named imports from `import … from "react-native"`.
/// The *source* name (`imported`) drives classification, not a local alias.
fn collect_import<'a>(import: &oxc_ast::ast::ImportDeclaration<'a>, out: &mut Vec<Marked>) {
    if import.source.value.as_str() != REACT_NATIVE || import.import_kind.is_type() {
        return;
    }
    let Some(specifiers) = &import.specifiers else {
        return;
    };
    for spec in specifiers {
        let ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
            continue;
        };
        if named.import_kind.is_type() {
            continue;
        }
        let name = named.imported.name();
        if let Some(platform) = component_platform(name.as_str()) {
            out.push(Marked {
                name: name.as_str().to_owned(),
                platform,
                span: named.span,
            });
        }
    }
}

/// Collect platform-marked names from `const { … } = require("react-native")`.
/// The destructuring *key* is the source name; the local binding may be aliased.
fn collect_require<'a>(declarator: &oxc_ast::ast::VariableDeclarator<'a>, out: &mut Vec<Marked>) {
    let Some(Expression::CallExpression(call)) = &declarator.init else {
        return;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return;
    };
    if callee.name.as_str() != "require" {
        return;
    }
    let Some(oxc_ast::ast::Argument::StringLiteral(source)) = call.arguments.first() else {
        return;
    };
    if source.value.as_str() != REACT_NATIVE {
        return;
    }
    let BindingPattern::ObjectPattern(pattern) = &declarator.id else {
        return;
    };
    for prop in &pattern.properties {
        let name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if let Some(platform) = component_platform(name) {
            out.push(Marked {
                name: name.to_owned(),
                platform,
                span: prop.span(),
            });
        }
    }
}

/// True when the file path matches any configured glob for `key`. An invalid
/// glob in config is skipped rather than aborting the rule.
fn path_matches(ctx: &CheckCtx, key: &str, path: &str) -> bool {
    let patterns = ctx.config.string_list(super::META.id, key, ctx.lang);
    patterns
        .iter()
        .filter_map(|p| compile_glob(p))
        .any(|matcher| matcher.is_match(path))
}

fn compile_glob(pattern: &str) -> Option<GlobMatcher> {
    Glob::new(pattern).ok().map(|g| g.compile_matcher())
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
