use async_graphql::Value;
use rustc_hash::FxHashMap;
use std::future::Future;
use std::pin::Pin;

use crate::error::ResolverError;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub type ResolverResult<T> = Result<T, ResolverError>;

pub struct ResolverContext {
    pub(crate) parent_value: Option<Value>,
    pub(crate) field_name: String,
    pub(crate) path: Vec<String>,
}

impl ResolverContext {
    pub fn new(field_name: String) -> Self {
        Self {
            parent_value: None,
            field_name,
            path: Vec::new(),
        }
    }

    pub fn with_parent(mut self, parent_value: Value) -> Self {
        self.parent_value = Some(parent_value);
        self
    }

    pub fn with_path(mut self, path: Vec<String>) -> Self {
        self.path = path;
        self
    }

    pub fn parent_value(&self) -> Option<&Value> {
        self.parent_value.as_ref()
    }

    pub fn field_name(&self) -> &str {
        &self.field_name
    }

    pub fn path(&self) -> &[String] {
        &self.path
    }
}

pub trait Resolver: Send + Sync + 'static {
    fn resolve<'a>(
        &'a self,
        ctx: &'a ResolverContext,
        args: FxHashMap<String, Value>,
    ) -> BoxFuture<'a, ResolverResult<Value>>;

    fn name(&self) -> &'static str;
}

#[allow(clippy::type_complexity)]
pub trait BatchResolver: Send + Sync + 'static {
    type Key: Clone + Eq + std::hash::Hash + Send + Sync + 'static;
    type Value: Clone + Send + Sync + 'static;

    fn load<'a>(
        &'a self,
        ctx: &'a ResolverContext,
        keys: Vec<Self::Key>,
    ) -> BoxFuture<'a, ResolverResult<Vec<(Self::Key, Self::Value)>>>;

    fn name(&self) -> &'static str;
    fn batch_key_field(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_context_new() {
        let ctx = ResolverContext::new("testField".to_string());
        assert_eq!(ctx.field_name(), "testField");
        assert!(ctx.parent_value().is_none());
        assert!(ctx.path().is_empty());
    }

    #[test]
    fn test_resolver_context_with_parent() {
        let ctx = ResolverContext::new("field".to_string())
            .with_parent(Value::String("parent_data".to_string()));

        assert!(ctx.parent_value().is_some());
        assert_eq!(
            ctx.parent_value().unwrap(),
            &Value::String("parent_data".to_string())
        );
    }

    #[test]
    fn test_resolver_context_with_path() {
        let ctx = ResolverContext::new("field".to_string())
            .with_path(vec!["Query".to_string(), "user".to_string()]);

        assert_eq!(ctx.path(), &["Query".to_string(), "user".to_string()]);
    }

    #[test]
    fn test_resolver_context_builder_chain() {
        let ctx = ResolverContext::new("myField".to_string())
            .with_parent(Value::Number(42.into()))
            .with_path(vec!["A".to_string(), "B".to_string()]);

        assert_eq!(ctx.field_name(), "myField");
        assert_eq!(ctx.parent_value().unwrap(), &Value::Number(42.into()));
        assert_eq!(ctx.path().len(), 2);
    }
}
