use rustcraft_world::{BlockPos, BlockStateId};

use crate::dimension::Dimension;
use crate::player::Player;
use crate::{
    BlockAction, BlockTarget, Effect, GameMode, InventoryAction, ItemStack, Rejection, RuleVersion,
    STACK_LIMIT, Server, SoundKind, WorldSound, effect,
};

#[derive(Clone, Copy)]
pub(crate) enum BlockInventory {
    NativeV1,
    JavaSurvivalV1,
}

pub(crate) struct Mining {
    position: BlockPos,
    state: BlockStateId,
    remaining: u16,
}

impl BlockInventory {
    pub(crate) const fn identity(self) -> RuleVersion {
        match self {
            Self::NativeV1 => RuleVersion {
                id: "rustcraft:block_inventory",
                version: 1,
            },
            Self::JavaSurvivalV1 => RuleVersion {
                id: "rustcraft:java_survival_inventory",
                version: 1,
            },
        }
    }

    pub(crate) fn target(self, player: Player, dimension: &Dimension) -> Option<BlockTarget> {
        player.target_v1(dimension)
    }

    pub(crate) fn use_block(
        self,
        server: &mut Server,
        action: BlockAction,
        claimed: BlockTarget,
        sounds: &mut Vec<WorldSound>,
    ) -> Result<Effect, Rejection> {
        if matches!(self, Self::JavaSurvivalV1)
            && server.game_mode() == GameMode::Survival
            && action == BlockAction::Break
        {
            let (_, player) = server.player_body().ok_or(Rejection::PlayerUnavailable)?;
            let actual = player
                .target_v1(&server.dimension)
                .ok_or(Rejection::NoBlockTarget)?;
            if actual.position != claimed.position || actual.revision != claimed.revision {
                return Err(Rejection::StaleBlockTarget);
            }
            let state = server
                .dimension
                .voxel(actual.position)
                .ok_or(Rejection::NoBlockTarget)?
                .0;
            let name = server
                .registry
                .block(state)
                .ok_or(Rejection::UnknownBlock)?
                .name;
            let remaining = match name {
                "rustcraft_java:bedrock" => return Err(Rejection::Unbreakable),
                "rustcraft_java:dirt" => 15,
                "rustcraft_java:grass_block" => 18,
                "rustcraft_java:smooth_stone" => 200,
                _ if server
                    .device(state)
                    .is_some_and(crate::redstone::Device::thin) =>
                {
                    1
                }
                _ => 30,
            };
            server.mining = Some(Mining {
                position: actual.position,
                state,
                remaining,
            });
            return Ok(Effect::MiningStarted);
        }
        use_block_v1(
            server,
            action,
            claimed,
            sounds,
            matches!(self, Self::JavaSurvivalV1),
        )
    }

    pub(crate) fn edit_inventory(
        self,
        server: &mut Server,
        action: InventoryAction,
        expected_revision: u64,
    ) -> Result<Effect, Rejection> {
        let (id, mut inventory) = server.player_inventory()?;
        let creative = server.game_mode() == GameMode::Creative;
        let changed = match action {
            InventoryAction::Catalog { block, stack } if creative => {
                if server.dimension.is_empty(block)
                    || server.registry.block(block).is_none()
                    || server.device_item(block) != block
                {
                    return Err(Rejection::InvalidInventoryAction);
                }
                inventory.set_carried(
                    Some(ItemStack {
                        block,
                        count: if stack { STACK_LIMIT } else { 1 },
                    }),
                    expected_revision,
                )?
            }
            InventoryAction::DiscardCarried if creative => {
                inventory.set_carried(None, expected_revision)?
            }
            _ if creative || matches!(self, Self::JavaSurvivalV1) => {
                inventory.apply_java(action, expected_revision)?
            }
            InventoryAction::ReturnCarried => inventory.apply_java(action, expected_revision)?,
            _ => inventory.apply_v1(action, expected_revision)?,
        };
        if changed {
            server.dimension.entities.set_inventory(id, inventory);
        }
        Ok(effect(changed, Effect::InventoryChanged))
    }
}

impl Server {
    pub(crate) fn advance_mining(&mut self, sounds: &mut Vec<WorldSound>) {
        let Some(mut mining) = self.mining.take() else {
            return;
        };
        let Some((_, player)) = self.player_body() else {
            return;
        };
        let Some(target) = player.target_v1(&self.dimension) else {
            return;
        };
        if target.position != mining.position
            || self
                .dimension
                .voxel(target.position)
                .map(|(state, _)| state)
                != Some(mining.state)
        {
            return;
        }
        mining.remaining -= 1;
        if mining.remaining == 0 {
            let _ = use_block_v1(self, BlockAction::Break, target, sounds, true);
        } else {
            self.mining = Some(mining);
        }
    }
}

fn use_block_v1(
    server: &mut Server,
    action: BlockAction,
    claimed: BlockTarget,
    sounds: &mut Vec<WorldSound>,
    java_survival: bool,
) -> Result<Effect, Rejection> {
    let (_, player) = server.player_body().ok_or(Rejection::PlayerUnavailable)?;
    let actual = player
        .target_v1(&server.dimension)
        .ok_or(Rejection::NoBlockTarget)?;
    if actual.position != claimed.position || actual.revision != claimed.revision {
        return Err(Rejection::StaleBlockTarget);
    }
    apply_block_action(
        server,
        action,
        claimed.placement,
        BlockInteractionView {
            target: actual,
            yaw: player.motion().yaw,
            pitch: player.motion().pitch,
            sneaking: player.sneaking(),
        },
        sounds,
        java_survival,
    )
}

