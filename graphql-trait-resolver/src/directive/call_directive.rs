use rustc_hash::FxHashMap;

use async_graphql_parser::types::ConstDirective;
use async_graphql_value::ConstValue;

use super::{get_directive_argument, get_string_argument};
use crate::config::ArgumentMapping;

#[derive(Debug, Clone)]
pub(crate) struct CallDirective {
    pub trait_name: String,
    pub args: FxHashMap<String, ArgumentMapping>,
}

pub(crate) fn parse_call_directive(directive: &ConstDirective) -> Option<CallDirective> {
    if directive.name.node.as_str() != "call" {
        return None;
    }

    let trait_name = get_string_argument(directive, "trait")?;
    let args = parse_call_args(directive);

    Some(CallDirective { trait_name, args })
}

fn parse_call_args(directive: &ConstDirective) -> FxHashMap<String, ArgumentMapping> {
    let mut args = FxHashMap::default();

    if let Some(ConstValue::Object(obj)) = get_directive_argument(directive, "args") {
        for (key, value) in obj.iter() {
            if let Some(mapping) = parse_argument_mapping(value) {
                args.insert(key.to_string(), mapping);
            }
        }
    }

    args
}

fn parse_argument_mapping(value: &ConstValue) -> Option<ArgumentMapping> {
    match value {
        ConstValue::String(s) => {
            if let Some(field) = s.strip_prefix("$parent.") {
                Some(ArgumentMapping::ParentField(field.to_string()))
            } else if let Some(arg) = s.strip_prefix("$arg.") {
                Some(ArgumentMapping::Argument(arg.to_string()))
            } else {
                Some(ArgumentMapping::Literal(const_value_to_json(value)))
            }
        }
        _ => Some(ArgumentMapping::Literal(const_value_to_json(value))),
    }
}

