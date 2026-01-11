use std::hint::black_box;

use async_graphql::Value;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use graphql_trait_resolver::{
    BoxFuture, ErasedBatchResolver, FxHashMap, GraphQLServer, Resolver, ResolverContext,
    ResolverResult, TraitRegistry,
};

const SIMPLE_SDL: &str = r#"
    type Query {
        hello: String
        user(id: ID!): User
    }

    type User {
        id: ID!
        name: String!
    }
"#;

const COMPLEX_SDL: &str = r#"
    type Query {
        users: [User!]!
        posts: [Post!]!
        comments: [Comment!]!
    }

    type User {
        id: ID!
        name: String!
        email: String
        posts: [Post!]! @trait(name: "getPostsByUser") @batchKey(field: "id")
    }

    type Post {
        id: ID!
        title: String!
        content: String
        authorId: ID!
        author: User @trait(name: "getUserById") @batchKey(field: "authorId")
        comments: [Comment!]! @trait(name: "getCommentsByPost") @batchKey(field: "id")
    }

    type Comment {
        id: ID!
        text: String!
        postId: ID!
        authorId: ID!
    }
"#;

const DEEPLY_NESTED_SDL: &str = r#"
    type Query {
        organizations: [Organization!]!
    }

    type Organization {
        id: ID!
        name: String!
        departments: [Department!]! @trait(name: "getDepartments") @batchKey(field: "id")
    }

    type Department {
        id: ID!
        name: String!
        teams: [Team!]! @trait(name: "getTeams") @batchKey(field: "id")
    }

    type Team {
        id: ID!
        name: String!
        members: [User!]! @trait(name: "getMembers") @batchKey(field: "id")
    }

    type User {
        id: ID!
        name: String!
        tasks: [Task!]! @trait(name: "getTasks") @batchKey(field: "id")
    }

    type Task {
        id: ID!
        title: String!
    }
"#;

const ECOMMERCE_SDL: &str = r#"
    type Query {
        products(limit: Int, offset: Int): [Product!]!
        product(id: ID!): Product
        orders(customerId: ID!): [Order!]!
        customer(id: ID!): Customer
        categories: [Category!]!
    }

    type Mutation {
        createOrder(input: OrderInput!): Order!
        updateProduct(id: ID!, input: ProductInput!): Product!
    }

    type Product {
        id: ID!
        sku: String!
        name: String!
        description: String
        price: Float!
        currency: String!
        inventory: Int!
        categoryId: ID!
        category: Category! @trait(name: "getCategoryById") @batchKey(field: "categoryId")
        reviews: [Review!]! @trait(name: "getReviewsByProduct") @batchKey(field: "id")
        relatedProducts: [Product!]! @trait(name: "getRelatedProducts") @batchKey(field: "id")
    }

    type Category {
        id: ID!
        name: String!
        slug: String!
        parentId: ID
        parent: Category @trait(name: "getCategoryById") @batchKey(field: "parentId")
        products: [Product!]! @trait(name: "getProductsByCategory") @batchKey(field: "id")
    }

    type Customer {
        id: ID!
        email: String!
        firstName: String!
        lastName: String!
        phone: String
        addresses: [Address!]!
        orders: [Order!]! @trait(name: "getOrdersByCustomer") @batchKey(field: "id")
        wishlist: [Product!]! @trait(name: "getWishlistProducts") @batchKey(field: "id")
    }

    type Address {
        id: ID!
        street: String!
        city: String!
        state: String!
        country: String!
        zipCode: String!
        isDefault: Boolean!
    }

    type Order {
        id: ID!
        orderNumber: String!
        status: OrderStatus!
        customerId: ID!
        customer: Customer! @trait(name: "getCustomerById") @batchKey(field: "customerId")
        items: [OrderItem!]!
        subtotal: Float!
        tax: Float!
        shipping: Float!
        total: Float!
        createdAt: String!
        updatedAt: String!
    }

    type OrderItem {
        id: ID!
        productId: ID!
        product: Product! @trait(name: "getProductById") @batchKey(field: "productId")
        quantity: Int!
        unitPrice: Float!
        total: Float!
    }

    type Review {
        id: ID!
        productId: ID!
        customerId: ID!
        customer: Customer! @trait(name: "getCustomerById") @batchKey(field: "customerId")
        rating: Int!
        title: String!
        content: String!
        createdAt: String!
    }

    enum OrderStatus {
        PENDING
        PROCESSING
        SHIPPED
        DELIVERED
        CANCELLED
    }

    input OrderInput {
        customerId: ID!
        items: [OrderItemInput!]!
    }

    input OrderItemInput {
        productId: ID!
        quantity: Int!
    }

    input ProductInput {
        name: String
        description: String
        price: Float
        inventory: Int
    }
