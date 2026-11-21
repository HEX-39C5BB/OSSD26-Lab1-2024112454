use std::collections::BTreeMap;

use bevy_ecs::prelude::{Component, Entity, World};
use rustcraft_mod_api::{ComponentValue, EntityId};
use rustcraft_world::BlockPos;

use crate::DroppedItem;
use crate::Rejection;
use crate::inventory::Inventory;
use crate::player::Player;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitySnapshot {
    pub id: EntityId,
    pub revision: u64,
    pub position: BlockPos,
    pub components: BTreeMap<&'static str, ComponentValue>,
}

#[derive(Component)]
struct Position(BlockPos);

#[derive(Component)]
#[component(storage = "SparseSet")]
struct ExtensionComponents(BTreeMap<&'static str, ComponentValue>);

struct ResidentEntity {
    handle: Entity,
    revision: u64,
}

/// Each dimension owns its own store. Handles are private to this adapter.
pub(crate) struct Entities {
    world: World,
    residents: BTreeMap<EntityId, ResidentEntity>,
}

impl Entities {
    pub(crate) fn new() -> Self {
        Self {
            world: World::new(),
            residents: BTreeMap::new(),
        }
    }

    pub(crate) fn spawn(&mut self, id: EntityId, position: BlockPos) {
        let handle = self.world.spawn(Position(position)).id();
        self.residents.insert(
            id,
            ResidentEntity {
                handle,
                revision: 1,
            },
        );
    }

    pub(crate) fn spawn_player(&mut self, id: EntityId, player: Player, inventory: Inventory) {
        let handle = self.world.spawn((player, inventory)).id();
        self.residents.insert(
            id,
            ResidentEntity {
                handle,
                revision: 1,
            },
        );
    }

    pub(crate) fn spawn_item(&mut self, id: EntityId, item: DroppedItem) {
        let handle = self.world.spawn(item).id();
        self.residents.insert(
            id,
            ResidentEntity {
                handle,
                revision: 1,
            },
        );
    }

    pub(crate) fn items(&self) -> impl Iterator<Item = (EntityId, DroppedItem)> + '_ {
        self.residents.iter().filter_map(|(&id, resident)| {
            self.world
                .get::<DroppedItem>(resident.handle)
                .map(|item| (id, *item))
        })
    }

    pub(crate) fn is_item(&self, id: EntityId) -> bool {
        self.residents
            .get(&id)
            .is_some_and(|entity| self.world.get::<DroppedItem>(entity.handle).is_some())
    }

    pub(crate) fn revision(&self, id: EntityId) -> u64 {
        self.residents.get(&id).expect("resident entity").revision
    }

    pub(crate) fn set_item(&mut self, id: EntityId, item: Option<DroppedItem>) {
        if let Some(item) = item {
            let resident = self.residents.get_mut(&id).expect("resident item");
            let mut body = self
                .world
                .get_mut::<DroppedItem>(resident.handle)
                .expect("item body");
            *body = item;
            resident.revision = resident
                .revision
                .checked_add(1)
                .expect("entity revision exhausted");
        } else {
            let resident = self.residents.remove(&id).expect("resident item");
            self.world.despawn(resident.handle);
        }
    }

    pub(crate) fn player(&self, id: EntityId) -> Option<Player> {
        let resident = self.residents.get(&id)?;
        self.world.get::<Player>(resident.handle).copied()
    }

    pub(crate) fn inventory(&self, id: EntityId) -> Option<&Inventory> {
        let resident = self.residents.get(&id)?;
        self.world.get::<Inventory>(resident.handle)
    }

    pub(crate) fn set_inventory(&mut self, id: EntityId, inventory: Inventory) {
        let resident = self.residents.get_mut(&id).expect("local player resident");
        *self
            .world
            .get_mut::<Inventory>(resident.handle)
            .expect("local player inventory") = inventory;
        resident.revision = resident
            .revision
            .checked_add(1)
            .expect("entity revision exhausted");
    }

    pub(crate) fn restore_revision(&mut self, id: EntityId, revision: u64) {
        self.residents
            .get_mut(&id)
            .expect("restored player")
            .revision = revision;
    }

    pub(crate) fn set_player(&mut self, id: EntityId, player: Player) {
        let resident = self.residents.get_mut(&id).expect("local player resident");
        let mut current = self
            .world
            .get_mut::<Player>(resident.handle)
            .expect("local player body");
        if *current != player {
            *current = player;
            resident.revision = resident
                .revision
                .checked_add(1)
                .expect("entity revision exhausted");
        }
    }

    pub(crate) fn despawn(
        &mut self,
        id: EntityId,
        expected_revision: u64,
    ) -> Result<(), Rejection> {
        self.check_revision(id, expected_revision)?;
        let resident = self.residents.remove(&id).expect("checked entity");
        self.world.despawn(resident.handle);
        Ok(())
    }

    pub(crate) fn set_component(
        &mut self,
        id: EntityId,
        expected_revision: u64,
        name: &'static str,
        value: ComponentValue,
    ) -> Result<bool, Rejection> {
        self.check_revision(id, expected_revision)?;
        let resident = self.residents.get_mut(&id).expect("checked entity");
        let mut entity = self.world.entity_mut(resident.handle);
        if let Some(mut components) = entity.get_mut::<ExtensionComponents>() {
            if components.0.get(name) == Some(&value) {
                return Ok(false);
            }
            components.0.insert(name, value);
        } else {
            entity.insert(ExtensionComponents(BTreeMap::from([(name, value)])));
        }
        resident.revision = resident
            .revision
            .checked_add(1)
            .expect("entity revision exhausted");
        Ok(true)
    }

    fn check_revision(&self, id: EntityId, expected: u64) -> Result<(), Rejection> {
        let actual = self
            .residents
            .get(&id)
            .ok_or(Rejection::UnknownEntity)?
            .revision;
        if actual != expected {
            return Err(Rejection::StaleEntity { actual });
        }
        Ok(())
    }

    pub(crate) fn snapshot(&self) -> Vec<EntitySnapshot> {
        self.residents
            .iter()
            .map(|(&id, resident)| {
                let entity = self.world.entity(resident.handle);
                EntitySnapshot {
                    id,
                    revision: resident.revision,
                    position: entity.get::<Player>().map_or_else(
                        || {
                            entity.get::<DroppedItem>().map_or_else(
                                || entity.get::<Position>().expect("entity position").0,
                                |item| item.block_position(),
                            )
                        },
                        |player| player.block_position(),
                    ),
                    components: entity
                        .get::<ExtensionComponents>()
                        .map_or_else(BTreeMap::new, |components| components.0.clone()),
                }
            })
            .collect()
    }
}
