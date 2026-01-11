use async_graphql::Value;
use graphql_trait_resolver::{
    BoxFuture, ErasedBatchResolver, FxHashMap, GraphQLServer, Resolver, ResolverContext,
    ResolverError, ResolverResult, ServerError,
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
                .unwrap_or_default();

            let user = serde_json::json!({
                "id": id,
                "name": format!("User {}", id),
            });

            Ok(serde_json::from_value(user).unwrap())
        })
    }
}

struct GetUsersBatchResolver;

impl ErasedBatchResolver for GetUsersBatchResolver {
    fn name(&self) -> &'static str {
        "getUsersByIds"
    }

    fn batch_key_field(&self) -> &'static str {
        "authorId"
    }

    fn load_erased<'a>(
        &'a self,
        _ctx: &'a ResolverContext,
        keys: Vec<serde_json::Value>,
    ) -> BoxFuture<'a, ResolverResult<Vec<(serde_json::Value, serde_json::Value)>>> {
        Box::pin(async move {
            let results: Vec<(serde_json::Value, serde_json::Value)> = keys
                .into_iter()
                .map(|key| {
                    let user = serde_json::json!({
                        "id": key,
                        "name": format!("User {:?}", key),
                    });
                    (key, user)
                })
                .collect();
            Ok(results)
        })
    }
}

#[test]
fn test_server_builder_without_sdl() {
    let result = GraphQLServer::builder().build();
    assert!(result.is_err());
    let err = result.err().unwrap();
    match err {
        ServerError::Config(msg) => assert!(msg.contains("SDL not provided")),
        _ => panic!("Expected Config error"),
    }
}

#[test]
fn test_server_builder_with_invalid_sdl() {
    let result = GraphQLServer::builder().sdl("invalid graphql").build();
    assert!(result.is_err());
    let err = result.err().unwrap();
    match err {
        ServerError::Parse(_) => {}
        _ => panic!("Expected Parse error"),
    }
}

#[test]
fn test_server_builder_simple_schema() {
    let sdl = r#"
        type Query {
            hello: String
        }
    "#;

    let result = GraphQLServer::builder().sdl(sdl).build();
    assert!(result.is_ok());
}

#[test]
fn test_server_builder_with_resolver() {
    let sdl = r#"
        type Query {
            user(id: ID!): User @trait(name: "getUser")
        }

        type User {
            id: ID!
            name: String!
        }
    "#;

    let result = GraphQLServer::builder()
        .sdl(sdl)
        .register_resolver(GetUserResolver)
        .build();

    assert!(result.is_ok());
}

#[test]
fn test_n1_detection_fails_without_batch_key() {
    let sdl = r#"
        type Query {
            users: [User!]!
        }

        type User {
            id: ID!
            posts: [Post!]! @trait(name: "getPostsByUser")
        }

        type Post {
            id: ID!
            title: String!
        }
    "#;

    let result = GraphQLServer::builder().sdl(sdl).build();
    assert!(result.is_err());
    let err = result.err().unwrap();
    match err {
        ServerError::N1Detection(errors) => {
            assert!(!errors.is_empty());
            assert!(errors[0].field_name == "posts");
        }
        _ => panic!("Expected N1Detection error"),
    }
}

#[test]
fn test_n1_detection_passes_with_batch_key() {
    let sdl = r#"
        type Query {
            users: [User!]!
        }

        type User {
            id: ID!
            posts: [Post!]! @trait(name: "getPostsByUser") @batchKey(field: "id")
        }

        type Post {
            id: ID!
            title: String!
        }
    "#;

    let result = GraphQLServer::builder().sdl(sdl).build();
    assert!(result.is_ok());
}

#[test]
fn test_skip_n1_validation() {
    let sdl = r#"
        type Query {
            users: [User!]!
        }

        type User {
            id: ID!
            posts: [Post!]! @trait(name: "getPostsByUser")
        }

        type Post {
            id: ID!
            title: String!
        }
    "#;

    let result = GraphQLServer::builder()
        .sdl(sdl)
        .skip_n1_validation()
        .build();
    assert!(result.is_ok());
}

#[test]
fn test_batch_delay_and_max_batch_size() {
    use std::time::Duration;

    let sdl = r#"
        type Query {
            hello: String
        }
    "#;

    let server = GraphQLServer::builder()
        .sdl(sdl)
        .batch_delay(Duration::from_millis(5))
        .max_batch_size(200)
        .build()
        .unwrap();

    assert_eq!(server.batch_delay(), Duration::from_millis(5));
    assert_eq!(server.max_batch_size(), 200);
}