"#;

const SOCIAL_MEDIA_SDL: &str = r#"
    type Query {
        feed(userId: ID!, limit: Int): [Post!]!
        user(id: ID!): User
        post(id: ID!): Post
        trending(limit: Int): [Hashtag!]!
        search(query: String!): SearchResult!
    }

    type Mutation {
        createPost(content: String!, mediaUrls: [String!]): Post!
        likePost(postId: ID!): Post!
        followUser(userId: ID!): User!
        createComment(postId: ID!, content: String!): Comment!
    }

    type User {
        id: ID!
        username: String!
        displayName: String!
        bio: String
        avatarUrl: String
        verified: Boolean!
        followerCount: Int!
        followingCount: Int!
        postCount: Int!
        followers: [User!]! @trait(name: "getFollowers") @batchKey(field: "id")
        following: [User!]! @trait(name: "getFollowing") @batchKey(field: "id")
        posts: [Post!]! @trait(name: "getPostsByUser") @batchKey(field: "id")
        likedPosts: [Post!]! @trait(name: "getLikedPosts") @batchKey(field: "id")
        notifications: [Notification!]! @trait(name: "getNotifications") @batchKey(field: "id")
        createdAt: String!
    }

    type Post {
        id: ID!
        content: String!
        authorId: ID!
        author: User! @trait(name: "getUserById") @batchKey(field: "authorId")
        mediaUrls: [String!]!
        likeCount: Int!
        commentCount: Int!
        repostCount: Int!
        likes: [Like!]! @trait(name: "getLikesByPost") @batchKey(field: "id")
        comments: [Comment!]! @trait(name: "getCommentsByPost") @batchKey(field: "id")
        hashtags: [Hashtag!]!
        mentions: [User!]! @trait(name: "getMentionedUsers") @batchKey(field: "id")
        repostedFrom: Post @trait(name: "getOriginalPost") @batchKey(field: "id")
        createdAt: String!
    }

    type Comment {
        id: ID!
        content: String!
        postId: ID!
        authorId: ID!
        author: User! @trait(name: "getUserById") @batchKey(field: "authorId")
        likeCount: Int!
        replies: [Comment!]! @trait(name: "getReplies") @batchKey(field: "id")
        createdAt: String!
    }

    type Like {
        id: ID!
        userId: ID!
        user: User! @trait(name: "getUserById") @batchKey(field: "userId")
        createdAt: String!
    }

    type Hashtag {
        id: ID!
        name: String!
        postCount: Int!
        posts: [Post!]! @trait(name: "getPostsByHashtag") @batchKey(field: "id")
    }

    type Notification {
        id: ID!
        type: NotificationType!
        actorId: ID!
        actor: User! @trait(name: "getUserById") @batchKey(field: "actorId")
        postId: ID
        post: Post @trait(name: "getPostById") @batchKey(field: "postId")
        read: Boolean!
        createdAt: String!
    }

    type SearchResult {
        users: [User!]!
        posts: [Post!]!
        hashtags: [Hashtag!]!
    }

    enum NotificationType {
        LIKE
        COMMENT
        FOLLOW
        MENTION
        REPOST
    }
"#;

