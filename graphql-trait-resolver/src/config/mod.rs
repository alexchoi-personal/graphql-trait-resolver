mod parser;
mod schema;

pub(crate) use parser::{parse_sdl, ParseError};
pub(crate) use schema::{
    ArgumentConfig, ArgumentMapping, FieldConfig, FieldType, GraphQLConfig, ResolverConfig, TypeConfig,
};