#[test]
fn test_resolver_error_display() {
    let err = ResolverError::NotFound("test".to_string());
    assert!(err.to_string().contains("Resolver not found: test"));

    let err = ResolverError::Argument("invalid".to_string());
    assert!(err.to_string().contains("Argument error: invalid"));

    let err = ResolverError::Execution("failed".to_string());
    assert!(err.to_string().contains("Execution error: failed"));
}

#[tokio::test]
async fn test_execute_simple_query() {
    let sdl = r#"
        type Query {
            hello: String
        }
    "#;

    let server = GraphQLServer::builder().sdl(sdl).build().unwrap();

    let response = server.execute(r#"{ __typename }"#).await;
    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);
}

#[test]
fn test_call_directive_parsing() {
    let sdl = r#"
        type Query {
            user(id: ID!): User @trait(name: "getUser")
        }

        type User {
            id: ID!
            name: String!
            profile: Profile @call(trait: "getProfile", args: { userId: "$parent.id" })
        }

        type Profile {
            bio: String
        }
    "#;

    let result = GraphQLServer::builder()
        .sdl(sdl)
        .register_resolver(GetUserResolver)
        .skip_n1_validation()
        .build();

    assert!(result.is_ok());
}

#[test]
fn test_server_error_debug() {
    let err = ServerError::Config("test".to_string());
    let debug = format!("{:?}", err);
    assert!(debug.contains("Config"));
}

#[test]
fn test_register_batch_resolver() {
    let sdl = r#"
        type Query {
            users: [User!]!
        }

        type User {
            id: ID!
            author: Author @trait(name: "getUsersByIds") @batchKey(field: "id")
        }

        type Author {
            id: ID!
            name: String!
        }
    "#;

    let result = GraphQLServer::builder()
        .sdl(sdl)
        .register_batch_resolver(GetUsersBatchResolver)
        .build();

    assert!(result.is_ok());
}

#[test]
fn test_server_schema_access() {
    let sdl = r#"
        type Query {
            hello: String
        }
    "#;

    let server = GraphQLServer::builder().sdl(sdl).build().unwrap();

    let _schema = server.schema();
    let _registry = server.registry();
}

#[test]
fn test_execute_sync() {
    let sdl = r#"
        type Query {
            hello: String
        }
    "#;

    let server = GraphQLServer::builder().sdl(sdl).build().unwrap();

    let response = server.execute_sync("{ __typename }");
    assert!(response.errors.is_empty());
}

#[test]
fn test_deeply_nested_n1_detection() {
    let sdl = r#"
        type Query {
            organizations: [Organization!]!
        }

        type Organization {
            id: ID!
            users: [User!]!
        }

        type User {
            id: ID!
            posts: [Post!]! @trait(name: "getPostsByUser")
        }

        type Post {
            id: ID!
            title: String!
        }
    "#;

    let result = GraphQLServer::builder().sdl(sdl).build();
    assert!(result.is_err());
    let err = result.err().unwrap();
    match err {
        ServerError::N1Detection(errors) => {
            assert!(!errors.is_empty());
        }
        _ => panic!("Expected N1Detection error"),
    }
}

#[test]
fn test_non_list_context_resolver_allowed() {
    let sdl = r#"
        type Query {
            user: User @trait(name: "getUser")
        }

        type User {
            id: ID!
            name: String!
        }
    "#;

    let result = GraphQLServer::builder()
        .sdl(sdl)
        .register_resolver(GetUserResolver)
        .build();

    assert!(result.is_ok());
}

