use async_graphql_parser::types::ConstDirective;

use super::get_string_argument;

#[derive(Debug, Clone)]
pub(crate) struct TraitDirective {
    pub name: String,
}

pub(crate) fn parse_trait_directive(directive: &ConstDirective) -> Option<TraitDirective> {
    if directive.name.node.as_str() != "trait" {
        return None;
    }

    let name = get_string_argument(directive, "name")?;
    Some(TraitDirective { name })
}
