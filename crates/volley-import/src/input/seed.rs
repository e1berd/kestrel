use std::fmt;

use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::{Value as Json, json};

use super::{Input, escape};

pub(super) struct Root<'a>(pub &'a mut Input);

impl<'de> DeserializeSeed<'de> for Root<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for Root<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an API specification object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "paths" | "definitions" | "$defs" | "parameters" | "responses" | "securityDefinitions" => {
                    map.next_value_seed(Nodes { input: self.0, prefix: format!("/{key}") })?;
                    if key == "paths" {
                        self.0.metadata["paths"] = json!({});
                    }
                }
                "components" => map.next_value_seed(Components(self.0))?,
                "tags" => map.next_value_seed(Tags(self.0))?,
                "apiCollection" | "httpCollection" | "schemaCollection" | "dataSchemaCollection" => {
                    map.next_value_seed(Tree { input: self.0, parent: None, schemas: key.contains("chema") })?;
                    self.0.metadata[key] = json!([]);
                }
                "openapi"
                | "swagger"
                | "apidogProject"
                | "info"
                | "name"
                | "servers"
                | "security"
                | "consumes"
                | "produces"
                | "x-kestrel-base-url"
                | "x-kestrel-variables"
                | "host"
                | "basePath"
                | "schemes" => {
                    self.0.metadata[key] = map.next_value()?;
                }
                "environments"
                | "webhooks"
                | "testSuiteCollection"
                | "testCaseCollection"
                | "apiTestCaseCollection"
                | "webSocketCollection"
                | "socketIOCollection"
                | "docCollection"
                | "securitySchemeCollection" => {
                    map.next_value::<IgnoredAny>()?;
                    self.0.metadata[key] = json!([null]);
                }
                _ => {
                    let value: Json = map.next_value()?;
                    let location = self.0.store.append(&value).map_err(de::Error::custom)?;
                    self.0.nodes.insert(format!("/{}", escape(&key)), location);
                }
            }
        }
        Ok(())
    }
}

struct Tags<'a>(&'a mut Input);
impl<'de> DeserializeSeed<'de> for Tags<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for Tags<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an OpenAPI tags array")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        while let Some(tag) = sequence.next_element::<Json>()? {
            if let Some(name) = tag.get("name").and_then(Json::as_str) {
                let location = self.0.store.append(&tag).map_err(de::Error::custom)?;
                self.0.nodes.insert(format!("/tags/{}", escape(name)), location);
            }
        }
        Ok(())
    }
}

struct Nodes<'a> {
    input: &'a mut Input,
    prefix: String,
}

impl<'de> DeserializeSeed<'de> for Nodes<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for Nodes<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a named definitions map")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some((name, value)) = map.next_entry::<String, Json>()? {
            if self.prefix == "/paths" {
                self.input.paths.push(name.clone());
            }
            let location = self.input.store.append(&value).map_err(de::Error::custom)?;
            self.input.nodes.insert(format!("{}/{}", self.prefix, escape(&name)), location);
        }
        Ok(())
    }
}

struct Components<'a>(&'a mut Input);
impl<'de> DeserializeSeed<'de> for Components<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for Components<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an OpenAPI components map")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(category) = map.next_key::<String>()? {
            map.next_value_seed(Nodes { input: self.0, prefix: format!("/components/{}", escape(&category)) })?;
        }
        Ok(())
    }
}

struct Tree<'a> {
    input: &'a mut Input,
    parent: Option<usize>,
    schemas: bool,
}
impl<'de> DeserializeSeed<'de> for Tree<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for Tree<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an Apidog items array")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        while sequence.next_element_seed(Node { input: self.input, parent: self.parent, schemas: self.schemas })?.is_some() {}
        Ok(())
    }
}

struct Node<'a> {
    input: &'a mut Input,
    parent: Option<usize>,
    schemas: bool,
}
impl<'de> DeserializeSeed<'de> for Node<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for Node<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an Apidog folder or request")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let index = self.input.folders.len();
        self.input.folders.push(None);
        let mut metadata = json!({});
        let mut folder = false;
        while let Some(key) = map.next_key::<String>()? {
            if key == "items" || key == "children" {
                folder = true;
                map.next_value_seed(Tree { input: self.input, parent: Some(index), schemas: self.schemas })?;
            } else {
                metadata[key] = map.next_value()?;
            }
        }
        if folder {
            if !self.schemas {
                self.input.folders[index] = Some((self.parent, metadata));
            }
        } else if self.schemas {
            let schema = metadata.get("schema").unwrap_or(&metadata);
            if let (Some(id), Some(value)) = (metadata.get("id").or_else(|| schema.get("id")), schema.get("jsonSchema")) {
                let id = id.as_str().map(str::to_string).unwrap_or_else(|| id.to_string());
                let location = self.input.store.append(value).map_err(de::Error::custom)?;
                self.input.nodes.insert(format!("/definitions/{}", escape(&id)), location);
            }
        } else {
            let location = self.input.store.append(&metadata).map_err(de::Error::custom)?;
            self.input.requests.push((self.parent, location));
        }
        Ok(())
    }
}
