use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use kestrel_codegen::{OutputOptions, QueryAdapter};
use kestrel_core::transfer::Progress;

pub fn verify(directory: &Path, collection: &Path, microservices: &Path, compiler: &OsStr, query: QueryAdapter, label: &str) {
    let sdk = directory.join("sdk");
    let other = directory.join("micro-sdk");
    kestrel_codegen::generate_to(collection, &sdk, OutputOptions { query, ..Default::default() }, false, &Progress::default()).unwrap();
    kestrel_codegen::generate_to(microservices, &other, OutputOptions { query, ..Default::default() }, false, &Progress::default())
        .unwrap();
    let valid = directory.join("valid-modules.ts");
    let source = valid_source(query).replace("./client.js", "./sdk/index.js");
    std::fs::write(&valid, source).unwrap();
    let output = Command::new("node")
        .arg(compiler)
        .args([
            "--strict",
            "--noUnusedLocals",
            "--noEmit",
            "--target",
            "ES2022",
            "--lib",
            "ES2022,ESNext.Array,DOM",
            "--module",
            "NodeNext",
        ])
        .arg(&valid)
        .arg(other.join("index.d.ts"))
        .output()
        .expect("compile modular SDK");
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/runtime.mjs");
    let output = Command::new("node")
        .env("KESTREL_QUERY_ADAPTER", label)
        .arg(script)
        .arg(sdk.join("index.js"))
        .arg(other.join("index.js"))
        .output()
        .expect("execute modular SDK");
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

pub fn valid_source(query: QueryAdapter) -> String {
    let mut source = include_str!("../valid.ts").to_string();
    if query == QueryAdapter::Vue {
        source.push_str(include_str!("../vue-valid.ts"));
        if let Some(package) = std::env::var_os("KESTREL_TANSTACK_QUERY_VUE") {
            let module = serde_json::to_string(&package.to_string_lossy()).unwrap();
            source.push_str(&format!("\nimport {{ useQuery, useMutation }} from {module};\nconst observed = useQuery(client.users.get.queryOptions<User>({{ userId }}));\nconst observedUser: User | undefined = observed.data.value;\nconst observedList = useQuery(client.users.list.queryOptions<User>());\nconst observedUsers: User[] | undefined = observedList.data.value;\nconst mutate = useMutation(client.users.insert.mutationOptions<User>());\nmutate.mutate({{ body: {{ name: 'Ada' }} }});\nvoid observedUser;\nvoid observedUsers;\n"));
        }
    } else if query.is_enabled() {
        source.push_str(include_str!("../tanstack-valid.ts"));
        if let Some(package) = std::env::var_os("KESTREL_TANSTACK_QUERY_REACT") {
            let module = serde_json::to_string(&package.to_string_lossy()).unwrap();
            source.push_str(&format!("\nimport {{ useQuery, useMutation }} from {module};\nconst observed = useQuery(client.users.get.queryOptions<User>({{ userId: '42' }}));\nconst observedUser: User | undefined = observed.data;\nconst observedList = useQuery(client.users.list.queryOptions<User>());\nconst observedUsers: User[] | undefined = observedList.data;\nconst mutate = useMutation(client.users.insert.mutationOptions<User>());\nmutate.mutate({{ body: {{ name: 'Ada' }} }});\nconst observedMutation: User | undefined = mutate.data;\nvoid observedUser;\nvoid observedUsers;\nvoid observedMutation;\n"));
        }
    }
    source
}
