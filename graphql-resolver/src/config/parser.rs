use async_graphql_parser::types::{
    BaseType, ConstDirective, FieldDefinition, SchemaDefinition, ServiceDocument, Type,
    TypeDefinition, TypeKind, TypeSystemDefinition,
};
use async_graphql_value::ConstValue;

use super::schema::{
    ArgumentConfig, FieldConfig, FieldType, GraphQLConfig, ResolverConfig, TypeConfig,
};
use crate::directive::{
    find_directive, parse_batch_key_directive, parse_call_directive, parse_resolver_directive,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ParseError {
    #[error("Failed to parse SDL: {0}")]
    SdlParseError(String),
}

pub(crate) fn parse_sdl(sdl: &str) -> Result<GraphQLConfig, ParseError> {
    let document = async_graphql_parser::parse_schema(sdl)
        .map_err(|e| ParseError::SdlParseError(e.to_string()))?;

    build_config_from_document(document)
}

fn build_config_from_document(document: ServiceDocument) -> Result<GraphQLConfig, ParseError> {
    let mut config = GraphQLConfig::default();

    for definition in document.definitions {
        match definition {
            TypeSystemDefinition::Schema(schema_def) => {
                process_schema_definition(&schema_def.node, &mut config);
            }
            TypeSystemDefinition::Type(type_def) => {
                if let Some(type_config) = process_type_definition(&type_def.node)? {
                    config.types.insert(type_config.name.clone(), type_config);
                }
            }
            TypeSystemDefinition::Directive(_) => {}
        }
    }

    infer_root_types(&mut config);

    Ok(config)
}

fn process_schema_definition(schema_def: &SchemaDefinition, config: &mut GraphQLConfig) {
    if let Some(query) = &schema_def.query {
        config.query_type = Some(query.node.to_string());
    }
    if let Some(mutation) = &schema_def.mutation {
        config.mutation_type = Some(mutation.node.to_string());
    }
}

fn process_type_definition(type_def: &TypeDefinition) -> Result<Option<TypeConfig>, ParseError> {
    let name = type_def.name.node.to_string();

    if name.starts_with("__") {
        return Ok(None);
    }

    let fields = match &type_def.kind {
        TypeKind::Object(obj) => process_fields(&obj.fields),
        TypeKind::Interface(iface) => process_fields(&iface.fields),
        _ => return Ok(None),
    };

    Ok(Some(TypeConfig { name, fields }))
}

fn process_fields(
    fields: &[async_graphql_parser::Positioned<FieldDefinition>],
) -> Vec<FieldConfig> {
    fields.iter().map(|f| process_field(&f.node)).collect()
}

fn process_field(field: &FieldDefinition) -> FieldConfig {
    let name = field.name.node.to_string();
    let field_type = convert_type(&field.ty.node);
    let arguments = process_arguments(&field.arguments);
    let resolver = extract_resolver(&field.directives);

    FieldConfig {
        name,
        field_type,
        arguments,
        resolver,
    }
}

fn convert_type(ty: &Type) -> FieldType {
    convert_base_type(&ty.base, ty.nullable)
}

fn convert_base_type(base: &BaseType, nullable: bool) -> FieldType {
    let inner = match base {
        BaseType::Named(name) => FieldType::Named(name.to_string()),
        BaseType::List(inner) => FieldType::List(Box::new(convert_type(inner))),
    };

    if nullable {
        inner
    } else {
        FieldType::NonNull(Box::new(inner))
    }
}

fn process_arguments(
    args: &[async_graphql_parser::Positioned<async_graphql_parser::types::InputValueDefinition>],
) -> Vec<ArgumentConfig> {
    args.iter()
        .map(|a| {
            let name = a.node.name.node.to_string();
            let arg_type = convert_type(&a.node.ty.node);
            let default_value = a
                .node
                .default_value
                .as_ref()
                .map(|v| const_value_to_json(&v.node));

            ArgumentConfig {
                name,
                arg_type,
                default_value,
            }
        })
        .collect()
}

fn extract_resolver(
    directives: &[async_graphql_parser::Positioned<ConstDirective>],
) -> Option<ResolverConfig> {
    if let Some(call_dir) = find_directive(directives, "call") {
        if let Some(call) = parse_call_directive(call_dir) {
            return Some(ResolverConfig::Call {
                trait_name: call.trait_name,
                args: call.args,
            });
        }
    }

    if let Some(resolver_dir) = find_directive(directives, "resolver") {
        if let Some(resolver_d) = parse_resolver_directive(resolver_dir) {
            let batch_key = find_directive(directives, "batchKey")
                .and_then(parse_batch_key_directive)
                .map(|b| b.field);

            return Some(ResolverConfig::Trait {
                name: resolver_d.name,
                batch_key,
            });
        }
    }

    None
}

fn const_value_to_json(value: &ConstValue) -> serde_json::Value {
    match value {
        ConstValue::Null => serde_json::Value::Null,
        ConstValue::Boolean(b) => serde_json::Value::Bool(*b),
        ConstValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                serde_json::Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        }
        ConstValue::String(s) => serde_json::Value::String(s.clone()),
        ConstValue::Enum(e) => serde_json::Value::String(e.to_string()),
        ConstValue::List(arr) => {
            serde_json::Value::Array(arr.iter().map(const_value_to_json).collect())
        }
        ConstValue::Object(obj) => {
            let map: serde_json::Map<String, serde_json::Value> = obj
                .iter()
                .map(|(k, v)| (k.to_string(), const_value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        ConstValue::Binary(b) => serde_json::Value::Array(
            b.iter()
                .map(|byte| serde_json::Value::Number((*byte).into()))
                .collect(),
        ),
    }
}

fn infer_root_types(config: &mut GraphQLConfig) {
    if config.query_type.is_none() && config.types.contains_key("Query") {
        config.query_type = Some("Query".to_string());
    }
    if config.mutation_type.is_none() && config.types.contains_key("Mutation") {
        config.mutation_type = Some("Mutation".to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_schema() {
        let sdl = r#"
            type Query {
                hello: String
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        assert_eq!(config.query_type, Some("Query".to_string()));
        assert!(config.types.contains_key("Query"));
    }

    #[test]
    fn test_parse_invalid_sdl() {
        let sdl = "not valid graphql";
        let result = parse_sdl(sdl);
        assert!(result.is_err());
        match result {
            Err(ParseError::SdlParseError(msg)) => assert!(!msg.is_empty()),
            _ => panic!("Expected SdlParseError"),
        }
    }

    #[test]
    fn test_parse_with_explicit_schema() {
        let sdl = r#"
            schema {
                query: MyQuery
                mutation: MyMutation
            }

            type MyQuery {
                hello: String
            }

            type MyMutation {
                setHello(msg: String): String
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        assert_eq!(config.query_type, Some("MyQuery".to_string()));
        assert_eq!(config.mutation_type, Some("MyMutation".to_string()));
    }

    #[test]
    fn test_parse_with_mutation_type() {
        let sdl = r#"
            type Query {
                hello: String
            }

            type Mutation {
                setHello(msg: String): String
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        assert_eq!(config.query_type, Some("Query".to_string()));
        assert_eq!(config.mutation_type, Some("Mutation".to_string()));
    }

    #[test]
    fn test_parse_ignores_internal_types() {
        let sdl = r#"
            type Query {
                hello: String
            }

            type __Internal {
                data: String
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        assert!(!config.types.contains_key("__Internal"));
    }

    #[test]
    fn test_parse_interface_type() {
        let sdl = r#"
            type Query {
                node: Node
            }

            interface Node {
                id: ID!
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        assert!(config.types.contains_key("Node"));
        let node_type = config.types.get("Node").unwrap();
        assert_eq!(node_type.fields.len(), 1);
        assert_eq!(node_type.fields[0].name, "id");
    }

    #[test]
    fn test_parse_enum_type_skipped() {
        let sdl = r#"
            type Query {
                status: Status
            }

            enum Status {
                ACTIVE
                INACTIVE
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        assert!(!config.types.contains_key("Status"));
    }

    #[test]
    fn test_parse_field_with_arguments() {
        let sdl = r#"
            type Query {
                user(id: ID!, name: String): User
            }

            type User {
                id: ID!
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        let query_type = config.types.get("Query").unwrap();
        let user_field = &query_type.fields[0];
        assert_eq!(user_field.name, "user");
        assert_eq!(user_field.arguments.len(), 2);
        assert_eq!(user_field.arguments[0].name, "id");
        assert_eq!(user_field.arguments[1].name, "name");
    }

    #[test]
    fn test_parse_field_with_default_value() {
        let sdl = r#"
            type Query {
                users(limit: Int = 10): [User!]!
            }

            type User {
                id: ID!
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        let query_type = config.types.get("Query").unwrap();
        let users_field = &query_type.fields[0];
        assert_eq!(users_field.arguments[0].name, "limit");
        assert!(users_field.arguments[0].default_value.is_some());
        assert_eq!(
            users_field.arguments[0].default_value.as_ref().unwrap(),
            &serde_json::json!(10)
        );
    }

    #[test]
    fn test_parse_list_type() {
        let sdl = r#"
            type Query {
                users: [User!]!
            }

            type User {
                id: ID!
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        let query_type = config.types.get("Query").unwrap();
        let users_field = &query_type.fields[0];

        match &users_field.field_type {
            FieldType::NonNull(inner) => match inner.as_ref() {
                FieldType::List(item) => match item.as_ref() {
                    FieldType::NonNull(user_type) => match user_type.as_ref() {
                        FieldType::Named(name) => assert_eq!(name, "User"),
                        _ => panic!("Expected Named type"),
                    },
                    _ => panic!("Expected NonNull item"),
                },
                _ => panic!("Expected List type"),
            },
            _ => panic!("Expected NonNull type"),
        }
    }

    #[test]
    fn test_parse_trait_directive() {
        let sdl = r#"
            type Query {
                user(id: ID!): User @resolver(name: "getUser")
            }

            type User {
                id: ID!
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        let query_type = config.types.get("Query").unwrap();
        let user_field = &query_type.fields[0];

        match &user_field.resolver {
            Some(ResolverConfig::Trait { name, batch_key }) => {
                assert_eq!(name, "getUser");
                assert!(batch_key.is_none());
            }
            _ => panic!("Expected Trait resolver"),
        }
    }

    #[test]
    fn test_parse_trait_with_batch_key() {
        let sdl = r#"
            type Query {
                users: [User!]!
            }

            type User {
                id: ID!
                posts: [Post!]! @resolver(name: "getPosts") @batchKey(field: "userId")
            }

            type Post {
                id: ID!
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        let user_type = config.types.get("User").unwrap();
        let posts_field = &user_type.fields[1];

        match &posts_field.resolver {
            Some(ResolverConfig::Trait { name, batch_key }) => {
                assert_eq!(name, "getPosts");
                assert_eq!(batch_key.as_ref().unwrap(), "userId");
            }
            _ => panic!("Expected Trait resolver with batch_key"),
        }
    }

    #[test]
    fn test_parse_call_directive() {
        let sdl = r#"
            type Query {
                user(id: ID!): User @resolver(name: "getUser")
            }

            type User {
                id: ID!
                profile: Profile @call(resolver: "getProfile", args: { userId: "$parent.id" })
            }

            type Profile {
                bio: String
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        let user_type = config.types.get("User").unwrap();
        let profile_field = &user_type.fields[1];

        match &profile_field.resolver {
            Some(ResolverConfig::Call { trait_name, args }) => {
                assert_eq!(trait_name, "getProfile");
                assert!(args.contains_key("userId"));
            }
            _ => panic!("Expected Call resolver"),
        }
    }

    #[test]
    fn test_parse_directive_definitions_ignored() {
        let sdl = r#"
            directive @custom on FIELD_DEFINITION

            type Query {
                hello: String
            }
        "#;

        let config = parse_sdl(sdl).unwrap();
        assert!(config.types.contains_key("Query"));
    }

    #[test]
    fn test_parse_error_display() {
        let err = ParseError::SdlParseError("test error".to_string());
        assert_eq!(err.to_string(), "Failed to parse SDL: test error");
    }

    #[test]
    fn test_parse_error_debug() {
        let err = ParseError::SdlParseError("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("SdlParseError"));
    }
}
