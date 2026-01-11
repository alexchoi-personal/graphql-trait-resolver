#[derive(Debug, Clone)]
pub struct N1Error {
    pub path: Vec<String>,
    pub field_name: String,
    pub parent_type: String,
    pub message: String,
}

impl std::fmt::Display for N1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "N+1 detected at {}: {}",
            self.path.join("."),
            self.message
        )
    }
}

impl std::error::Error for N1Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_n1_error_display() {
        let error = N1Error {
            path: vec!["Query".to_string(), "users".to_string(), "posts".to_string()],
            field_name: "posts".to_string(),
            parent_type: "User".to_string(),
            message: "Field 'posts' causes N+1".to_string(),
        };

        let display = format!("{}", error);
        assert!(display.contains("Query.users.posts"));
        assert!(display.contains("Field 'posts' causes N+1"));
    }

    #[test]
    fn test_n1_error_empty_path() {
        let error = N1Error {
            path: vec![],
            field_name: "field".to_string(),
            parent_type: "Type".to_string(),
            message: "error".to_string(),
        };

        let display = format!("{}", error);
        assert!(display.contains("error"));
    }

    #[test]
    fn test_n1_error_debug() {
        let error = N1Error {
            path: vec!["Query".to_string()],
            field_name: "field".to_string(),
            parent_type: "Type".to_string(),
            message: "msg".to_string(),
        };

        let debug = format!("{:?}", error);
        assert!(debug.contains("N1Error"));
    }

    #[test]
    fn test_n1_error_clone() {
        let error = N1Error {
            path: vec!["Query".to_string()],
            field_name: "field".to_string(),
            parent_type: "Type".to_string(),
            message: "msg".to_string(),
        };

        let cloned = error.clone();
        assert_eq!(cloned.path, error.path);
        assert_eq!(cloned.field_name, error.field_name);
    }

    #[test]
    fn test_n1_error_is_error() {
        let error = N1Error {
            path: vec![],
            field_name: "f".to_string(),
            parent_type: "T".to_string(),
            message: "m".to_string(),
        };

        let err: &dyn std::error::Error = &error;
        assert!(err.source().is_none());
    }
}
