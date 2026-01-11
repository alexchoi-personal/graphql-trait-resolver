use std::sync::Arc;

use async_graphql::http::GraphiQLSource;
use async_graphql::Value;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use graphql_resolver::{
    BoxFuture, ErasedBatchResolver, FxHashMap, GraphQLServer, Resolver, ResolverContext,
    ResolverResult,
};
use tower_http::cors::CorsLayer;

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
                    let id_str = user_id.as_str().unwrap_or("0");
                    let posts: Vec<serde_json::Value> = (1..=3)
                        .map(|i| {
                            serde_json::json!({
                                "id": format!("{}-post-{}", id_str, i),
                                "title": format!("Post {} by User {}", i, id_str),
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

const SCHEMA: &str = r#"
    type Query {
        user(id: ID!): User @resolver(name: "getUser")
        users: [User!]! @resolver(name: "listUsers")
    }

    type User {
        id: ID!
        name: String!
        email: String!
        posts: [Post!]! @resolver(name: "getPostsByUser") @batchKey(field: "id")
    }

    type Post {
        id: ID!
        title: String!
        content: String!
    }
"#;

async fn graphql_handler(
    State(server): State<Arc<GraphQLServer>>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    server.schema().execute(req.into_inner()).await.into()
}

async fn graphiql() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/graphql").finish())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let server = GraphQLServer::builder()
        .sdl(SCHEMA)
        .register_resolver(GetUserResolver)
        .register_resolver(ListUsersResolver)
        .register_batch_resolver(GetPostsByUserResolver)
        .build()
        .expect("Failed to build GraphQL server");

    let server = Arc::new(server);

    let app = Router::new()
        .route("/", get(graphiql))
        .route("/graphql", get(graphiql).post(graphql_handler))
        .layer(CorsLayer::permissive())
        .with_state(server);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!("GraphQL server running at http://localhost:8080");
    tracing::info!("GraphiQL playground at http://localhost:8080/graphql");

    axum::serve(listener, app).await.unwrap();
}
