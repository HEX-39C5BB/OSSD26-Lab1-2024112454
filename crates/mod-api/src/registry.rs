use std::collections::{BTreeMap, BTreeSet};

use rustcraft_world::BlockStateId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockStateSchema {
    pub name: &'static str,
    pub version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentSchema {
    pub name: &'static str,
    pub version: u32,
    /// Ordered integer fields in the P1 component model.
    pub fields: &'static [&'static str],
}

#[derive(Default)]
pub struct Registry {
    blocks: Vec<BlockStateSchema>,
    block_ids: BTreeMap<&'static str, BlockStateId>,
    components: BTreeMap<&'static str, ComponentSchema>,
}

impl Registry {
    pub fn block_id(&self, name: &str) -> Option<BlockStateId> {
        self.block_ids.get(name).copied()
    }

    pub fn block(&self, id: BlockStateId) -> Option<&BlockStateSchema> {
        self.blocks.get(id.registry_index())
    }

    pub fn component(&self, name: &str) -> Option<&ComponentSchema> {
        self.components.get(name)
    }
}

pub struct Registrar<'a> {
    namespace: &'static str,
    registry: &'a mut Registry,
}

impl<'a> Registrar<'a> {
    pub(crate) fn new(namespace: &'static str, registry: &'a mut Registry) -> Self {
        Self {
            namespace,
            registry,
        }
    }

    fn validate_name(&self, name: &str, version: u32) -> Result<(), String> {
        let path = name
            .strip_prefix(self.namespace)
            .and_then(|name| name.strip_prefix(':'));
        if version == 0 || !path.is_some_and(|path| !path.is_empty() && !path.contains(':')) {
            return Err(format!(
                "invalid registration {name}@{version} for {}",
                self.namespace
            ));
        }
        Ok(())
    }

    pub fn block(&mut self, schema: BlockStateSchema) -> Result<(), String> {
        self.validate_name(schema.name, schema.version)?;
        if self.registry.block_ids.contains_key(schema.name) {
            return Err(format!("duplicate block state {}", schema.name));
        }
        let index = u16::try_from(self.registry.blocks.len())
            .map_err(|_| String::from("block state registry is full"))?;
        self.registry
            .block_ids
            .insert(schema.name, BlockStateId::from_registry_index(index));
        self.registry.blocks.push(schema);
        Ok(())
    }

    pub fn component(&mut self, schema: ComponentSchema) -> Result<(), String> {
        self.validate_name(schema.name, schema.version)?;
        let mut fields = BTreeSet::new();
        if !schema
            .fields
            .iter()
            .all(|field| !field.is_empty() && fields.insert(field))
        {
            return Err(format!("invalid fields in component {}", schema.name));
        }
        if self.registry.components.contains_key(schema.name) {
            return Err(format!("duplicate component {}", schema.name));
        }
        self.registry.components.insert(schema.name, schema);
        Ok(())
    }
}