#[test]
fn test_multiple_sdl_files() {
    let types_sdl = r#"
        type User {
            id: ID!
            name: String!
        }

        type Post {
            id: ID!
            title: String!
        }
    "#;

    let query_sdl = r#"
        type Query {
            user(id: ID!): User @trait(name: "getUser")
            posts: [Post!]!
        }
    "#;

    let result = GraphQLServer::builder()
        .sdl(types_sdl)
        .sdl(query_sdl)
        .register_resolver(GetUserResolver)
        .build();

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_resolver_returns_data() {
    let sdl = r#"
        type Query {
            user(id: ID!): User @trait(name: "getUser")
        }

        type User {
            id: ID!
            name: String!
        }
    "#;

    let server = GraphQLServer::builder()
        .sdl(sdl)
        .register_resolver(GetUserResolver)
        .build()
        .unwrap();

    let response = server.execute(r#"{ user(id: "42") { id name } }"#).await;

    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json().unwrap();
    let user = &data["user"];
    assert_eq!(user["id"], "42");
    assert_eq!(user["name"], "User 42");
}

#[tokio::test]
async fn test_list_resolver_returns_data() {
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
                let users = serde_json::json!([
                    {"id": "1", "name": "Alice"},
                    {"id": "2", "name": "Bob"}
                ]);
                Ok(serde_json::from_value(users).unwrap())
            })
        }
    }

    let sdl = r#"
        type Query {
            users: [User!]! @trait(name: "listUsers")
        }

        type User {
            id: ID!
            name: String!
        }
    "#;

    let server = GraphQLServer::builder()
        .sdl(sdl)
        .register_resolver(ListUsersResolver)
        .build()
        .unwrap();

    let response = server.execute(r#"{ users { id name } }"#).await;

    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json().unwrap();
    let users = data["users"].as_array().unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0]["id"], "1");
    assert_eq!(users[0]["name"], "Alice");
    assert_eq!(users[1]["id"], "2");
    assert_eq!(users[1]["name"], "Bob");
}

