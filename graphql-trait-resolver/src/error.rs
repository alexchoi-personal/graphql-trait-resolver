use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResolverError {
    #[error("Resolver not found: {0}")]
    NotFound(String),
    #[error("Argument error: {0}")]
    Argument(String),
    #[error("Execution error: {0}")]
    Execution(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_error_display() {
        let err = ResolverError::NotFound("myResolver".to_string());
        assert_eq!(err.to_string(), "Resolver not found: myResolver");
    }

    #[test]
    fn test_argument_error_display() {
        let err = ResolverError::Argument("missing id".to_string());
        assert_eq!(err.to_string(), "Argument error: missing id");
    }

    #[test]
    fn test_execution_error_display() {
        let err = ResolverError::Execution("database failed".to_string());
        assert_eq!(err.to_string(), "Execution error: database failed");
    }

    #[test]
    fn test_serialization_error_from() {
        let invalid_json: Result<serde_json::Value, _> = serde_json::from_str("invalid json");
        let json_err = invalid_json.unwrap_err();
        let err: ResolverError = json_err.into();
        assert!(err.to_string().contains("Serialization error"));
    }

    #[test]
    fn test_serialization_error_display() {
        let invalid_json: Result<serde_json::Value, _> = serde_json::from_str("{bad}");
        let json_err = invalid_json.unwrap_err();
        let err = ResolverError::Serialization(json_err);
        let display = err.to_string();
        assert!(display.starts_with("Serialization error:"));
    }

    #[test]
    fn test_resolver_error_debug() {
        let err = ResolverError::NotFound("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("NotFound"));
    }

    #[test]
    fn test_error_trait_impl() {
        let err = ResolverError::Execution("test".to_string());
        let _: &dyn std::error::Error = &err;
    }
}
