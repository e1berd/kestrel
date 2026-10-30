use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::naming::identifier;
use crate::{Operation, Resource};

pub(crate) struct Candidate {
    pub resources: Vec<String>,
    pub name: String,
    pub explicit: bool,
    pub source: PathBuf,
    pub operation: Operation,
}

pub(crate) fn resources(mut candidates: Vec<Candidate>, warnings: &mut Vec<String>) -> Result<Resource> {
    let mut root = Resource::default();
    for candidate in &candidates {
        resource(&mut root, &candidate.resources);
    }
    candidates.sort_by(|left, right| right.explicit.cmp(&left.explicit).then_with(|| left.source.cmp(&right.source)));
    let mut deferred = Vec::new();
    for candidate in candidates {
        let node = resource(&mut root, &candidate.resources);
        if available(node, &candidate.name) {
            node.methods.insert(candidate.name, candidate.operation);
        } else if candidate.explicit {
            let previous = node.methods.get(&candidate.name).map(|operation| operation.request.as_str()).unwrap_or("a nested resource");
            bail!(
                "explicit SDK name `{}.{}` for {} conflicts with {previous}; choose a distinct extensions.kestrel.codegen.resource or method",
                candidate.resources.join("."),
                candidate.name,
                candidate.source.display()
            );
        } else {
            deferred.push(candidate);
        }
    }
    for mut candidate in deferred {
        let node = resource(&mut root, &candidate.resources);
        let mut suffix = identifier(&candidate.operation.request);
        suffix[..1].make_ascii_uppercase();
        let base = format!("{}{suffix}", candidate.name);
        let mut name = base.clone();
        let mut index = 2;
        while !available(node, &name) {
            name = format!("{base}{index}");
            index += 1;
        }
        let prefix = candidate.resources.join(".");
        warnings.push(format!(
            "Renamed inferred SDK method {prefix}.{} to {prefix}.{name} for {} to avoid a name conflict.",
            candidate.name,
            candidate.source.display()
        ));
        candidate.operation.transport.id = format!("{prefix}.{name}");
        node.methods.insert(name, candidate.operation);
    }
    Ok(root)
}

fn resource<'a>(root: &'a mut Resource, path: &[String]) -> &'a mut Resource {
    let mut node = root;
    for name in path {
        node = node.resources.entry(name.clone()).or_default();
    }
    node
}

fn available(resource: &Resource, name: &str) -> bool {
    !resource.resources.contains_key(name) && !resource.methods.contains_key(name)
}