#[tokio::test]
async fn test_batch_resolver_returns_batched_data() {
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
                        let posts = serde_json::json!([
                            {"id": format!("{}-post-1", user_id), "title": format!("Post by {}", user_id)}
                        ]);
                        (user_id, posts)
                    })
                    .collect();
                Ok(results)
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
                let users = serde_json::json!([
                    {"id": "user-1", "name": "Alice"},
                    {"id": "user-2", "name": "Bob"}
                ]);
                Ok(serde_json::from_value(users).unwrap())
            })
        }
    }

    let sdl = r#"
        type Query {
            users: [User!]! @trait(name: "listUsers")
        }

        type User {
            id: ID!
            name: String!
            posts: [Post!]! @trait(name: "getPostsByUser") @batchKey(field: "id")
        }

        type Post {
            id: ID!
            title: String!
        }
    "#;

    let server = GraphQLServer::builder()
        .sdl(sdl)
        .register_resolver(ListUsersResolver)
        .register_batch_resolver(GetPostsByUserResolver)
        .build()
        .unwrap();

    let response = server
        .execute(r#"{ users { id name posts { id title } } }"#)
        .await;

    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json().unwrap();
    let users = data["users"].as_array().unwrap();

    assert_eq!(users.len(), 2);
    assert_eq!(users[0]["id"], "user-1");
    assert_eq!(users[0]["name"], "Alice");

    let posts = users[0]["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 1);
    assert!(posts[0]["id"].as_str().unwrap().contains("user-1"));
    assert!(posts[0]["title"].as_str().unwrap().contains("user-1"));

    assert_eq!(users[1]["id"], "user-2");
    let posts2 = users[1]["posts"].as_array().unwrap();
    assert!(posts2[0]["id"].as_str().unwrap().contains("user-2"));
}

#[tokio::test]
async fn test_call_directive_maps_parent_field() {
    struct GetProfileResolver;

    impl Resolver for GetProfileResolver {
        fn name(&self) -> &'static str {
            "getProfile"
        }

        fn resolve<'a>(
            &'a self,
            _ctx: &'a ResolverContext,
            args: FxHashMap<String, Value>,
        ) -> BoxFuture<'a, ResolverResult<Value>> {
            Box::pin(async move {
                let user_id = args
                    .get("userId")
                    .and_then(|v| match v {
                        Value::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();

                let profile = serde_json::json!({
                    "bio": format!("Bio for user {}", user_id),
                    "avatarUrl": format!("https://example.com/avatar/{}.png", user_id)
                });

                Ok(serde_json::from_value(profile).unwrap())
            })
        }
    }

    let sdl = r#"
        type Query {
            user(id: ID!): User @trait(name: "getUser")
        }

        type User {
            id: ID!
            name: String!
            profile: Profile @call(trait: "getProfile", args: { userId: "$parent.id" })
        }

        type Profile {
            bio: String!
            avatarUrl: String!
        }
    "#;

    let server = GraphQLServer::builder()
        .sdl(sdl)
        .register_resolver(GetUserResolver)
        .register_resolver(GetProfileResolver)
        .skip_n1_validation()
        .build()
        .unwrap();

    let response = server
        .execute(r#"{ user(id: "42") { id name profile { bio avatarUrl } } }"#)
        .await;

    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json().unwrap();
    let user = &data["user"];

    assert_eq!(user["id"], "42");
    assert_eq!(user["name"], "User 42");

    let profile = &user["profile"];
    assert_eq!(profile["bio"], "Bio for user 42");
    assert!(profile["avatarUrl"].as_str().unwrap().contains("42"));
}

#[tokio::test]
async fn test_deeply_nested_resolver_data_flow() {
    struct GetOrgsResolver;

    impl Resolver for GetOrgsResolver {
        fn name(&self) -> &'static str {
            "getOrgs"
        }

        fn resolve<'a>(
            &'a self,
            _ctx: &'a ResolverContext,
            _args: FxHashMap<String, Value>,
        ) -> BoxFuture<'a, ResolverResult<Value>> {
            Box::pin(async move {
                let orgs = serde_json::json!([
                    {"id": "org-1", "name": "Acme Corp"}
                ]);
                Ok(serde_json::from_value(orgs).unwrap())
            })
        }
    }

    struct GetTeamsByOrgResolver;

    impl ErasedBatchResolver for GetTeamsByOrgResolver {
        fn name(&self) -> &'static str {
            "getTeamsByOrg"
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
                    .map(|org_id| {
                        let teams = serde_json::json!([
                            {"id": format!("{}-team-1", org_id), "name": "Engineering"},
                            {"id": format!("{}-team-2", org_id), "name": "Design"}
                        ]);
                        (org_id, teams)
                    })
                    .collect();
                Ok(results)
            })
        }
    }

    struct GetMembersByTeamResolver;

    impl ErasedBatchResolver for GetMembersByTeamResolver {
        fn name(&self) -> &'static str {
            "getMembersByTeam"
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
                    .map(|team_id| {
                        let members = serde_json::json!([
                            {"id": format!("{}-member-1", team_id), "name": "John"}
                        ]);
                        (team_id, members)
                    })
                    .collect();
                Ok(results)
            })
        }
    }

    let sdl = r#"
        type Query {
            organizations: [Organization!]! @trait(name: "getOrgs")
        }

        type Organization {
            id: ID!
            name: String!
            teams: [Team!]! @trait(name: "getTeamsByOrg") @batchKey(field: "id")
        }

        type Team {
            id: ID!
            name: String!
            members: [Member!]! @trait(name: "getMembersByTeam") @batchKey(field: "id")
        }

        type Member {
            id: ID!
            name: String!
        }
    "#;

    let server = GraphQLServer::builder()
        .sdl(sdl)
        .register_resolver(GetOrgsResolver)
        .register_batch_resolver(GetTeamsByOrgResolver)
        .register_batch_resolver(GetMembersByTeamResolver)
        .build()
        .unwrap();

    let response = server
        .execute(
            r#"{
                organizations {
                    id
                    name
                    teams {
                        id
                        name
                        members {
                            id
                            name
                        }
                    }
                }
            }"#,
        )
        .await;

    assert!(response.errors.is_empty(), "Errors: {:?}", response.errors);

    let data = response.data.into_json().unwrap();
    let orgs = data["organizations"].as_array().unwrap();

    assert_eq!(orgs.len(), 1);
    assert_eq!(orgs[0]["id"], "org-1");
    assert_eq!(orgs[0]["name"], "Acme Corp");

    let teams = orgs[0]["teams"].as_array().unwrap();
    assert_eq!(teams.len(), 2);
    assert_eq!(teams[0]["name"], "Engineering");
    assert_eq!(teams[1]["name"], "Design");

    let members = teams[0]["members"].as_array().unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["name"], "John");
    assert!(members[0]["id"].as_str().unwrap().contains("team-1"));
}

#[test]
fn test_multiple_sdl_with_shared_types() {
    let common_sdl = r#"
        type User {
            id: ID!
            name: String!
        }
    "#;

    let posts_sdl = r#"
        type Post {
            id: ID!
            title: String!
            authorId: ID!
        }
    "#;

    let query_sdl = r#"
        type Query {
            user(id: ID!): User @trait(name: "getUser")
        }
    "#;

    let result = GraphQLServer::builder()
        .sdl(common_sdl)
        .sdl(posts_sdl)
        .sdl(query_sdl)
        .register_resolver(GetUserResolver)
        .build();

    assert!(result.is_ok());
}
