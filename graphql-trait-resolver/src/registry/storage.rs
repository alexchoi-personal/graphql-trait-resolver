use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::error::ResolverError;
use crate::registry::resolver::{BoxFuture, Resolver, ResolverContext, ResolverResult};

pub trait ErasedBatchResolver: Send + Sync {
    fn name(&self) -> &'static str;
    fn batch_key_field(&self) -> &'static str;
    fn load_erased<'a>(
        &'a self,
        ctx: &'a ResolverContext,
        keys: Vec<serde_json::Value>,
    ) -> BoxFuture<'a, ResolverResult<Vec<(serde_json::Value, serde_json::Value)>>>;
}

pub struct ResolverRegistration {
    pub(crate) factory: fn() -> Box<dyn Resolver>,
    pub(crate) name: &'static str,
}

impl ResolverRegistration {
    pub const fn new(factory: fn() -> Box<dyn Resolver>, name: &'static str) -> Self {
        Self { factory, name }
    }
}

inventory::collect!(ResolverRegistration);

pub struct BatchResolverRegistration {
    pub(crate) factory: fn() -> Box<dyn ErasedBatchResolver>,
    pub(crate) name: &'static str,
    pub(crate) batch_key: &'static str,
}

impl BatchResolverRegistration {
    pub const fn new(
        factory: fn() -> Box<dyn ErasedBatchResolver>,
        name: &'static str,
        batch_key: &'static str,
    ) -> Self {
        Self {
            factory,
            name,
            batch_key,
        }
    }
}

inventory::collect!(BatchResolverRegistration);

pub struct TraitRegistry {
    resolvers: FxHashMap<String, Arc<dyn Resolver>>,
    batch_resolvers: FxHashMap<String, Arc<dyn ErasedBatchResolver>>,
}

impl TraitRegistry {
    pub(crate) fn new() -> Self {
        Self {
            resolvers: FxHashMap::default(),
            batch_resolvers: FxHashMap::default(),
        }
    }

    pub(crate) fn from_inventory() -> Self {
        let mut registry = Self::new();

        for registration in inventory::iter::<ResolverRegistration> {
            let resolver = (registration.factory)();
            registry.resolvers.insert(registration.name.to_string(), Arc::from(resolver));
        }

        for registration in inventory::iter::<BatchResolverRegistration> {
            let resolver = (registration.factory)();
            registry.batch_resolvers.insert(registration.name.to_string(), Arc::from(resolver));
        }

        registry
    }

    pub fn register_resolver<R: Resolver>(&mut self, resolver: R) {
        let name = resolver.name().to_string();
        self.resolvers.insert(name, Arc::new(resolver));
    }

    pub fn register_batch_resolver<R: ErasedBatchResolver + 'static>(&mut self, resolver: R) {
        let name = resolver.name().to_string();
        self.batch_resolvers.insert(name, Arc::new(resolver));
    }

    pub fn get_resolver(&self, name: &str) -> ResolverResult<Arc<dyn Resolver>> {
        self.resolvers
            .get(name)
            .cloned()
            .ok_or_else(|| ResolverError::NotFound(name.to_string()))
    }

    pub fn get_batch_resolver(&self, name: &str) -> ResolverResult<Arc<dyn ErasedBatchResolver>> {
        self.batch_resolvers
            .get(name)
            .cloned()
            .ok_or_else(|| ResolverError::NotFound(name.to_string()))
    }

    pub(crate) fn has_resolver(&self, name: &str) -> bool {
        self.resolvers.contains_key(name)
    }

    pub(crate) fn has_batch_resolver(&self, name: &str) -> bool {
        self.batch_resolvers.contains_key(name)
    }

    pub(crate) fn resolver_names(&self) -> impl Iterator<Item = &String> {
        self.resolvers.keys()
    }

    pub(crate) fn batch_resolver_names(&self) -> impl Iterator<Item = &String> {
        self.batch_resolvers.keys()
    }
}

