pub(crate) mod batch_key;
pub(crate) mod call_directive;
pub(crate) mod resolver_directive;

pub(crate) use batch_key::parse_batch_key_directive;
pub(crate) use call_directive::parse_call_directive;
pub(crate) use resolver_directive::parse_resolver_directive;

use async_graphql_parser::types::ConstDirective;
use async_graphql_value::ConstValue;

pub(crate) fn get_directive_argument<'a>(
    directive: &'a ConstDirective,
    name: &str,
) -> Option<&'a ConstValue> {
    directive
        .arguments
        .iter()
        .find(|(n, _)| n.node.as_str() == name)
        .map(|(_, v)| &v.node)
}

pub(crate) fn get_string_argument(directive: &ConstDirective, name: &str) -> Option<String> {
    get_directive_argument(directive, name).and_then(|v| match v {
        ConstValue::String(s) => Some(s.clone()),
        _ => None,
    })
}

pub(crate) fn find_directive<'a>(
    directives: &'a [async_graphql_parser::Positioned<ConstDirective>],
    name: &str,
) -> Option<&'a ConstDirective> {
    directives
        .iter()
        .find(|d| d.node.name.node.as_str() == name)
        .map(|d| &d.node)
}
