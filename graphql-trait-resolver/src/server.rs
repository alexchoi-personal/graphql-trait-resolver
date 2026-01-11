use std::sync::Arc;
use std::time::Duration;

use async_graphql::dynamic::Schema;

use crate::config::{parse_sdl, GraphQLConfig};
use crate::error::ResolverError;
use crate::n1::{N1Detector, N1Error};
use crate::registry::resolver::Resolver;
use crate::registry::storage::{ErasedBatchResolver, TraitRegistry};
use crate::schema::SchemaBuilder;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Resolver error: {0}")]
    Resolver(#[from] ResolverError),
    #[error("N+1 query detected")]
    N1Detection(Vec<N1Error>),
    #[error("Configuration error: {0}")]
    Config(String),
}

pub struct GraphQLServerBuilder {
    sdl_parts: Vec<String>,
    registry: TraitRegistry,
    batch_delay: Duration,
    max_batch_size: usize,
    validate_n1: bool,
}

impl Default for GraphQLServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphQLServerBuilder {
    pub fn new() -> Self {
        Self {
            sdl_parts: Vec::new(),
            registry: TraitRegistry::new(),
            batch_delay: Duration::from_millis(1),
            max_batch_size: 100,
            validate_n1: true,
        }
    }

    pub fn sdl(mut self, sdl: &str) -> Self {
        self.sdl_parts.push(sdl.to_string());
        self
    }

    pub fn register_resolver<R: Resolver>(mut self, resolver: R) -> Self {
        self.registry.register_resolver(resolver);
        self
    }

    pub fn register_batch_resolver<R: ErasedBatchResolver + 'static>(mut self, resolver: R) -> Self {
        self.registry.register_batch_resolver(resolver);
        self
    }

    pub fn batch_delay(mut self, delay: Duration) -> Self {
        self.batch_delay = delay;
        self
    }

    pub fn max_batch_size(mut self, size: usize) -> Self {
        self.max_batch_size = size;
        self
    }

    pub fn skip_n1_validation(mut self) -> Self {
        self.validate_n1 = false;
        self
    }

    pub fn validate(self) -> Result<ValidatedServerBuilder, ServerError> {
        if self.sdl_parts.is_empty() {
            return Err(ServerError::Config("SDL not provided".to_string()));
        }

        let sdl = self.sdl_parts.join("\n");
        let config = parse_sdl(&sdl).map_err(|e| ServerError::Parse(e.to_string()))?;

        if self.validate_n1 {
            let detector = N1Detector::new(&config, &self.registry);
            detector.detect().map_err(ServerError::N1Detection)?;
        }

        Ok(ValidatedServerBuilder {
            config,
            registry: self.registry,
            batch_delay: self.batch_delay,
            max_batch_size: self.max_batch_size,
        })
    }

    pub fn build(self) -> Result<GraphQLServer, ServerError> {
        self.validate()?.build()
    }
}

pub struct ValidatedServerBuilder {
    config: GraphQLConfig,
    registry: TraitRegistry,
    batch_delay: Duration,
    max_batch_size: usize,
}

impl ValidatedServerBuilder {
    pub fn build(self) -> Result<GraphQLServer, ServerError> {
        let registry = Arc::new(self.registry);
        let schema_builder = SchemaBuilder::new(self.config, registry.clone());
        let schema = schema_builder.build()?;

        Ok(GraphQLServer {
            schema,
            registry,
            batch_delay: self.batch_delay,
            max_batch_size: self.max_batch_size,
        })
    }
}

pub struct GraphQLServer {
    schema: Schema,
    registry: Arc<TraitRegistry>,
    batch_delay: Duration,
    max_batch_size: usize,
}

impl GraphQLServer {
    pub fn builder() -> GraphQLServerBuilder {
        GraphQLServerBuilder::new()
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn registry(&self) -> &TraitRegistry {
        &self.registry
    }

    pub fn batch_delay(&self) -> Duration {
        self.batch_delay
    }

    pub fn max_batch_size(&self) -> usize {
        self.max_batch_size
    }

    pub async fn execute(&self, query: &str) -> async_graphql::Response {
        self.schema.execute(query).await
    }

    pub fn execute_sync(&self, query: &str) -> async_graphql::Response {
        futures::executor::block_on(self.execute(query))
    }
}
