//! The third-party crates behind heka.exe.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use serde::Deserialize;

use crate::Error;

/// One third-party crate.
#[derive(Debug, Clone)]
pub struct Crate {
    pub name: String,
    pub version: String,
    /// The SPDX expression in its manifest.
    pub license: Option<String>,
    pub authors: Vec<String>,
    pub repository: Option<String>,
    /// Where its sources are unpacked.
    pub dir: PathBuf,
}

/// The parts of `cargo metadata --format-version 1` used here.
#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    resolve: Resolve,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    license: Option<String>,
    authors: Vec<String>,
    repository: Option<String>,
    /// `None` for a crate of this workspace.
    source: Option<String>,
    manifest_path: PathBuf,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<Dependency>,
}

#[derive(Deserialize)]
struct Dependency {
    pkg: String,
    dep_kinds: Vec<DependencyKind>,
}

#[derive(Deserialize)]
struct DependencyKind {
    /// `None` for a normal dependency, otherwise `"build"` or `"dev"`.
    kind: Option<String>,
}

/// Every third-party crate that the workspace package `root` depends on, directly
/// or not, sorted by name and version. Build dependencies count, because what they
/// generate can end up in the binary. Dev dependencies do not.
///
/// `metadata` should be filtered to the target platform (`--filter-platform`).
pub fn shipped(metadata: &str, root: &str) -> Result<Vec<Crate>, Error> {
    let metadata: Metadata = serde_json::from_str(metadata)?;
    let packages: HashMap<&str, &Package> = metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect();
    let nodes: HashMap<&str, &Node> = metadata
        .resolve
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let root = metadata
        .packages
        .iter()
        .find(|package| package.source.is_none() && package.name == root)
        .ok_or_else(|| format!("{root} is not in the cargo metadata"))?;

    let mut reached = BTreeSet::new();
    let mut queue = vec![root.id.as_str()];
    while let Some(id) = queue.pop() {
        if !reached.insert(id) {
            continue;
        }
        let node = nodes
            .get(id)
            .ok_or_else(|| format!("cargo metadata does not resolve {id}"))?;
        queue.extend(
            node.deps
                .iter()
                .filter(|dep| {
                    dep.dep_kinds
                        .iter()
                        .any(|kind| kind.kind.as_deref() != Some("dev"))
                })
                .map(|dep| dep.pkg.as_str()),
        );
    }

    let mut crates = Vec::new();
    for id in reached {
        let package = packages
            .get(id)
            .ok_or_else(|| format!("cargo metadata does not describe {id}"))?;
        if package.source.is_none() {
            continue;
        }
        let dir = package
            .manifest_path
            .parent()
            .ok_or_else(|| format!("{id} has no directory"))?;
        crates.push(Crate {
            name: package.name.clone(),
            version: package.version.clone(),
            license: package.license.clone(),
            authors: package.authors.clone(),
            repository: package.repository.clone(),
            dir: dir.to_path_buf(),
        });
    }
    crates.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    Ok(crates)
}