const GITHUB_LIKE_SDL: &str = r#"
    type Query {
        repository(owner: String!, name: String!): Repository
        user(login: String!): User
        organization(login: String!): Organization
        search(query: String!, type: SearchType!): SearchConnection!
    }

    type Repository {
        id: ID!
        name: String!
        fullName: String!
        description: String
        ownerId: ID!
        owner: RepositoryOwner! @trait(name: "getOwner") @batchKey(field: "ownerId")
        visibility: Visibility!
        defaultBranch: String!
        starCount: Int!
        forkCount: Int!
        watcherCount: Int!
        issues(state: IssueState): [Issue!]! @trait(name: "getIssuesByRepo") @batchKey(field: "id")
        pullRequests(state: PRState): [PullRequest!]! @trait(name: "getPRsByRepo") @batchKey(field: "id")
        branches: [Branch!]! @trait(name: "getBranches") @batchKey(field: "id")
        releases: [Release!]! @trait(name: "getReleases") @batchKey(field: "id")
        contributors: [User!]! @trait(name: "getContributors") @batchKey(field: "id")
        languages: [Language!]!
        topics: [String!]!
        license: String
        createdAt: String!
        updatedAt: String!
    }

    type User {
        id: ID!
        login: String!
        name: String
        email: String
        avatarUrl: String!
        bio: String
        company: String
        location: String
        repositories: [Repository!]! @trait(name: "getReposByUser") @batchKey(field: "id")
        starredRepositories: [Repository!]! @trait(name: "getStarredRepos") @batchKey(field: "id")
        followers: [User!]! @trait(name: "getFollowers") @batchKey(field: "id")
        following: [User!]! @trait(name: "getFollowing") @batchKey(field: "id")
        organizations: [Organization!]! @trait(name: "getOrganizations") @batchKey(field: "id")
        contributions: ContributionStats!
        createdAt: String!
    }

    type Organization {
        id: ID!
        login: String!
        name: String
        description: String
        avatarUrl: String!
        websiteUrl: String
        repositories: [Repository!]! @trait(name: "getReposByOrg") @batchKey(field: "id")
        members: [User!]! @trait(name: "getOrgMembers") @batchKey(field: "id")
        teams: [Team!]! @trait(name: "getOrgTeams") @batchKey(field: "id")
    }

    type Team {
        id: ID!
        name: String!
        slug: String!
        description: String
        members: [User!]! @trait(name: "getTeamMembers") @batchKey(field: "id")
        repositories: [Repository!]! @trait(name: "getTeamRepos") @batchKey(field: "id")
    }

    type Issue {
        id: ID!
        number: Int!
        title: String!
        body: String!
        state: IssueState!
        authorId: ID!
        author: User! @trait(name: "getUserById") @batchKey(field: "authorId")
        assignees: [User!]! @trait(name: "getAssignees") @batchKey(field: "id")
        labels: [Label!]!
        comments: [IssueComment!]! @trait(name: "getIssueComments") @batchKey(field: "id")
        milestone: Milestone
        createdAt: String!
        updatedAt: String!
        closedAt: String
    }

    type PullRequest {
        id: ID!
        number: Int!
        title: String!
        body: String!
        state: PRState!
        authorId: ID!
        author: User! @trait(name: "getUserById") @batchKey(field: "authorId")
        headBranch: String!
        baseBranch: String!
        mergeable: Boolean
        merged: Boolean!
        reviewers: [User!]! @trait(name: "getReviewers") @batchKey(field: "id")
        reviews: [Review!]! @trait(name: "getPRReviews") @batchKey(field: "id")
        commits: [Commit!]! @trait(name: "getPRCommits") @batchKey(field: "id")
        comments: [IssueComment!]! @trait(name: "getPRComments") @batchKey(field: "id")
        labels: [Label!]!
        createdAt: String!
        updatedAt: String!
        mergedAt: String
    }

    type Branch {
        name: String!
        commitId: ID!
        commit: Commit! @trait(name: "getCommitById") @batchKey(field: "commitId")
        protected: Boolean!
    }

    type Commit {
        id: ID!
        sha: String!
        message: String!
        authorId: ID
        author: User @trait(name: "getUserById") @batchKey(field: "authorId")
        authoredAt: String!
        additions: Int!
        deletions: Int!
    }

    type Release {
        id: ID!
        tagName: String!
        name: String
        body: String
        draft: Boolean!
        prerelease: Boolean!
        authorId: ID!
        author: User! @trait(name: "getUserById") @batchKey(field: "authorId")
        assets: [ReleaseAsset!]!
        createdAt: String!
        publishedAt: String
    }

    type ReleaseAsset {
        id: ID!
        name: String!
        size: Int!
        downloadCount: Int!
        downloadUrl: String!
    }

    type Review {
        id: ID!
        authorId: ID!
        author: User! @trait(name: "getUserById") @batchKey(field: "authorId")
        state: ReviewState!
        body: String
        submittedAt: String!
    }

    type IssueComment {
        id: ID!
        body: String!
        authorId: ID!
        author: User! @trait(name: "getUserById") @batchKey(field: "authorId")
        createdAt: String!
        updatedAt: String!
    }

    type Label {
        id: ID!
        name: String!
        color: String!
        description: String
    }

    type Milestone {
        id: ID!
        title: String!
        description: String
        dueOn: String
        state: MilestoneState!
    }

    type Language {
        name: String!
        color: String
        percentage: Float!
    }

    type ContributionStats {
        totalCommits: Int!
        totalPullRequests: Int!
        totalIssues: Int!
        totalReviews: Int!
    }

    type SearchConnection {
        totalCount: Int!
        nodes: [SearchResult!]!
    }

    union SearchResult = Repository | User | Issue | PullRequest
    union RepositoryOwner = User | Organization

    enum Visibility {
        PUBLIC
        PRIVATE
        INTERNAL
    }

    enum IssueState {
        OPEN
        CLOSED
    }

    enum PRState {
        OPEN
        CLOSED
        MERGED
    }

    enum ReviewState {
        PENDING
        APPROVED
        CHANGES_REQUESTED
        COMMENTED
    }

    enum MilestoneState {
        OPEN
        CLOSED
    }

    enum SearchType {
        REPOSITORY
        USER
        ISSUE
    }
