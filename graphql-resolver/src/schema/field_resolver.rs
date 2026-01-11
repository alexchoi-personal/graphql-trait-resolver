use rustc_hash::FxHashMap;
use std::sync::Arc;

use async_graphql::dynamic::{Field, FieldFuture, FieldValue, TypeRef};
use async_graphql::Value;

use crate::config::{ArgumentMapping, FieldConfig, ResolverConfig};
use crate::error::ResolverError;
use crate::registry::resolver::ResolverContext;
use crate::registry::storage::TraitRegistry;

pub(crate) fn value_to_field_value(value: Value) -> FieldValue<'static> {
    match value {
        Value::List(items) => FieldValue::list(items.into_iter().map(value_to_field_value)),
        Value::Object(_) => FieldValue::owned_any(value),
        scalar => FieldValue::from(scalar),
    }
}

pub(crate) struct FieldResolverFactory {
    parent_type: String,
    field_config: FieldConfig,
    registry: Arc<TraitRegistry>,
}

impl FieldResolverFactory {
    pub fn new(
        parent_type: String,
        field_config: FieldConfig,
        registry: Arc<TraitRegistry>,
    ) -> Self {
        Self {
            parent_type,
            field_config,
            registry,
        }
    }

    pub fn create_field(self, type_ref: TypeRef) -> Result<Field, ResolverError> {
        let resolver_config = self
            .field_config
            .resolver
            .clone()
            .ok_or_else(|| ResolverError::Execution("No resolver configured".to_string()))?;

        let field_name = self.field_config.name.clone();
        let parent_type = self.parent_type.clone();
        let registry = self.registry.clone();

        match resolver_config {
            ResolverConfig::Trait { name, batch_key } => self.create_trait_field(
                type_ref,
                name,
                batch_key,
                field_name,
                parent_type,
                registry,
            ),
            ResolverConfig::Call { trait_name, args } => self.create_call_field(
                type_ref,
                trait_name,
                args,
                field_name,
                parent_type,
                registry,
            ),
        }
    }

    fn create_trait_field(
        self,
        type_ref: TypeRef,
        resolver_name: String,
        batch_key: Option<String>,
        field_name: String,
        parent_type: String,
        registry: Arc<TraitRegistry>,
    ) -> Result<Field, ResolverError> {
        let field_name_for_new = field_name.clone();
        let mut field = Field::new(field_name_for_new, type_ref, move |ctx| {
            let resolver_name = resolver_name.clone();
            let field_name = field_name.clone();
            let parent_type = parent_type.clone();
            let registry = registry.clone();
            let batch_key = batch_key.clone();

            FieldFuture::new(async move {
                let parent = ctx
                    .parent_value
                    .try_downcast_ref::<Value>()
                    .cloned()
                    .unwrap_or(Value::Null);

                if let Some(ref key_field) = batch_key {
                    let batch_resolver = registry.get_batch_resolver(&resolver_name)?;

                    let key_value = if let Value::Object(obj) = &parent {
                        obj.get(key_field.as_str())
                            .cloned()
                            .map(|v| serde_json::to_value(&v).unwrap_or_default())
                            .unwrap_or(serde_json::Value::Null)
                    } else {
                        serde_json::Value::Null
                    };

                    let resolver_ctx = ResolverContext::new(field_name.clone())
                        .with_parent(parent)
                        .with_path(vec![parent_type, field_name]);

                    let results = batch_resolver
                        .load_erased(&resolver_ctx, vec![key_value.clone()])
                        .await?;

                    let result = results
                        .into_iter()
                        .find(|(k, _)| k == &key_value)
                        .map(|(_, v)| v);

                    match result {
                        Some(json_val) => {
                            let gql_val: Value =
                                serde_json::from_value(json_val).unwrap_or(Value::Null);
                            Ok(Some(value_to_field_value(gql_val)))
                        }
                        None => Ok(None),
                    }
                } else {
                    let resolver = registry.get_resolver(&resolver_name)?;

                    let mut args = FxHashMap::default();
                    for (name, value) in ctx.args.iter() {
                        if let Ok(gql_value) = value.deserialize::<Value>() {
                            args.insert(name.to_string(), gql_value);
                        }
                    }

                    let resolver_ctx = ResolverContext::new(field_name.clone())
                        .with_parent(parent)
                        .with_path(vec![parent_type, field_name]);

                    let result = resolver.resolve(&resolver_ctx, args).await?;
                    Ok(Some(value_to_field_value(result)))
                }
            })
        });

        for arg in &self.field_config.arguments {
            let arg_type = super::builder::convert_field_type(&arg.arg_type);
            field = field.argument(async_graphql::dynamic::InputValue::new(&arg.name, arg_type));
        }

        Ok(field)
    }

    fn create_call_field(
        self,
        type_ref: TypeRef,
        trait_name: String,
        arg_mappings: FxHashMap<String, ArgumentMapping>,
        field_name: String,
        parent_type: String,
        registry: Arc<TraitRegistry>,
    ) -> Result<Field, ResolverError> {
        let field_name_for_new = field_name.clone();
        let mut field = Field::new(field_name_for_new, type_ref, move |ctx| {
            let trait_name = trait_name.clone();
            let arg_mappings = arg_mappings.clone();
            let field_name = field_name.clone();
            let parent_type = parent_type.clone();
            let registry = registry.clone();

            FieldFuture::new(async move {
                let resolver = registry.get_resolver(&trait_name)?;

                let parent = ctx
                    .parent_value
                    .try_downcast_ref::<Value>()
                    .cloned()
                    .unwrap_or(Value::Null);

                let mut args = FxHashMap::default();

                for (arg_name, mapping) in &arg_mappings {
                    let value = match mapping {
                        ArgumentMapping::ParentField(field) => {
                            if let Value::Object(obj) = &parent {
                                obj.get(field.as_str()).cloned().unwrap_or(Value::Null)
                            } else {
                                Value::Null
                            }
                        }
                        ArgumentMapping::Argument(name) => ctx
                            .args
                            .get(name)
                            .and_then(|v| v.deserialize::<Value>().ok())
                            .unwrap_or(Value::Null),
                        ArgumentMapping::Literal(json_val) => {
                            serde_json::from_value(json_val.clone()).unwrap_or(Value::Null)
                        }
                    };
                    args.insert(arg_name.clone(), value);
                }

                let resolver_ctx = ResolverContext::new(field_name.clone())
                    .with_parent(parent)
                    .with_path(vec![parent_type, field_name]);

                let result = resolver.resolve(&resolver_ctx, args).await?;
                Ok(Some(value_to_field_value(result)))
            })
        });

        for arg in &self.field_config.arguments {
            let arg_type = super::builder::convert_field_type(&arg.arg_type);
            field = field.argument(async_graphql::dynamic::InputValue::new(&arg.name, arg_type));
        }

        Ok(field)
    }
}
