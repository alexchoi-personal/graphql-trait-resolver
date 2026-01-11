use rustc_hash::FxHashSet;

use crate::config::{FieldConfig, GraphQLConfig};
use crate::n1::error::N1Error;
use crate::registry::storage::TraitRegistry;

pub(crate) struct N1Detector<'a> {
    config: &'a GraphQLConfig,
    registry: &'a TraitRegistry,
    errors: Vec<N1Error>,
}

impl<'a> N1Detector<'a> {
    pub fn new(config: &'a GraphQLConfig, registry: &'a TraitRegistry) -> Self {
        Self {
            config,
            registry,
            errors: Vec::new(),
        }
    }

    pub fn detect(mut self) -> Result<(), Vec<N1Error>> {
        if let Some(query_type) = &self.config.query_type {
            let mut visited = FxHashSet::default();
            self.traverse(query_type, vec![query_type.clone()], &mut visited);
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors)
        }
    }

    fn traverse(&mut self, type_name: &str, path: Vec<String>, visited: &mut FxHashSet<String>) {
        if visited.contains(type_name) {
            return;
        }
        visited.insert(type_name.to_string());

        let Some(type_config) = self.config.types.get(type_name) else {
            return;
        };

        for field in &type_config.fields {
            let mut field_path = path.clone();
            field_path.push(field.name.clone());

            self.check_field(type_name, field, &field_path);

            if let Some(inner_type) = field.field_type.inner_type_name() {
                if self.config.types.contains_key(inner_type) {
                    self.traverse(inner_type, field_path, visited);
                }
            }
        }

        visited.remove(type_name);
    }

    fn check_field(&mut self, parent_type: &str, field: &FieldConfig, path: &[String]) {
        let Some(ref resolver) = field.resolver else {
            return;
        };

        let in_list_context = self.is_in_list_context(path);
        let is_batched =
            resolver.is_batched() || self.registry.has_batch_resolver(resolver.resolver_name());

        if in_list_context && !is_batched {
            self.errors.push(N1Error {
                path: path.to_vec(),
                field_name: field.name.clone(),
                parent_type: parent_type.to_string(),
                message: format!(
                    "Field '{}' on type '{}' has a resolver in list context without batching. \
                     Add @batchKey directive or use a BatchResolver.",
                    field.name, parent_type
                ),
            });
        }
    }

    fn is_in_list_context(&self, path: &[String]) -> bool {
        if path.len() < 2 {
            return false;
        }

        let root_type = self.config.query_type.as_deref().unwrap_or("Query");
        let mut current_type = root_type.to_string();

        for (i, segment) in path.iter().enumerate().skip(1) {
            if i == path.len() - 1 {
                break;
            }

            let Some(type_config) = self.config.types.get(&current_type) else {
                return false;
            };

            let Some(field_config) = type_config.fields.iter().find(|f| &f.name == segment) else {
                return false;
            };

            if field_config.field_type.is_list() {
                return true;
            }

            if let Some(inner_type) = field_config.field_type.inner_type_name() {
                if self.config.types.contains_key(inner_type) {
                    current_type = inner_type.to_string();
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FieldType, ResolverConfig, TypeConfig};

    fn make_config_with_types(types: Vec<(&str, Vec<FieldConfig>)>) -> GraphQLConfig {
        let mut config = GraphQLConfig {
            query_type: Some("Query".to_string()),
            ..Default::default()
        };
        for (name, fields) in types {
            config.types.insert(
                name.to_string(),
                TypeConfig {
                    name: name.to_string(),
                    fields,
                },
            );
        }
        config
    }

    fn make_field(
        name: &str,
        field_type: FieldType,
        resolver: Option<ResolverConfig>,
    ) -> FieldConfig {
        FieldConfig {
            name: name.to_string(),
            field_type,
            arguments: vec![],
            resolver,
        }
    }

    #[test]
    fn test_detector_no_query_type() {
        let config = GraphQLConfig::default();
        let registry = TraitRegistry::default();

        let detector = N1Detector::new(&config, &registry);
        let result = detector.detect();
        assert!(result.is_ok());
    }

    #[test]
    fn test_detector_no_errors_without_resolvers() {
        let config = make_config_with_types(vec![(
            "Query",
            vec![make_field(
                "hello",
                FieldType::Named("String".to_string()),
                None,
            )],
        )]);
        let registry = TraitRegistry::default();

        let detector = N1Detector::new(&config, &registry);
        let result = detector.detect();
        assert!(result.is_ok());
    }

    #[test]
    fn test_detector_single_resolver_no_list() {
        let config = make_config_with_types(vec![(
            "Query",
            vec![make_field(
                "user",
                FieldType::Named("User".to_string()),
                Some(ResolverConfig::Trait {
                    name: "getUser".to_string(),
                    batch_key: None,
                }),
            )],
        )]);
        let registry = TraitRegistry::default();

        let detector = N1Detector::new(&config, &registry);
        let result = detector.detect();
        assert!(result.is_ok());
    }

    #[test]
    fn test_detector_list_with_batched_resolver() {
        let config = make_config_with_types(vec![
            (
                "Query",
                vec![make_field(
                    "users",
                    FieldType::NonNull(Box::new(FieldType::List(Box::new(FieldType::Named(
                        "User".to_string(),
                    ))))),
                    None,
                )],
            ),
            (
                "User",
                vec![
                    make_field("id", FieldType::Named("ID".to_string()), None),
                    make_field(
                        "posts",
                        FieldType::List(Box::new(FieldType::Named("Post".to_string()))),
                        Some(ResolverConfig::Trait {
                            name: "getPosts".to_string(),
                            batch_key: Some("userId".to_string()),
                        }),
                    ),
                ],
            ),
        ]);
        let registry = TraitRegistry::default();

        let detector = N1Detector::new(&config, &registry);
        let result = detector.detect();
        assert!(result.is_ok());
    }

    #[test]
    fn test_detector_list_without_batch_key_error() {
        let config = make_config_with_types(vec![
            (
                "Query",
                vec![make_field(
                    "users",
                    FieldType::List(Box::new(FieldType::Named("User".to_string()))),
                    None,
                )],
            ),
            (
                "User",
                vec![
                    make_field("id", FieldType::Named("ID".to_string()), None),
                    make_field(
                        "posts",
                        FieldType::List(Box::new(FieldType::Named("Post".to_string()))),
                        Some(ResolverConfig::Trait {
                            name: "getPosts".to_string(),
                            batch_key: None,
                        }),
                    ),
                ],
            ),
        ]);
        let registry = TraitRegistry::default();

        let detector = N1Detector::new(&config, &registry);
        let result = detector.detect();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field_name, "posts");
    }

    #[test]
    fn test_detector_with_batch_resolver_in_registry() {
        use crate::registry::resolver::{BoxFuture, ResolverContext, ResolverResult};
        use crate::registry::storage::ErasedBatchResolver;

        struct TestBatchResolver;

        impl ErasedBatchResolver for TestBatchResolver {
            fn name(&self) -> &'static str {
                "getPosts"
            }

            fn batch_key_field(&self) -> &'static str {
                "userId"
            }

            fn load_erased<'a>(
                &'a self,
                _ctx: &'a ResolverContext,
                keys: Vec<serde_json::Value>,
            ) -> BoxFuture<'a, ResolverResult<Vec<(serde_json::Value, serde_json::Value)>>>
            {
                Box::pin(async move { Ok(keys.into_iter().map(|k| (k.clone(), k)).collect()) })
            }
        }

        let config = make_config_with_types(vec![
            (
                "Query",
                vec![make_field(
                    "users",
                    FieldType::List(Box::new(FieldType::Named("User".to_string()))),
                    None,
                )],
            ),
            (
                "User",
                vec![
                    make_field("id", FieldType::Named("ID".to_string()), None),
                    make_field(
                        "posts",
                        FieldType::List(Box::new(FieldType::Named("Post".to_string()))),
                        Some(ResolverConfig::Trait {
                            name: "getPosts".to_string(),
                            batch_key: None,
                        }),
                    ),
                ],
            ),
        ]);

        let mut registry = TraitRegistry::default();
        registry.register_batch_resolver(TestBatchResolver);

        let detector = N1Detector::new(&config, &registry);
        let result = detector.detect();
        assert!(result.is_ok());
    }

    #[test]
    fn test_detector_deeply_nested_list() {
        let config = make_config_with_types(vec![
            (
                "Query",
                vec![make_field(
                    "orgs",
                    FieldType::List(Box::new(FieldType::Named("Org".to_string()))),
                    None,
                )],
            ),
            (
                "Org",
                vec![make_field(
                    "users",
                    FieldType::List(Box::new(FieldType::Named("User".to_string()))),
                    None,
                )],
            ),
            (
                "User",
                vec![make_field(
                    "posts",
                    FieldType::List(Box::new(FieldType::Named("Post".to_string()))),
                    Some(ResolverConfig::Trait {
                        name: "getPosts".to_string(),
                        batch_key: None,
                    }),
                )],
            ),
        ]);
        let registry = TraitRegistry::default();

        let detector = N1Detector::new(&config, &registry);
        let result = detector.detect();
        assert!(result.is_err());
    }

    #[test]
    fn test_detector_unknown_field_type() {
        let config = make_config_with_types(vec![(
            "Query",
            vec![make_field(
                "data",
                FieldType::Named("UnknownType".to_string()),
                None,
            )],
        )]);
        let registry = TraitRegistry::default();

        let detector = N1Detector::new(&config, &registry);
        let result = detector.detect();
        assert!(result.is_ok());
    }

    #[test]
    fn test_detector_call_resolver() {
        let config = make_config_with_types(vec![
            (
                "Query",
                vec![make_field(
                    "users",
                    FieldType::List(Box::new(FieldType::Named("User".to_string()))),
                    None,
                )],
            ),
            (
                "User",
                vec![
                    make_field("id", FieldType::Named("ID".to_string()), None),
                    make_field(
                        "profile",
                        FieldType::Named("Profile".to_string()),
                        Some(ResolverConfig::Call {
                            trait_name: "getProfile".to_string(),
                            args: rustc_hash::FxHashMap::default(),
                        }),
                    ),
                ],
            ),
        ]);
        let registry = TraitRegistry::default();

        let detector = N1Detector::new(&config, &registry);
        let result = detector.detect();
        assert!(result.is_err());
    }
}