impl Default for TraitRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::Value;

    struct TestResolver;

    impl crate::registry::resolver::Resolver for TestResolver {
        fn resolve<'a>(
            &'a self,
            _ctx: &'a ResolverContext,
            _args: FxHashMap<String, Value>,
        ) -> BoxFuture<'a, ResolverResult<Value>> {
            Box::pin(async { Ok(Value::Null) })
        }

        fn name(&self) -> &'static str {
            "testResolver"
        }
    }

    struct TestBatchResolver;

    impl ErasedBatchResolver for TestBatchResolver {
        fn name(&self) -> &'static str {
            "testBatchResolver"
        }

        fn batch_key_field(&self) -> &'static str {
            "id"
        }

        fn load_erased<'a>(
            &'a self,
            _ctx: &'a ResolverContext,
            keys: Vec<serde_json::Value>,
        ) -> BoxFuture<'a, ResolverResult<Vec<(serde_json::Value, serde_json::Value)>>> {
            Box::pin(async move {
                Ok(keys.into_iter().map(|k| (k.clone(), k)).collect())
            })
        }
    }

    #[test]
    fn test_registry_new() {
        let registry = TraitRegistry::new();
        assert!(!registry.has_resolver("nonexistent"));
        assert!(!registry.has_batch_resolver("nonexistent"));
    }

    #[test]
    fn test_registry_default() {
        let registry = TraitRegistry::default();
        assert_eq!(registry.resolver_names().count(), 0);
    }

    #[test]
    fn test_register_and_get_resolver() {
        let mut registry = TraitRegistry::new();
        registry.register_resolver(TestResolver);

        assert!(registry.has_resolver("testResolver"));
        assert!(!registry.has_resolver("other"));

        let resolver = registry.get_resolver("testResolver");
        assert!(resolver.is_ok());
        assert_eq!(resolver.unwrap().name(), "testResolver");
    }

    #[test]
    fn test_get_resolver_not_found() {
        let registry = TraitRegistry::new();
        let result = registry.get_resolver("nonexistent");
        assert!(result.is_err());
        match result.err().unwrap() {
            ResolverError::NotFound(name) => assert_eq!(name, "nonexistent"),
            _ => panic!("Expected NotFound error"),
        }
    }

    #[test]
    fn test_register_and_get_batch_resolver() {
        let mut registry = TraitRegistry::new();
        registry.register_batch_resolver(TestBatchResolver);

        assert!(registry.has_batch_resolver("testBatchResolver"));
        assert!(!registry.has_batch_resolver("other"));

        let resolver = registry.get_batch_resolver("testBatchResolver");
        assert!(resolver.is_ok());
        let resolver = resolver.unwrap();
        assert_eq!(resolver.name(), "testBatchResolver");
        assert_eq!(resolver.batch_key_field(), "id");
    }

    #[test]
    fn test_get_batch_resolver_not_found() {
        let registry = TraitRegistry::new();
        let result = registry.get_batch_resolver("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolver_names() {
        let mut registry = TraitRegistry::new();
        registry.register_resolver(TestResolver);

        let names: Vec<_> = registry.resolver_names().collect();
        assert_eq!(names.len(), 1);
        assert!(names.contains(&&"testResolver".to_string()));
    }

    #[test]
    fn test_batch_resolver_names() {
        let mut registry = TraitRegistry::new();
        registry.register_batch_resolver(TestBatchResolver);

        let names: Vec<_> = registry.batch_resolver_names().collect();
        assert_eq!(names.len(), 1);
        assert!(names.contains(&&"testBatchResolver".to_string()));
    }

    #[test]
    fn test_resolver_registration_new() {
        fn factory() -> Box<dyn crate::registry::resolver::Resolver> {
            Box::new(TestResolver)
        }

        let reg = ResolverRegistration::new(factory, "test");
        assert_eq!(reg.name, "test");
    }

    #[test]
    fn test_batch_resolver_registration_new() {
        fn factory() -> Box<dyn ErasedBatchResolver> {
            Box::new(TestBatchResolver)
        }

        let reg = BatchResolverRegistration::new(factory, "test", "id");
        assert_eq!(reg.name, "test");
        assert_eq!(reg.batch_key, "id");
    }

    #[test]
    fn test_from_inventory() {
        let registry = TraitRegistry::from_inventory();
        let _ = registry.resolver_names().count();
    }
}
