#![forbid(unsafe_code)]

mod registry;

pub use registry::{BlockStateSchema, ComponentSchema, Registrar, Registry};
pub use semver::{Version, VersionReq};

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct BuiltinMod {
    pub id: &'static str,
    pub version: Version,
    pub dependencies: Vec<Dependency>,
    pub register: fn(&mut Registrar<'_>) -> Result<(), String>,
}

#[derive(Clone)]
pub struct Dependency {
    pub id: &'static str,
    pub version: VersionReq,
}

/// Identity of the statically linked content active in this world, in registration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedMod {
    pub id: &'static str,
    pub version: Version,
}

pub fn resolve(mods: &[BuiltinMod]) -> Result<(Vec<LockedMod>, Registry), String> {
    let mut by_id = BTreeMap::new();
    for module in mods {
        if by_id.insert(module.id, module).is_some() {
            return Err(format!("duplicate mod {}", module.id));
        }
    }
    for module in by_id.values() {
        for dependency in &module.dependencies {
            let target = by_id
                .get(dependency.id)
                .ok_or_else(|| format!("{} requires missing mod {}", module.id, dependency.id))?;
            if !dependency.version.matches(&target.version) {
                return Err(format!(
                    "{} requires {} {}, found {}",
                    module.id, dependency.id, dependency.version, target.version
                ));
            }
        }
    }

    let mut locked = Vec::with_capacity(mods.len());
    let mut resolved = BTreeSet::new();
    let mut registry = Registry::default();
    while !by_id.is_empty() {
        let Some((&id, module)) = by_id.iter().find(|(_, module)| {
            module
                .dependencies
                .iter()
                .all(|dependency| resolved.contains(dependency.id))
        }) else {
            return Err(format!("cyclic mod dependencies among {:?}", by_id.keys()));
        };
        (module.register)(&mut Registrar::new(id, &mut registry))
            .map_err(|error| format!("mod {id}: {error}"))?;
        locked.push(LockedMod {
            id,
            version: module.version.clone(),
        });
        resolved.insert(id);
        by_id.remove(id);
    }
    Ok((locked, registry))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(id: &'static str, dependencies: &[&'static str]) -> BuiltinMod {
        BuiltinMod {
            id,
            version: Version::new(1, 0, 0),
            dependencies: dependencies
                .iter()
                .map(|&id| Dependency {
                    id,
                    version: VersionReq::parse("^1").unwrap(),
                })
                .collect(),
            register: |_| Ok(()),
        }
    }

    #[test]
    fn resolves_dependencies_in_stable_order() {
        let mods = [module("a", &["b"]), module("c", &[]), module("b", &[])];
        let (locked, _) = resolve(&mods).unwrap();
        assert_eq!(
            locked.iter().map(|module| module.id).collect::<Vec<_>>(),
            ["b", "a", "c"]
        );
    }

    #[test]
    fn rejects_missing_incompatible_and_cyclic_dependencies() {
        let mut a = module("a", &["b"]);
        assert!(resolve(&[a.clone()]).err().unwrap().contains("missing"));
        assert!(
            resolve(&[a.clone(), module("b", &["a"])])
                .err()
                .unwrap()
                .contains("cyclic")
        );
        a.dependencies[0].version = VersionReq::parse("^2").unwrap();
        assert!(
            resolve(&[a, module("b", &[])])
                .err()
                .unwrap()
                .contains("found 1.0.0")
        );
    }
}

/// Stable within a world. ECS adapters own the mapping to ephemeral handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityId(u64);

impl EntityId {
    pub const fn from_persistent_id(id: u64) -> Self {
        Self(id)
    }

    pub const fn persistent_id(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentValue {
    pub version: u32,
    pub fields: Vec<i64>,
}