fn const_value_to_json(value: &ConstValue) -> serde_json::Value {
    match value {
        ConstValue::Null => serde_json::Value::Null,
        ConstValue::Boolean(b) => serde_json::Value::Bool(*b),
        ConstValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                serde_json::Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        }
        ConstValue::String(s) => serde_json::Value::String(s.clone()),
        ConstValue::Enum(e) => serde_json::Value::String(e.to_string()),
        ConstValue::List(arr) => {
            serde_json::Value::Array(arr.iter().map(const_value_to_json).collect())
        }
        ConstValue::Object(obj) => {
            let map: serde_json::Map<String, serde_json::Value> = obj
                .iter()
                .map(|(k, v)| (k.to_string(), const_value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        ConstValue::Binary(b) => {
            serde_json::Value::Array(b.iter().map(|byte| serde_json::Value::Number((*byte).into())).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql_parser::Pos;
    use async_graphql_parser::Positioned;
    use async_graphql_value::indexmap::IndexMap;
    use async_graphql_value::Name;

    fn make_positioned<T>(node: T) -> Positioned<T> {
        Positioned::new(node, Pos::default())
    }

    fn make_name(name: &str) -> Positioned<Name> {
        make_positioned(Name::new(name))
    }

    fn make_directive(name: &str, args: Vec<(&str, ConstValue)>) -> ConstDirective {
        let mut arguments = Vec::new();
        for (arg_name, value) in args {
            arguments.push((make_name(arg_name), make_positioned(value)));
        }
        ConstDirective {
            name: make_name(name),
            arguments,
        }
    }

    #[test]
    fn test_parse_call_directive_wrong_name() {
        let directive = make_directive("trait", vec![]);
        assert!(parse_call_directive(&directive).is_none());
    }

    #[test]
    fn test_parse_call_directive_missing_trait() {
        let directive = make_directive("call", vec![]);
        assert!(parse_call_directive(&directive).is_none());
    }

    #[test]
    fn test_parse_call_directive_basic() {
        let directive = make_directive(
            "call",
            vec![("trait", ConstValue::String("getUser".to_string()))],
        );
        let result = parse_call_directive(&directive);
        assert!(result.is_some());
        let call = result.unwrap();
        assert_eq!(call.trait_name, "getUser");
        assert!(call.args.is_empty());
    }

    #[test]
    fn test_parse_call_directive_with_parent_field_arg() {
        let mut obj = IndexMap::new();
        obj.insert(Name::new("userId"), ConstValue::String("$parent.id".to_string()));

        let directive = make_directive(
            "call",
            vec![
                ("trait", ConstValue::String("getProfile".to_string())),
                ("args", ConstValue::Object(obj)),
            ],
        );

        let result = parse_call_directive(&directive).unwrap();
        assert_eq!(result.trait_name, "getProfile");

        match result.args.get("userId").unwrap() {
            ArgumentMapping::ParentField(field) => assert_eq!(field, "id"),
            _ => panic!("Expected ParentField"),
        }
    }

    #[test]
    fn test_parse_call_directive_with_arg_mapping() {
        let mut obj = IndexMap::new();
        obj.insert(Name::new("id"), ConstValue::String("$arg.userId".to_string()));

        let directive = make_directive(
            "call",
            vec![
                ("trait", ConstValue::String("resolver".to_string())),
                ("args", ConstValue::Object(obj)),
            ],
        );

        let result = parse_call_directive(&directive).unwrap();
        match result.args.get("id").unwrap() {
            ArgumentMapping::Argument(arg) => assert_eq!(arg, "userId"),
            _ => panic!("Expected Argument"),
        }
    }

    #[test]
    fn test_parse_call_directive_with_literal_string() {
        let mut obj = IndexMap::new();
        obj.insert(Name::new("name"), ConstValue::String("literal value".to_string()));

        let directive = make_directive(
            "call",
            vec![
                ("trait", ConstValue::String("resolver".to_string())),
                ("args", ConstValue::Object(obj)),
            ],
        );

        let result = parse_call_directive(&directive).unwrap();
        match result.args.get("name").unwrap() {
            ArgumentMapping::Literal(val) => assert_eq!(val, "literal value"),
            _ => panic!("Expected Literal"),
        }
    }

    #[test]
    fn test_const_value_to_json_null() {
        let result = const_value_to_json(&ConstValue::Null);
        assert_eq!(result, serde_json::Value::Null);
    }

    #[test]
    fn test_const_value_to_json_boolean() {
        assert_eq!(const_value_to_json(&ConstValue::Boolean(true)), serde_json::Value::Bool(true));
        assert_eq!(const_value_to_json(&ConstValue::Boolean(false)), serde_json::Value::Bool(false));
    }

    #[test]
    fn test_const_value_to_json_number_i64() {
        use async_graphql_value::Number;
        let num = Number::from(42i64);
        let result = const_value_to_json(&ConstValue::Number(num));
        assert_eq!(result, serde_json::json!(42));
    }

    #[test]
    fn test_const_value_to_json_number_u64() {
        use async_graphql_value::Number;
        let num = Number::from(42u64);
        let result = const_value_to_json(&ConstValue::Number(num));
        assert_eq!(result, serde_json::json!(42));
    }

    #[test]
    fn test_const_value_to_json_number_f64() {
        use async_graphql_value::Number;
        let num = Number::from_f64(3.14).unwrap();
        let result = const_value_to_json(&ConstValue::Number(num));
        assert!(matches!(result, serde_json::Value::Number(_)));
    }

    #[test]
    fn test_const_value_to_json_string() {
        let result = const_value_to_json(&ConstValue::String("test".to_string()));
        assert_eq!(result, serde_json::Value::String("test".to_string()));
    }

    #[test]
    fn test_const_value_to_json_enum() {
        let result = const_value_to_json(&ConstValue::Enum(Name::new("ACTIVE")));
        assert_eq!(result, serde_json::Value::String("ACTIVE".to_string()));
    }

    #[test]
    fn test_const_value_to_json_list() {
        let list = vec![ConstValue::Boolean(true), ConstValue::Boolean(false)];
        let result = const_value_to_json(&ConstValue::List(list));
        assert_eq!(result, serde_json::json!([true, false]));
    }

    #[test]
    fn test_const_value_to_json_object() {
        let mut obj = IndexMap::new();
        obj.insert(Name::new("key"), ConstValue::String("value".to_string()));
        let result = const_value_to_json(&ConstValue::Object(obj));
        assert_eq!(result, serde_json::json!({"key": "value"}));
    }

    #[test]
    fn test_const_value_to_json_binary() {
        let binary = bytes::Bytes::from(vec![1u8, 2, 3]);
        let result = const_value_to_json(&ConstValue::Binary(binary));
        assert_eq!(result, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_call_directive_debug() {
        let call = CallDirective {
            trait_name: "test".to_string(),
            args: FxHashMap::default(),
        };
        let debug = format!("{:?}", call);
        assert!(debug.contains("CallDirective"));
    }

    #[test]
    fn test_call_directive_clone() {
        let call = CallDirective {
            trait_name: "test".to_string(),
            args: FxHashMap::default(),
        };
        let cloned = call.clone();
        assert_eq!(cloned.trait_name, call.trait_name);
    }

    #[test]
    fn test_parse_call_args_non_object() {
        let directive = make_directive(
            "call",
            vec![
                ("trait", ConstValue::String("resolver".to_string())),
                ("args", ConstValue::String("not an object".to_string())),
            ],
        );

        let result = parse_call_directive(&directive).unwrap();
        assert!(result.args.is_empty());
    }

    #[test]
    fn test_parse_literal_number_arg() {
        let mut obj = IndexMap::new();
        obj.insert(Name::new("count"), ConstValue::Number(async_graphql_value::Number::from(10i64)));

        let directive = make_directive(
            "call",
            vec![
                ("trait", ConstValue::String("resolver".to_string())),
                ("args", ConstValue::Object(obj)),
            ],
        );

        let result = parse_call_directive(&directive).unwrap();
        match result.args.get("count").unwrap() {
            ArgumentMapping::Literal(val) => assert_eq!(val, &serde_json::json!(10)),
            _ => panic!("Expected Literal"),
        }
    }
}
