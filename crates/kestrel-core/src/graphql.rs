use anyhow::{Context as _, Result, ensure};
use serde_json::Value;

pub fn parse_variables(source: &str) -> Result<Value> {
    if source.trim().is_empty() {
        return Ok(Value::Null);
    }
    let variables: Value = serde_json::from_str(source).context("GraphQL variables must be valid JSON")?;
    ensure!(variables.is_object() || variables.is_null(), "GraphQL variables must be a JSON object");
    Ok(variables)
}

#[cfg(test)]
mod tests {
    use super::parse_variables;

    #[test]
    fn variables_keep_nested_values_and_allow_an_omitted_map() {
        assert_eq!(parse_variables("").unwrap(), serde_json::Value::Null);
        assert_eq!(parse_variables("null").unwrap(), serde_json::Value::Null);
        assert_eq!(
            parse_variables(r#"{"id":"123","filter":{"tags":["a"]}}"#).unwrap(),
            serde_json::json!({"id": "123", "filter": {"tags": ["a"]}})
        );
    }

    #[test]
    fn invalid_variables_are_rejected_instead_of_being_sent_as_null() {
        for source in ["{", "[1]", "true", "42", "\"string\""] {
            assert!(parse_variables(source).is_err(), "{source}");
        }
    }
}
