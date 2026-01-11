use std::sync::Arc;

use async_graphql::dynamic::{Field, FieldFuture, Object, Schema, TypeRef};
use async_graphql::Value;

use crate::config::{FieldConfig, FieldType, GraphQLConfig, TypeConfig};
use crate::error::ResolverError;
use crate::registry::storage::TraitRegistry;
use crate::schema::field_resolver::{value_to_field_value, FieldResolverFactory};

pub(crate) struct SchemaBuilder {
    config: GraphQLConfig,
    registry: Arc<TraitRegistry>,
}

impl SchemaBuilder {
    pub fn new(config: GraphQLConfig, registry: Arc<TraitRegistry>) -> Self {
        Self { config, registry }
    }

    pub fn build(self) -> Result<Schema, ResolverError> {
        let query_type_name = self
            .config
            .query_type
            .clone()
            .unwrap_or_else(|| "Query".to_string());

        let mut schema_builder = Schema::build(&query_type_name, None, None);

        for (type_name, type_config) in &self.config.types {
            let object = self.build_object_type(type_name, type_config)?;
            schema_builder = schema_builder.register(object);
        }

        schema_builder
            .finish()
            .map_err(|e| ResolverError::Execution(e.to_string()))
    }

    fn build_object_type(
        &self,
        type_name: &str,
        type_config: &TypeConfig,
    ) -> Result<Object, ResolverError> {
        let mut object = Object::new(type_name);

        for field_config in &type_config.fields {
            let field = self.build_field(type_name, field_config)?;
            object = object.field(field);
        }

        Ok(object)
    }

    fn build_field(
        &self,
        parent_type: &str,
        field_config: &FieldConfig,
    ) -> Result<Field, ResolverError> {
        let field_name = field_config.name.clone();
        let type_ref = convert_field_type(&field_config.field_type);

        if field_config.resolver.is_some() {
            let factory = FieldResolverFactory::new(
                parent_type.to_string(),
                field_config.clone(),
                self.registry.clone(),
            );
            return factory.create_field(type_ref);
        }

        let field_name_clone = field_name.clone();
        let mut field = Field::new(&field_name, type_ref, move |ctx| {
            let field_name = field_name_clone.clone();
            FieldFuture::new(async move {
                if let Ok(Value::Object(obj)) = ctx.parent_value.try_downcast_ref::<Value>() {
                    if let Some(value) = obj.get(field_name.as_str()) {
                        return Ok(Some(value_to_field_value(value.clone())));
                    }
                }
                Ok(None)
            })
        });

        for arg in &field_config.arguments {
            let arg_type = convert_field_type(&arg.arg_type);
            field = field.argument(async_graphql::dynamic::InputValue::new(&arg.name, arg_type));
        }

        Ok(field)
    }
}

pub(crate) fn convert_field_type(field_type: &FieldType) -> TypeRef {
    match field_type {
        FieldType::Named(name) => TypeRef::named(name),
        FieldType::List(inner) => TypeRef::List(Box::new(convert_field_type(inner))),
        FieldType::NonNull(inner) => TypeRef::NonNull(Box::new(convert_field_type(inner))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_field_type_named() {
        let ft = FieldType::Named("String".to_string());
        let tr = convert_field_type(&ft);
        assert_eq!(format!("{:?}", tr), "Named(\"String\")");
    }

    #[test]
    fn test_convert_field_type_list() {
        let ft = FieldType::List(Box::new(FieldType::Named("Int".to_string())));
        let tr = convert_field_type(&ft);
        let debug = format!("{:?}", tr);
        assert!(debug.contains("List"));
        assert!(debug.contains("Int"));
    }

    #[test]
    fn test_convert_field_type_nonnull() {
        let ft = FieldType::NonNull(Box::new(FieldType::Named("ID".to_string())));
        let tr = convert_field_type(&ft);
        let debug = format!("{:?}", tr);
        assert!(debug.contains("NonNull"));
        assert!(debug.contains("ID"));
    }

    #[test]
    fn test_convert_field_type_complex() {
        let ft = FieldType::NonNull(Box::new(FieldType::List(Box::new(FieldType::NonNull(
            Box::new(FieldType::Named("User".to_string())),
        )))));
        let tr = convert_field_type(&ft);
        let debug = format!("{:?}", tr);
        assert!(debug.contains("NonNull"));
        assert!(debug.contains("List"));
        assert!(debug.contains("User"));
    }

    #[test]
    fn test_schema_builder_new() {
        let config = GraphQLConfig::default();
        let registry = Arc::new(TraitRegistry::default());
        let builder = SchemaBuilder::new(config, registry);
        let _ = builder;
    }

    #[test]
    fn test_schema_builder_simple_query() {
        use crate::config::TypeConfig;

        let mut config = GraphQLConfig {
            query_type: Some("Query".to_string()),
            ..Default::default()
        };
        config.types.insert(
            "Query".to_string(),
            TypeConfig {
                name: "Query".to_string(),
                fields: vec![FieldConfig {
                    name: "hello".to_string(),
                    field_type: FieldType::Named("String".to_string()),
                    arguments: vec![],
                    resolver: None,
                }],
            },
        );

        let registry = Arc::new(TraitRegistry::default());
        let builder = SchemaBuilder::new(config, registry);
        let result = builder.build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_schema_builder_with_arguments() {
        use crate::config::{ArgumentConfig, TypeConfig};

        let mut config = GraphQLConfig {
            query_type: Some("Query".to_string()),
            ..Default::default()
        };
        config.types.insert(
            "Query".to_string(),
            TypeConfig {
                name: "Query".to_string(),
                fields: vec![FieldConfig {
                    name: "user".to_string(),
                    field_type: FieldType::Named("User".to_string()),
                    arguments: vec![ArgumentConfig {
                        name: "id".to_string(),
                        arg_type: FieldType::NonNull(Box::new(FieldType::Named("ID".to_string()))),
                        default_value: None,
                    }],
                    resolver: None,
                }],
            },
        );
        config.types.insert(
            "User".to_string(),
            TypeConfig {
                name: "User".to_string(),
                fields: vec![FieldConfig {
                    name: "id".to_string(),
                    field_type: FieldType::Named("ID".to_string()),
                    arguments: vec![],
                    resolver: None,
                }],
            },
        );

        let registry = Arc::new(TraitRegistry::default());
        let builder = SchemaBuilder::new(config, registry);
        let result = builder.build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_schema_builder_default_query_type() {
        use crate::config::TypeConfig;

        let mut config = GraphQLConfig::default();
        config.types.insert(
            "Query".to_string(),
            TypeConfig {
                name: "Query".to_string(),
                fields: vec![FieldConfig {
                    name: "hello".to_string(),
                    field_type: FieldType::Named("String".to_string()),
                    arguments: vec![],
                    resolver: None,
                }],
            },
        );

        let registry = Arc::new(TraitRegistry::default());
        let builder = SchemaBuilder::new(config, registry);
        let result = builder.build();
        assert!(result.is_ok());
    }
}
