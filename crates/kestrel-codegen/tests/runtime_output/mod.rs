use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use kestrel_codegen::OutputOptions;
use kestrel_core::transfer::Progress;

pub fn verify(directory: &Path, collection: &Path, microservices: &Path, compiler: &OsStr) {
    let sdk = directory.join("sdk");
    let other = directory.join("micro-sdk");
    kestrel_codegen::generate_to(collection, &sdk, OutputOptions::default(), false, &Progress::default()).unwrap();
    kestrel_codegen::generate_to(microservices, &other, OutputOptions::default(), false, &Progress::default()).unwrap();
    let valid = directory.join("valid-modules.ts");
    std::fs::write(&valid, include_str!("../valid.ts").replace("./client.ts", "./sdk/index.js")).unwrap();
    let compiled = directory.join("compiled");
    let output = Command::new("node")
        .arg(compiler)
        .args([
            "--strict",
            "--noUnusedLocals",
            "--declaration",
            "--target",
            "ES2022",
            "--lib",
            "ES2022,ESNext.Array,DOM",
            "--module",
            "NodeNext",
            "--outDir",
        ])
        .arg(&compiled)
        .arg(&valid)
        .arg(other.join("index.ts"))
        .output()
        .expect("compile modular SDK");
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/runtime.mjs");
    let output = Command::new("node")
        .arg(script)
        .arg(compiled.join("sdk/index.js"))
        .arg(compiled.join("micro-sdk/index.js"))
        .output()
        .expect("execute modular SDK");
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}
