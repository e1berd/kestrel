use std::collections::HashSet;

use serde_yaml_ng::{Mapping, Value};

#[derive(Debug, Clone, Default)]
pub struct Layout {
    spaced: HashSet<String>,
    pub space_new_keys: bool,
}

impl Layout {
    pub fn of(text: &str, space_new_keys: bool) -> Self {
        let mut spaced = HashSet::new();
        let mut previous_blank = false;
        for line in text.lines() {
            let starts_key = line.chars().next().is_some_and(|c| !c.is_whitespace() && c != '#' && c != '-');
            if starts_key && previous_blank {
                if let Some((key, _)) = line.split_once(':') {
                    spaced.insert(key.trim_matches('"').to_string());
                }
            }
            previous_blank = line.trim().is_empty();
        }
        Self { spaced, space_new_keys }
    }

    pub fn fresh(space_keys: bool) -> Self {
        Self { spaced: HashSet::new(), space_new_keys: space_keys }
    }

    fn spaced_before(&self, key: &str, known: bool) -> bool {
        if known { self.spaced.contains(key) } else { self.space_new_keys }
    }
}

pub fn merge(old: &Value, new: Value) -> Value {
    match (old, new) {
        (Value::Mapping(old), Value::Mapping(mut new)) => {
            let mut out = Mapping::new();
            for (key, old_value) in old {
                if let Some(value) = new.remove(key) {
                    out.insert(key.clone(), merge(old_value, value));
                }
            }
            for (key, value) in new {
                out.insert(key, value);
            }
            Value::Mapping(out)
        }
        (Value::Sequence(old), Value::Sequence(new)) => {
            let empty = Value::Null;
            Value::Sequence(new.into_iter().enumerate().map(|(i, value)| merge(old.get(i).unwrap_or(&empty), value)).collect())
        }
        (old @ (Value::Number(_) | Value::Bool(_)), Value::String(text)) if scalar_text(old).as_deref() == Some(text.as_str()) => {
            old.clone()
        }
        (_, new) => new,
    }
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

pub fn emit(doc: &Value, layout: &Layout, old_keys: &HashSet<String>) -> String {
    let mut out = String::new();
    match doc {
        Value::Mapping(map) => {
            for (i, (key, value)) in map.iter().enumerate() {
                let key_text = scalar_text(key).unwrap_or_default();
                if i > 0 && layout.spaced_before(&key_text, old_keys.contains(&key_text)) {
                    out.push('\n');
                }
                emit_entry(&mut out, 0, key, value);
            }
        }
        other => {
            emit_value_inline(&mut out, 0, other);
            out.push('\n');
        }
    }
    out
}

pub fn top_keys(doc: &Value) -> HashSet<String> {
    match doc {
        Value::Mapping(map) => map.keys().filter_map(scalar_text).collect(),
        _ => HashSet::new(),
    }
}

fn indent(out: &mut String, n: usize) {
    out.extend(std::iter::repeat_n(' ', n));
}

fn emit_entry(out: &mut String, level: usize, key: &Value, value: &Value) {
    indent(out, level);
    out.push_str(&scalar(key));
    out.push(':');
    emit_after_colon(out, level, value);
}

fn emit_after_colon(out: &mut String, level: usize, value: &Value) {
    match value {
        Value::Mapping(map) if !map.is_empty() => {
            out.push('\n');
            for (key, value) in map {
                emit_entry(out, level + 2, key, value);
            }
        }
        Value::Sequence(seq) if !seq.is_empty() => {
            out.push('\n');
            emit_sequence(out, level + 2, seq);
        }
        Value::String(text) if is_block(text) => {
            out.push(' ');
            emit_block(out, level + 2, text);
        }
        Value::Tagged(tagged) => {
            out.push(' ');
            out.push_str(&tagged.tag.to_string());
            emit_after_colon(out, level, &tagged.value);
        }
        other => {
            out.push(' ');
            emit_value_inline(out, level, other);
            out.push('\n');
        }
    }
}

fn emit_sequence(out: &mut String, level: usize, seq: &[Value]) {
    for item in seq {
        indent(out, level);
        out.push('-');
        match item {
            Value::Mapping(map) if !map.is_empty() => {
                for (i, (key, value)) in map.iter().enumerate() {
                    if i == 0 {
                        out.push(' ');
                        out.push_str(&scalar(key));
                        out.push(':');
                        emit_after_colon(out, level + 2, value);
                    } else {
                        emit_entry(out, level + 2, key, value);
                    }
                }
            }
            Value::Sequence(inner) if !inner.is_empty() => {
                out.push('\n');
                emit_sequence(out, level + 2, inner);
            }
            other => emit_after_colon(out, level, other),
        }
    }
}

fn emit_value_inline(out: &mut String, _level: usize, value: &Value) {
    match value {
        Value::Mapping(_) => out.push_str("{}"),
        Value::Sequence(_) => out.push_str("[]"),
        other => out.push_str(&scalar(other)),
    }
}

fn is_block(text: &str) -> bool {
    text.contains('\n')
        && !text.starts_with([' ', '\t'])
        && !text.chars().any(|c| c.is_control() && c != '\n' && c != '\t')
        && !text.lines().any(|line| line.ends_with([' ', '\t']))
}

fn emit_block(out: &mut String, level: usize, text: &str) {
    let trailing = text.len() - text.trim_end_matches('\n').len();
    out.push_str(match trailing {
        0 => "|-",
        1 => "|",
        _ => "|+",
    });
    out.push('\n');
    let body = if trailing == 0 { text } else { &text[..text.len() - 1] };
    for line in body.split('\n') {
        if !line.is_empty() {
            indent(out, level);
            out.push_str(line);
        }
        out.push('\n');
    }
}

pub fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            if needs_quotes(s) {
                quoted(s)
            } else {
                s.clone()
            }
        }
        Value::Mapping(_) => "{}".into(),
        Value::Sequence(_) => "[]".into(),
        Value::Tagged(t) => format!("{} {}", t.tag, scalar(&t.value)),
    }
}