"#;

struct MockResolver {
    name: &'static str,
}

impl Resolver for MockResolver {
    fn name(&self) -> &'static str {
        self.name
    }

    fn resolve<'a>(
        &'a self,
        _ctx: &'a ResolverContext,
        _args: FxHashMap<String, Value>,
    ) -> BoxFuture<'a, ResolverResult<Value>> {
        Box::pin(async move {
            Ok(Value::Object(Default::default()))
        })
    }
}

struct MockBatchResolver {
    name: &'static str,
    batch_key: &'static str,
}

impl ErasedBatchResolver for MockBatchResolver {
    fn name(&self) -> &'static str {
        self.name
    }

    fn batch_key_field(&self) -> &'static str {
        self.batch_key
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

struct FakeUserResolver;

impl Resolver for FakeUserResolver {
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
            }).unwrap_or_else(|| "1".to_string());

            let mut user = async_graphql::indexmap::IndexMap::new();
            user.insert(async_graphql::Name::new("id"), Value::String(id.clone()));
            user.insert(async_graphql::Name::new("name"), Value::String(format!("User {}", id)));
            user.insert(async_graphql::Name::new("email"), Value::String(format!("user{}@example.com", id)));
            Ok(Value::Object(user))
        })
    }
}

struct FakeUsersResolver;

impl Resolver for FakeUsersResolver {
    fn name(&self) -> &'static str {
        "listUsers"
    }

    fn resolve<'a>(
        &'a self,
        _ctx: &'a ResolverContext,
        _args: FxHashMap<String, Value>,
    ) -> BoxFuture<'a, ResolverResult<Value>> {
        Box::pin(async move {
            let users: Vec<Value> = (1..=10).map(|i| {
                let mut user = async_graphql::indexmap::IndexMap::new();
                user.insert(async_graphql::Name::new("id"), Value::String(i.to_string()));
                user.insert(async_graphql::Name::new("name"), Value::String(format!("User {}", i)));
                user.insert(async_graphql::Name::new("email"), Value::String(format!("user{}@example.com", i)));
                Value::Object(user)
            }).collect();
            Ok(Value::List(users))
        })
    }
}

