//! Shared tree-sitter grammar setup.

use crate::files::Language;
use tree_sitter::Parser;

/// Map a `crate::files::Language` to the corresponding tree-sitter grammar
/// `Language` object, or `None` for languages we don't have a grammar for
/// (e.g. Toml/Json/Sql).
pub(crate) fn ts_language_for(language: Language) -> Option<tree_sitter::Language> {
    Some(match language {
        Language::TypeScript | Language::JavaScript => {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        }
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::Vue => tree_sitter_vue_updated::language(),
        Language::Css => tree_sitter_css::LANGUAGE.into(),
        Language::Yaml => tree_sitter_yaml::LANGUAGE.into(),
        Language::Dockerfile => tree_sitter_dockerfile_updated::language(),
        Language::Toml | Language::Json | Language::Sql | Language::GraphQl => return None,
    })
}

/// Configure the parser for the language and parse the source.
///
/// Returns None when no tree-sitter grammar is bundled for the language.
pub(crate) fn parse_with_grammar(
    parser: &mut Parser,
    language: Language,
    source: &[u8],
) -> Option<tree_sitter::Tree> {
    let lang = ts_language_for(language)?;
    parser.set_language(&lang).ok()?;
    parser.parse(source, None)
}
