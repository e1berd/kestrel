use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use kestrel_core::Collection;
use kestrel_core::transfer::{Progress, Unit};

use crate::{Generated, OutputOptions, Resource, services, typescript};

pub trait CodegenPlugin: Send + Sync {
    fn name(&self) -> &str;

    fn transform(&self, _resources: &mut Resource) -> Result<()> {
        Ok(())
    }

    fn runtime(&self) -> &str {
        ""
    }
}

#[derive(Default)]
pub struct Generator {
    plugins: Vec<Box<dyn CodegenPlugin>>,
    pub(crate) output: OutputOptions,
}

impl Generator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn plugin(mut self, plugin: impl CodegenPlugin + 'static) -> Self {
        self.plugins.push(Box::new(plugin));
        self
    }

    pub fn output(mut self, options: OutputOptions) -> Self {
        self.output = options;
        self
    }

    pub fn generate_to(&self, source: &Path, destination: &Path, overwrite: bool, progress: &Progress) -> Result<Generated> {
        crate::writing::generate_with(self, source, destination, overwrite, progress)
    }

    pub fn generate(&self, collection: &Collection, progress: &Progress) -> Result<Generated> {
        let (mut resources, warnings) = crate::resources(collection, progress)?;
        let mut names = BTreeSet::new();
        for plugin in &self.plugins {
            progress.check()?;
            if plugin.name().trim().is_empty() || !names.insert(plugin.name()) {
                bail!("generator plugins must have unique nonempty names");
            }
            plugin.transform(&mut resources).with_context(|| format!("run codegen plugin `{}`", plugin.name()))?;
        }
        let mut services = BTreeMap::new();
        services::synchronize(&mut resources, &[], &mut services)?;
        progress.start("Generating TypeScript client", None, Unit::Requests)?;
        let plugins: Vec<_> = self.plugins.iter().map(|plugin| (plugin.name(), plugin.runtime())).collect();
        let (files, methods) = typescript::generate(&resources, &services, &plugins, self.output.single_file, progress)?;
        Ok(Generated {
            files,
            entrypoint: if self.output.single_file { "client.ts".into() } else { "index.ts".into() },
            requests: methods.len(),
            methods,
            warnings,
        })
    }
}
