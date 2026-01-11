//! Axum GraphQL Server Example
//!
//! This example demonstrates how to set up a GraphQL server using
//! `graphql-trait-resolver` with Axum.
//!
//! ## Running as HTTP Server
//!
//! To run this example as an HTTP server, add these dependencies to Cargo.toml:
//! ```toml
//! axum = "0.8"
//! async-graphql-axum = "7"
//! ```
//!
//! Then run: `cargo run --example axum_server`
//!
//! GraphQL Playground will be available at: http://localhost:8080/graphql
//!
//! ## Example Queries
//!
//! ```graphql
//! # Get a single user
//! query {
//!   user(id: "1") {
//!     id
//!     name
//!     email
//!   }
//! }
//!
//! # List all users with their posts (batched)
//! query {
//!   users {
//!     id
//!     name
//!     posts {
//!       id
//!       title
//!     }
//!   }
//! }
//! ```

use std::sync::Arc;

use async_graphql::Value;
use graphql_trait_resolver::{
    BoxFuture, ErasedBatchResolver, FxHashMap, GraphQLServer, Resolver, ResolverContext,
    ResolverResult,
};

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
            let id = args
                .get("id")
                .and_then(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "1".to_string());

            let user = serde_json::json!({
                "id": id,
                "name": format!("User {}", id),
                "email": format!("user{}@example.com", id),
            });

            Ok(serde_json::from_value(user).unwrap())
        })
    }
}

struct ListUsersResolver;

impl Resolver for ListUsersResolver {
    fn name(&self) -> &'static str {
        "listUsers"
    }

    fn resolve<'a>(
        &'a self,
        _ctx: &'a ResolverContext,
        _args: FxHashMap<String, Value>,
    ) -> BoxFuture<'a, ResolverResult<Value>> {
        Box::pin(async move {
            let users: Vec<serde_json::Value> = (1..=5)
                .map(|i| {
                    serde_json::json!({
                        "id": i.to_string(),
                        "name": format!("User {}", i),
                        "email": format!("user{}@example.com", i),
                    })
                })
                .collect();

            Ok(serde_json::from_value(serde_json::Value::Array(users)).unwrap())
        })
    }
}

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
            let results: Vec<(serde_json::Value, serde_json::Value)> = keys
                .into_iter()
                .map(|user_id| {
                    let posts: Vec<serde_json::Value> = (1..=3)
                        .map(|i| {
                            serde_json::json!({
                                "id": format!("{}-post-{}", user_id, i),
                                "title": format!("Post {} by user {}", i, user_id),
                                "content": "Lorem ipsum dolor sit amet...",
                            })
                        })
                        .collect();
                    (user_id, serde_json::Value::Array(posts))
                })
                .collect();
            Ok(results)
        })
    }
}

const SCHEMA_SDL: &str = r#"
    type Query {
        user(id: ID!): User @trait(name: "getUser")
        users: [User!]! @trait(name: "listUsers")
    }

    type User {
        id: ID!
        name: String!
        email: String!
        posts: [Post!]! @trait(name: "getPostsByUser") @batchKey(field: "id")
    }

    type Post {
        id: ID!
        title: String!
        content: String!
    }
"#;

fn build_server() -> Arc<GraphQLServer> {
    let server = GraphQLServer::builder()
        .sdl(SCHEMA_SDL)
        .register_resolver(GetUserResolver)
        .register_resolver(ListUsersResolver)
        .register_batch_resolver(GetPostsByUserResolver)
        .build()
        .expect("Failed to build GraphQL server");

    Arc::new(server)
}

#[tokio::main]
async fn main() {
    let server = build_server();

    println!("Executing sample queries...\n");

    let response = server
        .execute(r#"{ user(id: "42") { id name email } }"#)
        .await;
    println!("Query: user(id: \"42\")");
    println!("Response: {}", serde_json::to_string_pretty(&response.data).unwrap());
    if !response.errors.is_empty() {
        println!("Errors: {:?}", response.errors);
    }
    println!();

    let response = server
        .execute(r#"{ users { id name posts { id title } } }"#)
        .await;
    println!("Query: users with posts (batched)");
    println!("Response: {}", serde_json::to_string_pretty(&response.data).unwrap());
    if !response.errors.is_empty() {
        println!("Errors: {:?}", response.errors);
    }
    println!();

    println!("---");
    println!("To run as HTTP server, add to Cargo.toml:");
    println!("  axum = \"0.8\"");
    println!("  async-graphql-axum = \"7\"");
    println!();
    println!("Then use this handler:");
    println!(r#"
use axum::{{extract::State, routing::get, Router}};
use async_graphql_axum::{{GraphQLRequest, GraphQLResponse}};

async fn graphql_handler(
    State(server): State<Arc<GraphQLServer>>,
    req: GraphQLRequest,
) -> GraphQLResponse {{
    server.schema().execute(req.into_inner()).await.into()
}}

async fn playground() -> impl IntoResponse {{
    Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new("/graphql"),
    ))
}}

let app = Router::new()
    .route("/graphql", get(playground).post(graphql_handler))
    .with_state(server);
"#);
}
