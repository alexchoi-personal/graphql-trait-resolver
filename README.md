# graphql-resolver

A Rust library for building GraphQL servers with trait-based resolvers. Define your schema in SDL, wire up resolvers with simple directives, and get automatic N+1 detection out of the box.

Built on top of [async-graphql](https://github.com/async-graphql/async-graphql).

## Features

- **SDL-first schema design** - Write your GraphQL schema in SDL with custom directives to wire up resolvers
- **Trait-based resolvers** - Implement simple traits to define your query logic
- **Batch resolvers** - Built-in support for batching to solve the N+1 problem (like DataLoader)
- **N+1 detection** - Automatically validates your schema for potential N+1 issues at build time
- **Framework agnostic** - Works with Axum, Actix, or any async runtime

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
graphql-resolver = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

Define your schema and resolvers:

```rust
use graphql_resolver::{
    BoxFuture, FxHashMap, GraphQLServer, Resolver, ResolverContext, ResolverResult,
};
use async_graphql::Value;

struct GetUserResolver;

impl Resolver for GetUserResolver {
    fn name(&self) -> &'static str {
        "getUser"
    }

    fn resolve<'a>(
        &'a self,
        _ctx: &'a ResolverContext,
        args: FxHashMap<String, Value>,
    ) -> BoxFuture<'a, ResolverResult<Value>> {
        Box::pin(async move {
            let id = args.get("id").and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                _ => None,
            }).unwrap_or_default();

            Ok(serde_json::json!({
                "id": id,
                "name": format!("User {}", id),
            }).into())
        })
    }
}

const SCHEMA: &str = r#"
    type Query {
        user(id: ID!): User @resolver(name: "getUser")
    }

    type User {
        id: ID!
        name: String!
    }
"#;

#[tokio::main]
async fn main() {
    let server = GraphQLServer::builder()
        .sdl(SCHEMA)
        .register_resolver(GetUserResolver)
        .build()
        .expect("Failed to build server");

    let response = server.execute(r#"{ user(id: "1") { id name } }"#).await;
    println!("{:?}", response.data);
}
```

## Directives

### `@resolver`

Maps a field to a resolver by name:

```graphql
type Query {
    user(id: ID!): User @resolver(name: "getUser")
    users: [User!]! @resolver(name: "listUsers")
}
```

### `@batchKey`

Enables batching for nested fields to prevent N+1 queries:

```graphql
type User {
    id: ID!
    posts: [Post!]! @resolver(name: "getPostsByUser") @batchKey(field: "id")
}
```

When querying multiple users, the `getPostsByUser` resolver receives all user IDs at once instead of being called once per user.

## Batch Resolvers

Implement `ErasedBatchResolver` for efficient data loading:

```rust
use graphql_resolver::{BoxFuture, ErasedBatchResolver, ResolverContext, ResolverResult};

struct GetPostsByUserResolver;

impl ErasedBatchResolver for GetPostsByUserResolver {
    fn name(&self) -> &'static str {
        "getPostsByUser"
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
            // Fetch all posts for all user IDs in a single query
            let results = fetch_posts_by_user_ids(&keys).await;
            Ok(results)
        })
    }
}
```

## Server Configuration

```rust
use std::time::Duration;

let server = GraphQLServer::builder()
    .sdl(SCHEMA)
    .register_resolver(GetUserResolver)
    .register_batch_resolver(GetPostsByUserResolver)
    .batch_delay(Duration::from_millis(2))  // Wait before batching
    .max_batch_size(100)                     // Max keys per batch
    .skip_n1_validation()                    // Disable N+1 checks (not recommended)
    .build()?;
```

## Integration with Axum

```rust
use axum::{extract::State, routing::get, Router};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use std::sync::Arc;

async fn graphql_handler(
    State(server): State<Arc<GraphQLServer>>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    server.schema().execute(req.into_inner()).await.into()
}

let app = Router::new()
    .route("/graphql", get(playground).post(graphql_handler))
    .with_state(Arc::new(server));
```

## License

MIT
