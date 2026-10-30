use anyhow::{Result, bail};

pub(crate) fn identifier(text: &str) -> String {
    let mut result = String::new();
    let mut uppercase = false;
    for character in text.chars() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '$' {
            if character == '_' {
                uppercase = !result.is_empty();
            } else {
                if result.is_empty() && character.is_ascii_digit() {
                    result.push('_');
                }
                result.push(if uppercase { character.to_ascii_uppercase() } else { character });
                uppercase = false;
            }
        } else {
            uppercase = !result.is_empty();
        }
    }
    if let Some(first) = result.get_mut(..1) {
        first.make_ascii_lowercase();
    }
    if result.is_empty() { "resource".into() } else { result }
}

pub(crate) fn validate(name: &str) -> Result<()> {
    if name.is_empty()
        || !name.chars().enumerate().all(|(index, character)| {
            character.is_ascii_alphabetic() || character == '_' || character == '$' || (index > 0 && character.is_ascii_digit())
        })
        || matches!(name, "__proto__" | "prototype" | "constructor" | "then" | "requestFilter")
    {
        bail!("`{name}` is not a safe SDK identifier; choose an explicit resource or method name in extensions.kestrel.codegen");
    }
    Ok(())
}
