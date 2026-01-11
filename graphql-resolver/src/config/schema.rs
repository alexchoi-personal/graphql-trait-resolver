use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Default)]
pub(crate) struct GraphQLConfig {
    pub types: FxHashMap<String, TypeConfig>,
    pub query_type: Option<String>,
    pub mutation_type: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct TypeConfig {
    pub name: String,
    pub fields: Vec<FieldConfig>,
}

#[derive(Debug, Clone)]
pub(crate) struct FieldConfig {
    pub name: String,
    pub field_type: FieldType,
    pub arguments: Vec<ArgumentConfig>,
    pub resolver: Option<ResolverConfig>,
}

#[derive(Debug, Clone)]
pub(crate) enum FieldType {
    Named(String),
    List(Box<FieldType>),
    NonNull(Box<FieldType>),
}

impl FieldType {
    pub fn inner_type_name(&self) -> Option<&str> {
        match self {
            FieldType::Named(name) => Some(name),
            FieldType::List(inner) => inner.inner_type_name(),
            FieldType::NonNull(inner) => inner.inner_type_name(),
        }
    }

    pub fn is_list(&self) -> bool {
        match self {
            FieldType::List(_) => true,
            FieldType::NonNull(inner) => inner.is_list(),
            FieldType::Named(_) => false,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ArgumentConfig {
    pub name: String,
    pub arg_type: FieldType,
    #[allow(dead_code)]
    pub default_value: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub(crate) enum ResolverConfig {
    Trait {
        name: String,
        batch_key: Option<String>,
    },
    Call {
        trait_name: String,
        args: FxHashMap<String, ArgumentMapping>,
    },
}

impl ResolverConfig {
    pub fn is_batched(&self) -> bool {
        match self {
            ResolverConfig::Trait { batch_key, .. } => batch_key.is_some(),
            ResolverConfig::Call { .. } => false,
        }
    }

    pub fn resolver_name(&self) -> &str {
        match self {
            ResolverConfig::Trait { name, .. } => name,
            ResolverConfig::Call { trait_name, .. } => trait_name,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ArgumentMapping {
    ParentField(String),
    Literal(serde_json::Value),
    Argument(String),
}
