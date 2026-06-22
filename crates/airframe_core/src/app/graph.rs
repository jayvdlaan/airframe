//! Module dependency graph construction and the topological-sort resolver.
//!
//! This module owns the introspective [`ModuleGraph`] (used for visualization
//! and tests), the dependency resolver that determines init/start order, and
//! the optional layer-validation pass guarded by the `layer-check` feature.

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};
use tracing::{debug, error};

use crate::module::Module;

use super::lifecycle::AppBuilder;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModuleNode {
    pub name: &'static str,
    pub provides: Vec<&'static str>,
    pub requires: Vec<&'static str>,
    pub optional_requires: Vec<&'static str>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModuleEdge {
    pub from: &'static str,
    pub to: &'static str,
    pub kind: &'static str, // "requires" | "optional"
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModuleGraph {
    pub nodes: Vec<ModuleNode>,
    pub edges: Vec<ModuleEdge>,
}

impl ModuleGraph {
    pub fn to_dot(&self) -> String {
        let mut s = String::from("digraph modules {\n");
        for n in &self.nodes {
            s.push_str(&format!("  \"{}\";\n", n.name));
        }
        for e in &self.edges {
            if e.kind == "optional" {
                s.push_str(&format!(
                    "  \"{}\" -> \"{}\" [style=dashed,label=optional];\n",
                    e.from, e.to
                ));
            } else {
                s.push_str(&format!("  \"{}\" -> \"{}\";\n", e.from, e.to));
            }
        }
        s.push_str("}\n");
        s
    }
}

impl AppBuilder {
    #[cfg(feature = "layer-check")]
    fn layer_of(module_name: &str) -> u8 {
        // Lower number = lower layer. Unknown modules default to 255 (top), so they rarely flag.
        match module_name {
            // L0
            "core" => 0,
            // L1 primitives
            "crypt" => 1,
            "codec" => 1,
            "compress" => 1,
            "api" => 1,
            "pdata" => 1,
            "sdata" => 1,
            // L2 config/args
            "args" => 2,
            "config" => 2,
            // L3 logging
            "logging" => 3,
            // L4 runtime adapters / IO
            "http-axum-server" => 4,
            "kv" => 4,
            "data" => 4,
            "db" => 4,
            "secrets" => 4,
            "scheduler" => 4,
            "event" => 4,
            "health" => 4,
            "redis" => 4,
            "sqlite" => 4,
            "mysql" => 4,
            "winreg" => 4,
            // L5 integrations/prefab
            "prefab-openapi" => 5,
            // Higher layers (L6+ domain services) belong to downstream
            // super-projects and are intentionally not enumerated here; they
            // default to the top layer, so a higher-layer module depending
            // downward never trips the layer check.
            _ => 255,
        }
    }

    #[cfg(feature = "layer-check")]
    pub(super) fn validate_layers(&self) -> Result<()> {
        let g = self.graph();
        // Build a quick map name->layer (no need to precompute for all nodes; compute on the fly)
        for e in &g.edges {
            let from_l = Self::layer_of(e.from);
            let to_l = Self::layer_of(e.to);
            if from_l < to_l {
                // Upward dependency detected
                error!(
                    target = "airframe_core",
                    from = e.from,
                    to = e.to,
                    from_layer = from_l,
                    to_layer = to_l,
                    kind = e.kind,
                    "layer violation: dependency goes upward"
                );
                bail!(
                    "layer violation: {} (L{}) depends on {} (L{})",
                    e.from,
                    from_l,
                    e.to,
                    to_l
                );
            }
        }
        Ok(())
    }

    pub fn graph(&self) -> ModuleGraph {
        // Build nodes
        let mut nodes: Vec<ModuleNode> = Vec::with_capacity(self.modules.len());
        for m in &self.modules {
            let d = m.descriptor();
            nodes.push(ModuleNode {
                name: d.name,
                provides: d.provides.to_vec(),
                requires: d.requires.to_vec(),
                optional_requires: d.optional_requires.to_vec(),
            });
        }
        // Map provide -> provider names
        let mut providers: std::collections::HashMap<&'static str, Vec<&'static str>> =
            std::collections::HashMap::new();
        for m in &self.modules {
            let d = m.descriptor();
            for p in d.provides {
                providers.entry(p).or_default().push(d.name);
            }
        }
        // Build edges from module to provider module(s)
        let mut edges: Vec<ModuleEdge> = Vec::new();
        for m in &self.modules {
            let d = m.descriptor();
            for r in d.requires {
                if let Some(ps) = providers.get(r) {
                    for &prov in ps {
                        edges.push(ModuleEdge {
                            from: d.name,
                            to: prov,
                            kind: "requires",
                        });
                    }
                }
            }
            for r in d.optional_requires {
                if let Some(ps) = providers.get(r) {
                    for &prov in ps {
                        edges.push(ModuleEdge {
                            from: d.name,
                            to: prov,
                            kind: "optional",
                        });
                    }
                }
            }
        }
        ModuleGraph { nodes, edges }
    }

    pub(super) fn resolve_dependencies(mods: &[Box<dyn Module>]) -> Result<Vec<usize>> {
        use semver::VersionReq;
        // Topo-sort with deterministic order; supports version-ranged requirements.
        debug!(
            target = "airframe_core",
            modules = mods.len(),
            "resolving module order"
        );
        let mut provided: HashSet<&'static str> = HashSet::new();
        // Track versions provided for each capability by modules already ordered
        let mut provided_versions: HashMap<&'static str, Vec<semver::Version>> = HashMap::new();
        let mut remaining: Vec<usize> = (0..mods.len()).collect(); // preserve insertion order
        let mut order: Vec<usize> = Vec::with_capacity(mods.len());

        loop {
            let mut progressed = false;
            let snapshot = remaining.clone(); // keep stable order
            let mut to_remove: Vec<usize> = Vec::new();
            for &idx in &snapshot {
                let d = mods[idx].descriptor();
                // 1) Unversioned requires must be provided
                let requires_ok = d.requires.iter().all(|r| provided.contains(r));
                if !requires_ok {
                    continue;
                }
                // 2) Versioned requires must have at least one matching provider version already available
                let mut version_requires_ok = true;
                for (cap, range_str) in d.requires_with_versions {
                    if let Ok(range) = VersionReq::parse(range_str) {
                        if let Some(versions) = provided_versions.get(cap) {
                            if !versions.iter().any(|v| range.matches(v)) {
                                version_requires_ok = false;
                                break;
                            }
                        } else {
                            version_requires_ok = false;
                            break;
                        }
                    } else {
                        // If range cannot parse, treat as not satisfied and surface later in error
                        version_requires_ok = false;
                        break;
                    }
                }
                if !version_requires_ok {
                    continue;
                }

                // 3) Optional requires ordering bias: if this module optionally depends on a
                // capability that has a provider among the remaining modules (but is not yet
                // provided), defer this module to allow the provider to be ordered first.
                let mut defer_due_to_optional = false;
                if !d.optional_requires.is_empty() {
                    // Pre-compute whether a required optional cap is already provided
                    // Avoid deadlocks: do not defer if the prospective provider has a hard
                    // requirement on any capability provided by this module (A opts for B, but
                    // B requires A) — in that case, initialize A first.
                    let consumer_provides: HashSet<&'static str> =
                        d.provides.iter().cloned().collect();
                    for r in d.optional_requires {
                        if provided.contains(r) {
                            continue;
                        }
                        // Check if any remaining module can provide this capability
                        let mut found_provider_without_backedge = false;
                        for &j in &snapshot {
                            if j == idx {
                                continue;
                            }
                            let dj = mods[j].descriptor();
                            if dj.provides.iter().any(|p| p == r) {
                                // If provider has a hard requires on any of our provided caps, do NOT defer.
                                let provider_requires_consumer =
                                    dj.requires
                                        .iter()
                                        .any(|req_cap| consumer_provides.contains(req_cap))
                                        || dj.requires_with_versions.iter().any(|(req_cap, _)| {
                                            consumer_provides.contains(req_cap)
                                        });
                                if !provider_requires_consumer {
                                    found_provider_without_backedge = true;
                                    break;
                                }
                            }
                        }
                        if found_provider_without_backedge {
                            defer_due_to_optional = true;
                            break;
                        }
                    }
                }
                if defer_due_to_optional {
                    continue;
                }

                // Eligible: record provided caps and versions, remove from remaining, and progress
                for p in d.provides.iter() {
                    provided.insert(*p);
                    provided_versions
                        .entry(*p)
                        .or_default()
                        .push(d.version.clone());
                }
                to_remove.push(idx);
                order.push(idx);
                progressed = true;
            }
            if !to_remove.is_empty() {
                remaining.retain(|i| !to_remove.contains(i));
            }
            if remaining.is_empty() {
                break;
            }
            if !progressed {
                // Compute missing requirements (including versioned) and potential cycle info
                let mut missing: HashMap<&'static str, Vec<String>> = HashMap::new();
                let mut remaining_names: Vec<&'static str> = Vec::new();
                let mut remaining_provides: HashSet<&'static str> = HashSet::new();
                for idx in &remaining {
                    for p in mods[*idx].descriptor().provides {
                        remaining_provides.insert(p);
                    }
                }
                for idx in &remaining {
                    let d = mods[*idx].descriptor();
                    remaining_names.push(d.name);
                    let mut req_missing: Vec<String> = Vec::new();
                    // unversioned missing
                    for r in d.requires.iter().cloned().filter(|r| !provided.contains(r)) {
                        req_missing.push(r.to_string());
                    }
                    // versioned missing
                    for (cap, range_str) in d.requires_with_versions {
                        let parsed = semver::VersionReq::parse(range_str);
                        let ok = match provided_versions.get(cap) {
                            Some(versions) => match &parsed {
                                Ok(req) => versions.iter().any(|v| req.matches(v)),
                                Err(_) => false,
                            },
                            None => false,
                        };
                        if !ok {
                            req_missing.push(format!("{}@{}", cap, range_str));
                        }
                    }
                    if !req_missing.is_empty() {
                        missing.insert(d.name, req_missing);
                    }
                }
                // If every missing unversioned requirement is provided by someone in remaining, and
                // every missing versioned capability has at least a provider (ignoring version) in remaining,
                // we likely have a cycle.
                let mut all_missing_provided_within_remaining = true;
                for reqs in missing.values() {
                    for r in reqs {
                        let cap = r.split('@').next().unwrap_or("");
                        if !remaining_provides.contains(cap) {
                            all_missing_provided_within_remaining = false;
                            break;
                        }
                    }
                    if !all_missing_provided_within_remaining {
                        break;
                    }
                }
                if all_missing_provided_within_remaining && !remaining.is_empty() {
                    error!(target = "airframe_core", remaining = ?remaining_names, "cycle detected among modules");
                    bail!("cycle detected among modules: {:?}", remaining_names);
                } else {
                    error!(target = "airframe_core", missing = ?missing, "unresolved module dependencies");
                    bail!("unresolved module dependencies: {:?}", missing);
                }
            }
        }
        Ok(order)
    }
}
