use async_graphql_parser::types::ConstDirective;

use super::get_string_argument;

#[derive(Debug, Clone)]
pub(crate) struct BatchKeyDirective {
    pub field: String,
}

pub(crate) fn parse_batch_key_directive(directive: &ConstDirective) -> Option<BatchKeyDirective> {
    if directive.name.node.as_str() != "batchKey" {
        return None;
    }

    let field = get_string_argument(directive, "field")?;
    Some(BatchKeyDirective { field })
}
