use async_graphql_parser::types::ConstDirective;

use super::get_string_argument;

#[derive(Debug, Clone)]
pub(crate) struct ResolverDirective {
    pub name: String,
}

pub(crate) fn parse_resolver_directive(directive: &ConstDirective) -> Option<ResolverDirective> {
    if directive.name.node.as_str() != "resolver" {
        return None;
    }

    let name = get_string_argument(directive, "name")?;
    Some(ResolverDirective { name })
}