fn needs_quotes(s: &str) -> bool {
    if s.is_empty() || s.starts_with([' ', '\t']) || s.ends_with([' ', '\t', ':']) {
        return true;
    }
    if s.chars().any(|c| c.is_control()) {
        return true;
    }
    let first = s.chars().next().unwrap_or(' ');
    if "[]{}#&*!|>'\"%@`,".contains(first) {
        return true;
    }
    if ["- ", "? ", ": "].iter().any(|p| s.starts_with(p)) || matches!(s, "-" | "?" | ":") {
        return true;
    }
    if s.contains(": ") || s.contains(" #") {
        return true;
    }
    looks_like_other_type(s)
}

fn looks_like_other_type(s: &str) -> bool {
    if matches!(s, "~" | "null" | "Null" | "NULL" | "true" | "True" | "TRUE" | "false" | "False" | "FALSE") {
        return true;
    }
    let lower = s.to_ascii_lowercase();
    let unsigned = lower.trim_start_matches(['-', '+']);
    if matches!(unsigned, ".inf" | ".nan") {
        return true;
    }
    if let Some(hex) = lower.strip_prefix("0x") {
        return !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    if let Some(oct) = lower.strip_prefix("0o") {
        return !oct.is_empty() && oct.chars().all(|c| ('0'..='7').contains(&c));
    }
    s.len() - unsigned.len() <= 1 && is_decimal(unsigned)
}

fn is_decimal(s: &str) -> bool {
    let (mantissa, exponent) = match s.find(['e', 'E']) {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let digits = mantissa.replace('.', "");
    let mantissa_ok = !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) && mantissa.matches('.').count() <= 1;
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.trim_start_matches(['-', '+']);
        !e.is_empty() && e.chars().all(|c| c.is_ascii_digit())
    });
    mantissa_ok && exponent_ok
}

fn quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(text: &str) -> String {
        let doc: Value = serde_yaml_ng::from_str(text).unwrap();
        emit(&doc, &Layout::of(text, true), &top_keys(&doc))
    }

    #[test]
    fn keeps_a_bruno_request_as_it_was() {
        let text = r#"info:
  name: Update Draft
  type: http
  seq: 8
  tags:
    - ticket-operations

http:
  method: PATCH
  url: "{{baseUrl}}/api/v1/tickets/:ticket_id"
  params:
    - name: ticket_id
      value: ""
      type: path
    - name: include_inactive
      value: "false"
      type: query
      disabled: true
  body:
    type: json
    data: |-
      {
        "body": ""
      }
  auth:
    type: oauth2
    pkce: {}

settings:
  encodeUrl: true
  timeout: 0
"#;
        assert_eq!(round_trip(text), text);
    }

    #[test]
    fn quotes_only_what_would_change_type() {
        assert_eq!(scalar(&Value::String("1".into())), "\"1\"");
        assert_eq!(scalar(&Value::String("false".into())), "\"false\"");
        assert_eq!(scalar(&Value::String("http://127.0.0.1:8000".into())), "http://127.0.0.1:8000");
        assert_eq!(scalar(&Value::String("Readiness: database".into())), "\"Readiness: database\"");
        assert_eq!(scalar(&Value::String("2026-10-28T13:18:01.774Z".into())), "2026-10-28T13:18:01.774Z");
        assert_eq!(scalar(&Value::String("policy ('-' is default)".into())), "policy ('-' is default)");
    }

    #[test]
    fn merge_keeps_the_old_order() {
        let old: Value = serde_yaml_ng::from_str("b: 1\na: 2\n").unwrap();
        let new: Value = serde_yaml_ng::from_str("a: 3\nb: 1\nc: 4\n").unwrap();
        assert_eq!(serde_yaml_ng::to_string(&merge(&old, new)).unwrap(), "b: 1\na: 3\nc: 4\n");
    }
}
