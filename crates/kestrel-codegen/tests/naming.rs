use std::path::PathBuf;

use kestrel_codegen::{Generated, generate};
use kestrel_core::format::RequestFile;
use kestrel_core::transfer::Progress;
use kestrel_core::{Collection, Document};

struct Fixture(Collection);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kestrel-sdk-names-{}", uuid::Uuid::new_v4()));
        Self(Collection::create(&path, "Names").unwrap())
    }

    fn add(&mut self, name: &str, url: &str, method: Option<&str>) -> PathBuf {
        let mut file = RequestFile::new_http(name, "GET", url);
        if let Some(method) = method {
            file.extra
                .insert("extensions".into(), serde_yaml_ng::from_str(&format!("kestrel:\n  codegen:\n    method: {method}\n")).unwrap());
        }
        self.0.add_request(&self.0.root.clone(), file).unwrap()
    }

    fn generate(&self) -> Generated {
        generate(&self.0, &Progress::default()).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0.root).unwrap();
    }
}

#[test]
fn login_and_identity_requests_to_the_same_endpoint_are_both_generated_without_editing_the_collection() {
    let mut fixture = Fixture::new();
    let login = fixture.add("Keycloak Login", "{{baseUrl}}/api/v1/me", None);
    let identity = fixture.add("Me", "{{baseUrl}}/api/v1/me", None);
    let original_login = std::fs::read(&login).unwrap();
    let original_identity = std::fs::read(&identity).unwrap();
    let generated = fixture.generate();
    assert_eq!(generated.requests, 2);
    assert_eq!(generated.methods, ["me.list", "me.listMe"]);
    assert!(generated.warnings.iter().any(|warning| warning.contains("me.listMe")));
    let (root, _) = kestrel_codegen::resources(&fixture.0, &Progress::default()).unwrap();
    assert_eq!(root.resources["me"].methods["listMe"].transport.id, "me.listMe");
    assert_eq!(std::fs::read(login).unwrap(), original_login);
    assert_eq!(std::fs::read(identity).unwrap(), original_identity);
    assert_eq!(generated.files, fixture.generate().files);
    fixture.0.items.reverse();
    assert_eq!(generated.files, fixture.generate().files);
}

#[test]
fn inferred_names_avoid_reserved_explicit_names_and_repeated_normalized_names() {
    let mut fixture = Fixture::new();
    fixture.add("A", "/users", None);
    fixture.add("B", "/users?active=true", None);
    fixture.add("B!", "/users?active=false", None);
    fixture.add("Z", "/users", Some("listB"));
    assert_eq!(fixture.generate().methods, ["users.list", "users.listB", "users.listB2", "users.listB3"]);
}

#[test]
fn resource_and_method_conflicts_are_resolved_in_either_order() {
    for urls in [["/users", "/users/list"], ["/users/list", "/users"]] {
        let mut fixture = Fixture::new();
        fixture.add("Users", urls[0], None);
        fixture.add("Nested", urls[1], None);
        let generated = fixture.generate();
        assert_eq!(generated.requests, 2);
        assert!(generated.methods.contains(&"users.list.list".into()));
        assert_eq!(generated.warnings.len(), 1);
    }
}

#[test]
fn conflicting_explicit_names_report_both_requests() {
    let mut fixture = Fixture::new();
    fixture.add("All users", "/users", Some("list"));
    fixture.add("Active users", "/users?active=true", Some("list"));
    let error = generate(&fixture.0, &Progress::default()).unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("users.list"));
    assert!(message.contains("All users"));
    assert!(message.contains("Active users"));
}

#[test]
fn explicit_names_can_select_the_public_name_for_duplicate_operations() {
    let mut fixture = Fixture::new();
    fixture.add("All users", "/users", None);
    let path = fixture.add("Active users", "/users?active=true", Some("listActive"));
    let generated = fixture.generate();
    assert_eq!(generated.methods, ["users.list", "users.listActive"]);
    assert!(generated.warnings.is_empty());
    assert!(Document::<RequestFile>::read(&path, true).unwrap().data.http.unwrap().url.contains("active=true"));
}

#[test]
fn explicit_methods_cannot_replace_nested_resources() {
    for nested_first in [false, true] {
        let mut fixture = Fixture::new();
        if nested_first {
            fixture.add("Nested", "/users/list", None);
        }
        fixture.add("All users", "/users", Some("list"));
        if !nested_first {
            fixture.add("Nested", "/users/list", None);
        }
        let error = generate(&fixture.0, &Progress::default()).unwrap_err();
        assert!(format!("{error:#}").contains("conflicts with a nested resource"));
    }
}