struct FakePostsBatchResolver;

impl ErasedBatchResolver for FakePostsBatchResolver {
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
            let results: Vec<(serde_json::Value, serde_json::Value)> = keys.into_iter().map(|user_id| {
                let posts: Vec<serde_json::Value> = (1..=3).map(|i| {
                    serde_json::json!({
                        "id": format!("{}-{}", user_id, i),
                        "title": format!("Post {} by user {}", i, user_id),
                        "content": "Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
                        "authorId": user_id
                    })
                }).collect();
                (user_id, serde_json::Value::Array(posts))
            }).collect();
            Ok(results)
        })
    }
}

struct FakeCommentsBatchResolver;

impl ErasedBatchResolver for FakeCommentsBatchResolver {
    fn name(&self) -> &'static str {
        "getCommentsByPost"
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
            let results: Vec<(serde_json::Value, serde_json::Value)> = keys.into_iter().map(|post_id| {
                let comments: Vec<serde_json::Value> = (1..=2).map(|i| {
                    serde_json::json!({
                        "id": format!("{}-comment-{}", post_id, i),
                        "text": format!("This is comment {} on post {}", i, post_id),
                        "postId": post_id,
                        "authorId": i.to_string()
                    })
                }).collect();
                (post_id, serde_json::Value::Array(comments))
            }).collect();
            Ok(results)
        })
    }
}

fn bench_sdl_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("sdl_parsing");

    let schemas: &[(&str, &str)] = &[
        ("simple", SIMPLE_SDL),
        ("complex", COMPLEX_SDL),
        ("deeply_nested", DEEPLY_NESTED_SDL),
        ("ecommerce", ECOMMERCE_SDL),
        ("social_media", SOCIAL_MEDIA_SDL),
        ("github_like", GITHUB_LIKE_SDL),
    ];

    for (name, sdl) in schemas {
        group.throughput(Throughput::Bytes(sdl.len() as u64));
        group.bench_with_input(
            BenchmarkId::new(*name, sdl.len()),
            sdl,
            |b, sdl| {
                b.iter(|| {
                    let server = GraphQLServer::builder()
                        .sdl(black_box(sdl))
                        .skip_n1_validation()
                        .build();
                    black_box(server)
                });
            },
        );
    }

    group.finish();
}

fn bench_n1_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("n1_detection");

    group.bench_function("simple_schema_validation", |b| {
        b.iter(|| {
            let server = GraphQLServer::builder()
                .sdl(black_box(SIMPLE_SDL))
                .build();
            black_box(server)
        });
    });

    group.bench_function("complex_schema_validation", |b| {
        b.iter(|| {
            let server = GraphQLServer::builder()
                .sdl(black_box(COMPLEX_SDL))
                .skip_n1_validation()
                .build();
            black_box(server)
        });
    });

    group.bench_function("deeply_nested_validation", |b| {
        b.iter(|| {
            let server = GraphQLServer::builder()
                .sdl(black_box(DEEPLY_NESTED_SDL))
                .skip_n1_validation()
                .build();
            black_box(server)
        });
    });

    group.finish();
}

fn bench_registry_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("registry_lookup");

    let mut registry = TraitRegistry::default();
    for i in 0..100 {
        registry.register_resolver(MockResolver {
            name: Box::leak(format!("resolver_{}", i).into_boxed_str()),
        });
        registry.register_batch_resolver(MockBatchResolver {
            name: Box::leak(format!("batch_resolver_{}", i).into_boxed_str()),
            batch_key: "id",
        });
    }

    group.bench_function("resolver_lookup_hit", |b| {
        b.iter(|| {
            let result = registry.get_resolver(black_box("resolver_50"));
            black_box(result)
        });
    });

    group.bench_function("resolver_lookup_miss", |b| {
        b.iter(|| {
            let result = registry.get_resolver(black_box("nonexistent"));
            black_box(result)
        });
    });

    group.bench_function("batch_resolver_lookup_hit", |b| {
        b.iter(|| {
            let result = registry.get_batch_resolver(black_box("batch_resolver_50"));
            black_box(result)
        });
    });

    group.bench_function("batch_resolver_lookup_miss", |b| {
        b.iter(|| {
            let result = registry.get_batch_resolver(black_box("nonexistent"));
            black_box(result)
        });
    });

    group.finish();
}

