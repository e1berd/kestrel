use indexmap::IndexMap;
use rand::Rng;

pub type Vars = IndexMap<String, String>;

#[derive(Debug, Clone, Default)]
pub struct Scope {
    pub collection: Vars,
    pub environment: Vars,
    pub folders: Vec<Vars>,
    pub request: Vars,
    pub runtime: Vars,
    pub process_env: Vars,
}

impl Scope {
    pub fn get(&self, name: &str) -> Option<String> {
        if let Some(env_name) = name.strip_prefix("process.env.") {
            return self.process_env.get(env_name).cloned().or_else(|| std::env::var(env_name).ok());
        }
        if let Some(value) = dynamic(name) {
            return Some(value);
        }
        self.runtime
            .get(name)
            .or_else(|| self.request.get(name))
            .or_else(|| self.folders.iter().rev().find_map(|f| f.get(name)))
            .or_else(|| self.environment.get(name))
            .or_else(|| self.collection.get(name))
            .cloned()
    }

    pub fn all(&self) -> Vars {
        let mut out = Vars::new();
        let layers = [&self.collection, &self.environment].into_iter().chain(self.folders.iter()).chain([&self.request, &self.runtime]);
        for layer in layers {
            for (k, v) in layer {
                out.insert(k.clone(), v.clone());
            }
        }
        out
    }

    pub fn interpolate(&self, text: &str) -> String {
        let mut current = text.to_string();
        for _ in 0..5 {
            let next = self.interpolate_once(&current);
            if next == current {
                break;
            }
            current = next;
        }
        current
    }

    fn interpolate_once(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            match after.find("}}") {
                Some(end) => {
                    let name = after[..end].trim();
                    match self.get(name) {
                        Some(value) => out.push_str(&value),
                        None => {
                            out.push_str("{{");
                            out.push_str(&after[..end]);
                            out.push_str("}}");
                        }
                    }
                    rest = &after[end + 2..];
                }
                None => {
                    out.push_str(&rest[start..]);
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out
    }

    pub fn missing(&self, text: &str) -> Vec<String> {
        let resolved = self.interpolate(text);
        names(&resolved)
    }
}

pub fn names(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim().to_string();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
        rest = &after[end + 2..];
    }
    out
}

fn dynamic(name: &str) -> Option<String> {
    let mut rng = rand::rng();
    let now = chrono::Utc::now();
    Some(match name {
        "$guid" | "$randomUUID" => uuid::Uuid::new_v4().to_string(),
        "$timestamp" => now.timestamp().to_string(),
        "$isoTimestamp" => now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "$randomInt" => rng.random_range(0..1000).to_string(),
        "$randomBoolean" => rng.random_bool(0.5).to_string(),
        "$randomAlphaNumeric" => {
            const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
            (CHARS[rng.random_range(0..CHARS.len())] as char).to_string()
        }
        "$randomHexColor" => format!("#{:06x}", rng.random_range(0..0x1000000)),
        "$randomEmail" => format!("user{}@example.com", rng.random_range(1000..99999)),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_scope_wins_and_unknown_names_stay() {
        let mut scope = Scope::default();
        scope.collection.insert("baseUrl".into(), "http://127.0.0.1:8000".into());
        scope.environment.insert("baseUrl".into(), "{{host}}/api".into());
        scope.environment.insert("host".into(), "https://dev.example.com".into());
        scope.runtime.insert("token".into(), "abc".into());
        assert_eq!(scope.interpolate("{{baseUrl}}/me?t={{ token }}&x={{nope}}"), "https://dev.example.com/api/me?t=abc&x={{nope}}");
        assert_eq!(scope.missing("{{baseUrl}}/{{nope}}"), vec!["nope".to_string()]);
    }

    #[test]
    fn process_env_comes_from_dotenv() {
        let mut scope = Scope::default();
        scope.process_env.insert("KEY".into(), "secret".into());
        assert_eq!(scope.interpolate("{{process.env.KEY}}"), "secret");
    }
}