pub(crate) struct BlockInteractionView {
    pub target: BlockTarget,
    pub yaw: f64,
    pub pitch: f64,
    pub sneaking: bool,
}

pub(crate) fn apply_block_action(
    server: &mut Server,
    action: BlockAction,
    expected_placement: Option<crate::BlockPlacement>,
    view: BlockInteractionView,
    sounds: &mut Vec<WorldSound>,
    java_survival: bool,
) -> Result<Effect, Rejection> {
    let actual = view.target;
    let (_, player) = server.player_body().ok_or(Rejection::PlayerUnavailable)?;
    let (hit_state, _) = server
        .dimension
        .voxel(actual.position)
        .ok_or(Rejection::NoBlockTarget)?;
    if java_survival
        && action == BlockAction::Place
        && !view.sneaking
        && let Some(effect) =
            server.use_redstone_device(actual.position, actual.revision, hit_state)?
    {
        return Ok(effect);
    }
    let (id, mut inventory) = server.player_inventory()?;
    let creative = server.game_mode() == GameMode::Creative;
    let (position, revision, state, kind) = match action {
        BlockAction::Break => {
            let block = hit_state;
            let name = server.registry.block(block).expect("registered voxel").name;
            if !creative {
                match (java_survival, name) {
                    (true, "rustcraft_java:bedrock") => return Err(Rejection::Unbreakable),
                    (true, "rustcraft_java:smooth_stone") => {}
                    (true, "rustcraft_java:grass_block") => {
                        let dirt = server
                            .registry
                            .block_id("rustcraft_java:dirt")
                            .expect("java dirt");
                        inventory.insert_v1(dirt)?;
                    }
                    _ => inventory.insert_v1(server.device_item(block))?,
                }
            }
            (
                actual.position,
                actual.revision,
                server.dimension.empty_state(),
                SoundKind::Break,
            )
        }
        BlockAction::Place => {
            if actual.placement != expected_placement {
                return Err(Rejection::StaleBlockTarget);
            }
            let placement = actual.placement.ok_or(Rejection::PlacementBlocked)?;
            if !java_survival && player.overlaps(placement.position) {
                return Err(Rejection::PlacementBlocked);
            }
            let mut state = if creative {
                inventory
                    .selected_item()
                    .ok_or(Rejection::EmptySelectedSlot)?
                    .block
            } else {
                inventory.take_selected_v1()?
            };
            if java_survival {
                use crate::redstone::Device;
                state = match server.device(state) {
                    Some(Device::Torch { .. }) => server
                        .placed_java_torch(
                            placement.position,
                            actual.position,
                            view.yaw,
                            view.pitch,
                        )
                        .ok_or(Rejection::PlacementBlocked)?,
                    Some(Device::Button(button)) => server
                        .orient_java_button(button, placement.position, actual.position, view.yaw)
                        .ok_or(Rejection::PlacementBlocked)?,
                    Some(Device::Lever(_)) if server.rules().uses_java_block_events() => server
                        .orient_java_lever(
                            placement.position,
                            actual.position,
                            view.yaw,
                            view.pitch,
                        )
                        .ok_or(Rejection::PlacementBlocked)?,
                    Some(Device::Lamp(_)) if server.rules().uses_java_block_events() => {
                        Device::Lamp(server.java_neighbor_signal(placement.position) == Some(true))
                            .state(&server.registry)
                    }
                    Some(Device::Dust(..)) if server.rules().uses_java_block_events() => {
                        server.placed_java_wire(placement.position)
                    }
                    _ => server.oriented_device(state, view.yaw, view.pitch),
                };
                if let Some(support) = server
                    .device(state)
                    .and_then(crate::redstone::Device::attachment_face)
                    && !server.has_redstone_attachment(placement.position, support)
                {
                    return Err(Rejection::PlacementBlocked);
                }
            }
            if player.overlaps(placement.position) && !server.dimension.is_passable(state) {
                return Err(Rejection::PlacementBlocked);
            }
            (
                placement.position,
                placement.revision,
                state,
                SoundKind::Place,
            )
        }
        BlockAction::Pick if java_survival || creative => {
            let item = server.device_item(hit_state);
            let changed = if creative {
                inventory.creative_pick(item)
            } else {
                inventory.pick(item)?
            };
            if changed {
                server.dimension.entities.set_inventory(id, inventory);
            }
            return Ok(effect(changed, Effect::InventoryChanged));
        }
        BlockAction::Pick => return Err(Rejection::InvalidInventoryAction),
    };
    // Only publish the staged inventory after the block edit succeeds.
    let changed = server.edit_linked_block(
        position,
        state,
        revision,
        crate::java_redstone::EditNotification::All,
    )?;
    if changed {
        if action == BlockAction::Place && java_survival {
            server.initialize_placed_diode(position);
        }
        if !creative {
            server.dimension.entities.set_inventory(id, inventory);
        }
        sounds.push(WorldSound { kind, position });
    }
    Ok(effect(changed, Effect::ChunkChanged(position.chunk())))
}