fn bench_server_builder(c: &mut Criterion) {
    let mut group = c.benchmark_group("server_builder");

    group.bench_function("builder_with_resolvers", |b| {
        b.iter(|| {
            let server = GraphQLServer::builder()
                .sdl(black_box(COMPLEX_SDL))
                .register_resolver(MockResolver { name: "getPostsByUser" })
                .register_resolver(MockResolver { name: "getUserById" })
                .register_resolver(MockResolver { name: "getCommentsByPost" })
                .register_batch_resolver(MockBatchResolver {
                    name: "getPostsByUserBatch",
                    batch_key: "userId",
                })
                .skip_n1_validation()
                .build();
            black_box(server)
        });
    });

    group.bench_function("builder_minimal", |b| {
        b.iter(|| {
            let server = GraphQLServer::builder()
                .sdl(black_box(SIMPLE_SDL))
                .build();
            black_box(server)
        });
    });

    group.finish();
}

fn bench_query_execution(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("query_execution");

    let server = GraphQLServer::builder()
        .sdl(SIMPLE_SDL)
        .build()
        .unwrap();

    group.bench_function("introspection_typename", |b| {
        b.to_async(&rt).iter(|| async {
            let response = server.execute(black_box("{ __typename }")).await;
            black_box(response)
        });
    });

    group.bench_function("simple_field", |b| {
        b.to_async(&rt).iter(|| async {
            let response = server.execute(black_box("{ hello }")).await;
            black_box(response)
        });
    });

    group.finish();
}

fn bench_resolver_context(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolver_context");

    group.bench_function("context_creation", |b| {
        b.iter(|| {
            let ctx = ResolverContext::new(black_box("fieldName".to_string()));
            black_box(ctx)
        });
    });

    group.bench_function("context_with_parent", |b| {
        let parent = Value::Object(Default::default());
        b.iter(|| {
            let ctx = ResolverContext::new(black_box("fieldName".to_string()))
                .with_parent(black_box(parent.clone()));
            black_box(ctx)
        });
    });

    group.bench_function("context_with_path", |b| {
        let path = vec!["Query".to_string(), "user".to_string(), "posts".to_string()];
        b.iter(|| {
            let ctx = ResolverContext::new(black_box("fieldName".to_string()))
                .with_path(black_box(path.clone()));
            black_box(ctx)
        });
    });

    group.bench_function("context_full_chain", |b| {
        let parent = Value::Object(Default::default());
        let path = vec!["Query".to_string(), "user".to_string(), "posts".to_string()];
        b.iter(|| {
            let ctx = ResolverContext::new(black_box("fieldName".to_string()))
                .with_parent(black_box(parent.clone()))
                .with_path(black_box(path.clone()));
            black_box(ctx)
        });
    });

    group.finish();
}

fn bench_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling");

    for num_types in [10, 50, 100, 200] {
        let sdl = generate_large_schema(num_types);

        group.throughput(Throughput::Elements(num_types as u64));
        group.bench_with_input(
            BenchmarkId::new("schema_build", num_types),
            &sdl,
            |b, sdl| {
                b.iter(|| {
                    let server = GraphQLServer::builder()
                        .sdl(black_box(sdl))
                        .skip_n1_validation()
                        .build();
                    black_box(server)
                });
            },
        );
    }

    group.finish();
}

fn generate_large_schema(num_types: usize) -> String {
    let mut sdl = String::from("type Query {\n");

    for i in 0..num_types {
        sdl.push_str(&format!("    type{}: Type{}!\n", i, i));
    }
    sdl.push_str("}\n\n");

    for i in 0..num_types {
        sdl.push_str(&format!(
            r#"type Type{} {{
    id: ID!
    name: String!
    value: Int
    description: String
}}

"#,
            i
        ));
    }

    sdl
}

