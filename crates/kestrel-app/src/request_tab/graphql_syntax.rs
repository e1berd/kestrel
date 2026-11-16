use std::collections::HashSet;
use std::ops::Range;

pub(super) struct Token {
    pub range: Range<usize>,
    pub capture: &'static str,
}

pub(super) fn tokens(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut offset = 0;
    while offset < source.len() {
        let start = offset;
        let character = source[offset..].chars().next().expect("a GraphQL character");
        let capture = match character {
            '#' => {
                offset += source[offset..].find('\n').unwrap_or(source.len() - offset);
                "comment"
            }
            '"' => {
                offset = string_end(source, offset);
                "string"
            }
            '$' | '@' => {
                offset = name_end(source, offset + 1);
                if character == '$' { "variable" } else { "attribute" }
            }
            '0'..='9' | '-' => {
                offset += 1;
                while offset < source.len() && matches!(source.as_bytes()[offset], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-') {
                    offset += 1;
                }
                "number"
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                offset = name_end(source, offset);
                name_capture(&source[start..offset])
            }
            '(' | ')' | '[' | ']' | '{' | '}' => {
                offset += 1;
                "punctuation.bracket"
            }
            ':' | '!' | '=' | '|' | '&' | '.' | ',' => {
                offset += 1;
                "punctuation.delimiter"
            }
            _ => {
                offset += character.len_utf8();
                continue;
            }
        };
        tokens.push(Token { range: start..offset, capture });
    }
    tokens
}

pub(super) fn variable_names(source: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    tokens(source)
        .into_iter()
        .filter(|token| token.capture == "variable" && token.range.len() > 1)
        .map(|token| source[token.range.start + 1..token.range.end].to_string())
        .filter(|name| seen.insert(name.clone()))
        .collect()
}

fn name_end(source: &str, mut offset: usize) -> usize {
    if !source.as_bytes().get(offset).is_some_and(|character| character.is_ascii_alphabetic() || *character == b'_') {
        return offset;
    }
    while offset < source.len() && (source.as_bytes()[offset].is_ascii_alphanumeric() || source.as_bytes()[offset] == b'_') {
        offset += 1;
    }
    offset
}

fn string_end(source: &str, start: usize) -> usize {
    let delimiter = if source[start..].starts_with("\"\"\"") { "\"\"\"" } else { "\"" };
    let mut offset = start + delimiter.len();
    while offset < source.len() {
        if source[offset..].starts_with(delimiter) {
            return offset + delimiter.len();
        }
        let character = source[offset..].chars().next().expect("a GraphQL string character");
        offset += character.len_utf8();
        if character == '\\' && offset < source.len() {
            offset += source[offset..].chars().next().expect("an escaped GraphQL character").len_utf8();
        }
    }
    offset
}

fn name_capture(name: &str) -> &'static str {
    match name {
        "query" | "mutation" | "subscription" | "fragment" | "on" | "schema" | "type" | "interface" | "union" | "enum" | "input"
        | "scalar" | "directive" | "extend" | "implements" | "repeatable" => "keyword",
        "true" | "false" | "null" => "constant",
        _ if name.starts_with(|character: char| character.is_ascii_uppercase()) => "type",
        _ => "property",
    }
}

#[cfg(test)]
mod tests {
    use super::{tokens, variable_names};

    #[test]
    fn strings_comments_and_unicode_do_not_create_variables_or_break_ranges() {
        let source = "# $comment\nquery User($id: ID!) { user(id: $id, text: \"Привет \\\"$ignored\") { name } }";
        assert_eq!(variable_names(source), ["id"]);
        let highlighted = tokens(source);
        for token in &highlighted {
            assert!(source.is_char_boundary(token.range.start));
            assert!(source.is_char_boundary(token.range.end));
        }
        assert!(highlighted.iter().any(|token| token.capture == "keyword" && &source[token.range.clone()] == "query"));
        assert!(highlighted.iter().any(|token| token.capture == "type" && &source[token.range.clone()] == "ID"));
        assert!(highlighted.iter().any(|token| token.capture == "string" && source[token.range.clone()].contains("Привет")));
    }

    #[test]
    fn block_strings_and_unfinished_input_are_highlighted_without_reading_past_the_buffer() {
        let source = "query($id: ID, $limit: Int) { search(text: \"\"\"first\n$notVariable\nlast\"\"\", id: $id, limit: $limit) }";
        assert_eq!(variable_names(source), ["id", "limit"]);
        for unfinished in ["$", "@", "\"", "\"\"\"unfinished", "# comment", "query($id: ID!", "\"привет\\"] {
            for token in tokens(unfinished) {
                assert!(token.range.end <= unfinished.len());
            }
        }
    }
}