const FAKE_BLOG_SDL: &str = r#"
    type Query {
        user(id: ID!): User @trait(name: "getUser")
        users: [User!]! @trait(name: "listUsers")
    }

    type User {
        id: ID!
        name: String!
        email: String
        posts: [Post!]! @trait(name: "getPostsByUser") @batchKey(field: "id")
    }

    type Post {
        id: ID!
        title: String!
        content: String
        authorId: ID!
        comments: [Comment!]! @trait(name: "getCommentsByPost") @batchKey(field: "id")
    }

    type Comment {
        id: ID!
        text: String!
        postId: ID!
        authorId: ID!
    }
"#;

fn bench_fake_example(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("fake_example");

    let server = GraphQLServer::builder()
        .sdl(FAKE_BLOG_SDL)
        .register_resolver(FakeUserResolver)
        .register_resolver(FakeUsersResolver)
        .register_batch_resolver(FakePostsBatchResolver)
        .register_batch_resolver(FakeCommentsBatchResolver)
        .skip_n1_validation()
        .build()
        .unwrap();

    group.bench_function("single_user_query", |b| {
        b.to_async(&rt).iter(|| async {
            let response = server.execute(black_box(r#"
                query {
                    user(id: "1") {
                        id
                        name
                        email
                    }
                }
            "#)).await;
            black_box(response)
        });
    });

    group.bench_function("users_list_query", |b| {
        b.to_async(&rt).iter(|| async {
            let response = server.execute(black_box(r#"
                query {
                    users {
                        id
                        name
                        email
                    }
                }
            "#)).await;
            black_box(response)
        });
    });

    group.bench_function("nested_posts_query", |b| {
        b.to_async(&rt).iter(|| async {
            let response = server.execute(black_box(r#"
                query {
                    user(id: "1") {
                        id
                        name
                        posts {
                            id
                            title
                            content
                        }
                    }
                }
            "#)).await;
            black_box(response)
        });
    });

    group.bench_function("deep_nested_query", |b| {
        b.to_async(&rt).iter(|| async {
            let response = server.execute(black_box(r#"
                query {
                    user(id: "1") {
                        id
                        name
                        posts {
                            id
                            title
                            comments {
                                id
                                text
                            }
                        }
                    }
                }
            "#)).await;
            black_box(response)
        });
    });

    group.bench_function("batched_users_posts", |b| {
        b.to_async(&rt).iter(|| async {
            let response = server.execute(black_box(r#"
                query {
                    users {
                        id
                        name
                        posts {
                            id
                            title
                        }
                    }
                }
            "#)).await;
            black_box(response)
        });
    });

    group.bench_function("full_nested_query", |b| {
        b.to_async(&rt).iter(|| async {
            let response = server.execute(black_box(r#"
                query {
                    users {
                        id
                        name
                        email
                        posts {
                            id
                            title
                            content
                            comments {
                                id
                                text
                                authorId
                            }
                        }
                    }
                }
            "#)).await;
            black_box(response)
        });
    });

    group.finish();
}

fn bench_schema_size_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("schema_size_comparison");

    let schemas: &[(&str, &str)] = &[
        ("simple_117b", SIMPLE_SDL),
        ("complex_614b", COMPLEX_SDL),
        ("deeply_nested_686b", DEEPLY_NESTED_SDL),
        ("ecommerce_2461b", ECOMMERCE_SDL),
        ("social_media_2178b", SOCIAL_MEDIA_SDL),
        ("github_like_4729b", GITHUB_LIKE_SDL),
    ];

    for (name, sdl) in schemas {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("build_time", name),
            sdl,
            |b, sdl| {
                b.iter(|| {
                    let server = GraphQLServer::builder()
                        .sdl(black_box(sdl))
                        .skip_n1_validation()
                        .build();
                    black_box(server)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_sdl_parsing,
    bench_n1_detection,
    bench_registry_lookup,
    bench_server_builder,
    bench_query_execution,
    bench_resolver_context,
    bench_scaling,
    bench_fake_example,
    bench_schema_size_comparison,
);

criterion_main!(benches);
