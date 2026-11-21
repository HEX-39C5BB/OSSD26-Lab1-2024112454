#![forbid(unsafe_code)]

mod builtin;
pub mod checkpoint;
mod chunk_view;
#[cfg(any(debug_assertions, feature = "dev-tools"))]
mod debug_interaction;
mod dimension;
mod entity;
mod interaction;
mod inventory;
pub mod java_block_ticks;
pub mod java_bud;
mod java_comparator;
mod java_controls;
mod java_ejector;
mod java_items;
mod java_observer;
mod java_redstone;
mod java_repeater;
pub mod java_signal;
mod java_torch;
mod java_wire;
mod player;
mod random;
mod redstone;
mod rules;
mod schedule;
mod seed;
mod tasks;
mod time;

use std::collections::BTreeMap;
use std::sync::Arc;

use chunk_view::ChunkView;
use dimension::Dimension;
use player::Player;
use rustcraft_mod_api::{BuiltinMod, LockedMod, Registry};
use rustcraft_world::{BlockPos, BlockStateId, Chunk, ChunkPos, VerticalRange};

pub use chunk_view::ViewDistance;
#[cfg(any(debug_assertions, feature = "dev-tools"))]
pub use debug_interaction::{DebugInteraction, DebugInteractionEvent, DebugView};
pub use entity::EntitySnapshot;
pub use inventory::{
    Inventory, InventoryAction, InventoryButton, ItemStack, JAVA_HOTBAR_COUNT, JAVA_SLOT_COUNT,
    SLOT_COUNT, STACK_LIMIT,
};
pub use java_items::DroppedItem;
pub use java_redstone::{
    JavaBlockTickKind, JavaNotification, PistonAction, PistonEvent, PistonKind, PistonTrace,
    PistonTraceStage,
};
pub use player::{BlockAction, BlockPlacement, BlockTarget, PlayerInput, PlayerSnapshot};
pub use random::{RandomScope, RandomStreamKey, SystemRandom};
pub use redstone::WireConnection;
pub use rules::{RulePreset, RuleVersion};
pub use rustcraft_mod_api::{ComponentValue, EntityId};
pub use seed::{SeedScope, WorldSeeds};
pub use tasks::{ScheduledOperation, ScheduledTask, TaskEvent};
pub use time::{GAME_TICK_INTERVAL, GAME_TICKS_PER_SECOND, JAVA_REDSTONE_TICK_GAME_TICKS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SessionId(u64);

/// A capability set supplied by the trusted in-process host.
#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    pub edit_blocks: bool,
    pub edit_entities: bool,
    pub manage_chunks: bool,
}

impl Capabilities {
    pub const P1_HOST: Self = Self {
        edit_blocks: true,
        edit_entities: true,
        manage_chunks: true,
    };
}

#[derive(Clone, Copy)]
enum SessionAccess {
    Host(Capabilities),
    LocalPlayer,
    LocalOperator,
}

impl SessionAccess {
    fn allows(self, mutation: &Mutation) -> bool {
        match mutation {
            Mutation::ControlPlayer(_)
            | Mutation::UseBlock { .. }
            | Mutation::CancelMining
            | Mutation::EditInventory { .. } => self.controls_player(),
            Mutation::PlayerCommand(_) => matches!(self, Self::LocalOperator),
            #[cfg(any(debug_assertions, feature = "dev-tools"))]
            Mutation::DebugUseBlock(_) => matches!(self, Self::LocalOperator),
            Mutation::SetBlock { .. }
            | Mutation::NotifyJavaBlock { .. }
            | Mutation::NotifyJavaNeighborsExcept { .. }
            | Mutation::NotifyJavaComparatorOutput { .. } => {
                matches!(self, Self::Host(grants) if grants.edit_blocks)
            }
            Mutation::SetJavaEjectorInventory { .. } => {
                matches!(self, Self::Host(grants) if grants.edit_blocks)
            }
            Mutation::SetChunkActive { .. } => {
                matches!(self, Self::Host(grants) if grants.manage_chunks)
            }
            Mutation::SpawnEntity { .. }
            | Mutation::DespawnEntity { .. }
            | Mutation::SetComponent { .. } => {
                matches!(self, Self::Host(grants) if grants.edit_entities)
            }
        }
    }

    fn controls_player(self) -> bool {
        matches!(self, Self::LocalPlayer | Self::LocalOperator)
    }
}

struct Session {
    access: SessionAccess,
    last_operation: u64,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub session: SessionId,
    pub operation: u64,
    pub mutation: Mutation,
}

#[derive(Debug, Clone)]
pub enum Mutation {
    ControlPlayer(PlayerInput),
    UseBlock {
        action: BlockAction,
        target: BlockTarget,
    },
    CancelMining,
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    DebugUseBlock(DebugInteraction),
    EditInventory {
        action: InventoryAction,
        expected_revision: u64,
    },
    PlayerCommand(PlayerCommand),
    SetBlock {
        position: BlockPos,
        state: BlockStateId,
        expected_revision: u64,
    },
    NotifyJavaBlock {
        target: BlockPos,
        from: java_signal::Face,
        kind: JavaNotification,
    },
    NotifyJavaNeighborsExcept {
        source: BlockPos,
        excluded: java_signal::Face,
    },
    NotifyJavaComparatorOutput {
        source: BlockPos,
    },
    SetJavaEjectorInventory {
        position: BlockPos,
        expected_chunk_revision: u64,
        slots: [Option<ItemStack>; 9],
    },
    SetChunkActive {
        position: ChunkPos,
        active: bool,
    },
    SpawnEntity {
        position: BlockPos,
    },
    DespawnEntity {
        entity: EntityId,
        expected_revision: u64,
    },
    SetComponent {
        entity: EntityId,
        expected_revision: u64,
        name: String,
        value: ComponentValue,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum Coordinate {
    Absolute(f64),
    Relative(f64),
    Local(f64),
}

#[derive(Debug, Clone)]
pub enum PlayerCommand {
    Teleport([Coordinate; 3]),
    TeleportFacing {
        position: [Coordinate; 3],
        rotation: [Coordinate; 2],
    },
    Give {
        item: String,
        count: u16,
    },
    GameMode(GameMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameMode {
    #[default]
    Creative,
    Survival,
}

impl GameMode {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Creative => "creative",
            Self::Survival => "survival",
        }
    }
}

impl std::str::FromStr for GameMode {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "creative" => Ok(Self::Creative),
            "survival" => Ok(Self::Survival),
            _ => Err("expected creative or survival"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Unchanged,
    PlayerInputAccepted,
    InventoryChanged,
    PlayerTeleported,
    GameModeChanged(GameMode),
    MiningStarted,
    ChunkChanged(ChunkPos),
    EntityChanged(EntityId),
    Spawned(EntityId),
    Despawned(EntityId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    UnknownSession,
    InvalidOperation,
    DuplicateOperation,
    MissingCapability,
    UnknownBlock,
    UnexpectedBlockState { actual: BlockStateId },
    InactiveChunk,
    ChunkOccupied,
    StaleChunk { actual: u64 },
    UnknownEntity,
    StaleEntity { actual: u64 },
    InvalidComponent,
    InvalidPlayerInput,
    PlayerUnavailable,
    NoBlockTarget,
    StaleBlockTarget,
    PlacementBlocked,
    StaleInventory { actual: u64 },
    InvalidInventoryAction,
    EmptySelectedSlot,
    Unbreakable,
    InventoryFull,
    OutOfWorld,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ack {
    pub session: SessionId,
    pub operation: u64,
    pub result: Result<Effect, Rejection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub tick: u64,
    pub seed: u64,
    pub chunks: BTreeMap<ChunkPos, Arc<Chunk>>,
    pub entities: Vec<EntitySnapshot>,
    pub dropped_items: Vec<(EntityId, DroppedItem)>,
    pub player: Option<PlayerSnapshot>,
    pub inventory: Option<Inventory>,
    pub height: Option<VerticalRange>,
    pub game_mode: GameMode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TickReport {
    pub acks: Vec<Ack>,
    pub snapshot: Snapshot,
    pub sounds: Vec<WorldSound>,
    pub due_tasks: Vec<ScheduledTask>,
    pub task_events: Vec<TaskEvent>,
    pub piston_events: Vec<PistonTrace>,
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    pub block_updates: Vec<BlockUpdate>,
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    pub redstone_updates: Vec<RedstoneUpdate>,
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    pub debug_interactions: Vec<DebugInteractionEvent>,
}

#[cfg(any(debug_assertions, feature = "dev-tools"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockUpdate {
    pub position: BlockPos,
    pub before: BlockStateId,
    pub after: BlockStateId,
}

#[cfg(any(debug_assertions, feature = "dev-tools"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedstoneUpdate {
    pub source: BlockPos,
    pub position: BlockPos,
    pub power: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundKind {
    Place,
    Break,
    Step,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldSound {
    pub kind: SoundKind,
    pub position: BlockPos,
}

/// Runs one dimension in one world instance.
pub struct Server {
    tick: u64,
    seed: u64,
    rules: RulePreset,
    dimension: Dimension,
    registry: Registry,
    locked_mods: Vec<LockedMod>,
    sessions: BTreeMap<SessionId, Session>,
    next_session: u64,
    next_entity: u64,
    player: Option<EntityId>,
    distance_since_step: f64,
    player_tick: u64,
    schedule: Vec<schedule::Phase>,
    chunk_view: Option<ChunkView>,
    random_streams: BTreeMap<RandomStreamKey, u64>,
    tasks: tasks::TaskQueue,
    java_redstone: Option<java_redstone::JavaRedstone>,
    mining: Option<interaction::Mining>,
    saved_game_mode: Option<GameMode>,
}

impl Server {
    pub fn bootstrap(seed: u64) -> Result<Self, String> {
        Self::with_builtins(seed, &[])
    }

    pub fn with_builtins(seed: u64, extensions: &[BuiltinMod]) -> Result<Self, String> {
        Self::boot(seed, extensions, None, RulePreset::NativeV1)
    }

    pub fn playable(seed: u64) -> Result<Self, String> {
        Self::playable_with_range(seed, VerticalRange::FLAT_DEFAULT)
    }

    pub fn playable_with_range(seed: u64, range: VerticalRange) -> Result<Self, String> {
        Self::playable_with_rules(seed, range, RulePreset::NativeV1)
    }

    pub fn playable_with_rules(
        seed: u64,
        range: VerticalRange,
        rules: RulePreset,
    ) -> Result<Self, String> {
        match rules {
            RulePreset::NativeV1 if range.min > -32 || range.max_exclusive < 4 => {
                return Err(
                    "flat world must contain its terrain and spawn volume (-32 <= y < 4)".into(),
                );
            }
            _ if rules.is_java_flat() && range != rules.default_range() => {
                return Err("java-1.20.1 requires -64 <= y < 320".into());
            }
            _ => {}
        }
        Self::boot(seed, &[], Some(range), rules)
    }

    fn boot(
        seed: u64,
        extensions: &[BuiltinMod],
        height: Option<VerticalRange>,
        rules: RulePreset,
    ) -> Result<Self, String> {
        let mut modules = builtin::modules().to_vec();
        if rules.is_java_flat() {
            modules.push(builtin::java_content(rules.uses_java_block_events()));
        }
        modules.extend_from_slice(extensions);
        let (locked_mods, registry) = rustcraft_mod_api::resolve(&modules)?;
        let block = |name| registry.block_id(name).expect("builtin terrain block");
        let empty = block("rustcraft:empty");
        let generator = height.map(|_| rules.create_world_generator(&registry));
        let starter = height.map(|_| match rules {
            RulePreset::NativeV1 => Inventory::new([
                block("rustcraft_content:basalt"),
                block("rustcraft_content:loam"),
                block("rustcraft_content:lichenstone"),
            ]),
            RulePreset::JavaFlatV1 | RulePreset::JavaFlatV2 | RulePreset::JavaFlatV3 => {
                Inventory::empty(JAVA_SLOT_COUNT)
            }
        });
        let mut dimension = Dimension::new(empty, seed, generator, height);
        if rules.is_java_flat() {
            dimension.non_solid = redstone::non_solid(&registry);
            if rules.uses_java_block_events() {
                dimension.java_redstone_edits = Some(Default::default());
            } else {
                dimension.redstone_edits = Some(Default::default());
            }
        }
        let mut server = Self {
            tick: 0,
            seed,
            rules,
            dimension,
            registry,
            locked_mods,
            sessions: BTreeMap::new(),
            next_session: 1,
            next_entity: 1,
            player: None,
            distance_since_step: 0.0,
            player_tick: 0,
            schedule: rules.build_tick_schedule()?,
            chunk_view: None,
            random_streams: BTreeMap::new(),
            tasks: tasks::TaskQueue::default(),
            java_redstone: rules
                .uses_java_block_events()
                .then(java_redstone::JavaRedstone::default),
            mining: None,
            saved_game_mode: None,
        };
        if let Some(inventory) = starter {
            for x in -1..=1 {
                for z in -1..=1 {
                    server.dimension.set_active(ChunkPos { x, z }, true);
                }
            }
            let entity = server.allocate_entity();
            let spawn_y = match rules {
                RulePreset::NativeV1 => 2.0,
                RulePreset::JavaFlatV1 | RulePreset::JavaFlatV2 | RulePreset::JavaFlatV3 => {
                    rustcraft_world::generation::ClassicSuperflat::SPAWN_Y as f64
                }
            };
            server
                .dimension
                .entities
                .spawn_player(entity, Player::new(spawn_y), inventory);
            server.player = Some(entity);
        } else {
            server.dimension.set_active(ChunkPos::ORIGIN, true);
        }
        Ok(server)
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn rules(&self) -> RulePreset {
        self.rules
    }

    /// Select the mode for a playable world launch. An explicit mode takes priority.
    pub fn configure_game_mode(&mut self, requested: Option<GameMode>) -> GameMode {
        let mode = requested.or(self.saved_game_mode).unwrap_or_default();
        self.saved_game_mode = Some(mode);
        mode
    }

    pub fn game_mode(&self) -> GameMode {
        self.saved_game_mode.unwrap_or(GameMode::Survival)
    }

    pub fn creative_items(&self) -> impl Iterator<Item = (BlockStateId, &'static str)> + '_ {
        (0..=u16::MAX)
            .map(BlockStateId::from_registry_index)
            .map_while(|id| self.registry.block(id).map(|block| (id, block.name)))
            .filter(|&(id, name)| {
                !self.dimension.is_empty(id)
                    && self.device_item(id) == id
                    && (!self.rules.is_java_flat() || name.starts_with("rustcraft_java:"))
            })
    }

    pub fn world_seeds(&self) -> WorldSeeds {
        self.rules.world_seeds(self.seed)
    }

    pub fn locked_mods(&self) -> &[LockedMod] {
        &self.locked_mods
    }

    /// Open a trusted Host session with the supplied administrative capabilities.
    pub fn open_local_session(&mut self, capabilities: Capabilities) -> SessionId {
        self.open_session(SessionAccess::Host(capabilities))
    }

    pub fn open_player_session(&mut self) -> Result<SessionId, &'static str> {
        self.open_controlled_player(SessionAccess::LocalPlayer)
    }

    /// The trusted local single-player host can run commands for its own player.
    pub fn open_local_operator(&mut self) -> Result<SessionId, &'static str> {
        self.open_controlled_player(SessionAccess::LocalOperator)
    }

    fn open_controlled_player(&mut self, access: SessionAccess) -> Result<SessionId, &'static str> {
        if self.player.is_none()
            || self
                .sessions
                .values()
                .any(|session| session.access.controls_player())
        {
            return Err("local player unavailable");
        }
        Ok(self.open_session(access))
    }

    fn open_session(&mut self, access: SessionAccess) -> SessionId {
        let id = SessionId(self.next_session);
        self.next_session = self
            .next_session
            .checked_add(1)
            .expect("session identity exhausted");
        self.sessions.insert(
            id,
            Session {
                access,
                last_operation: 0,
            },
        );
        id
    }

    pub fn close_session(&mut self, id: SessionId) {
        if self
            .sessions
            .remove(&id)
            .is_some_and(|session| session.access.controls_player())
        {
            self.clear_player_input();
        }
    }

    pub fn clear_player_input(&mut self) {
        self.mining = None;
        self.distance_since_step = 0.0;
        if let Some((id, mut player)) = self.player_body() {
            player.clear_input();
            self.dimension.entities.set_player(id, player);
        }
    }

    /// Configures the trusted local player's chunk view for subsequent ticks.
    pub fn set_local_view_distance(&mut self, distance: ViewDistance) {
        if self
            .chunk_view
            .as_ref()
            .is_none_or(|view| view.distance != distance)
        {
            self.chunk_view = Some(ChunkView::new(distance));
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            tick: self.tick,
            seed: self.seed,
            chunks: self.dimension.chunks(),
            entities: self.dimension.entities.snapshot(),
            dropped_items: self.dimension.entities.items().collect(),
            player: self.player_body().map(|(id, player)| {
                let target = self.rules.interaction().target(player, &self.dimension);
                player.snapshot(id, target)
            }),
            inventory: self
                .player
                .and_then(|id| self.dimension.entities.inventory(id))
                .cloned(),
            height: self.dimension.height,
            game_mode: self.game_mode(),
        }
    }

    pub fn step(&mut self, requests: Vec<Request>) -> TickReport {
        self.step_local(requests, true, true)
    }

    /// Drive the local host clock. Frozen player frames can still commit input
    /// and stream terrain. Delayed world tasks wait for a world tick.
    pub fn step_local(
        &mut self,
        mut requests: Vec<Request>,
        advance_world: bool,
        simulate_player: bool,
    ) -> TickReport {
        #[cfg(any(debug_assertions, feature = "dev-tools"))]
        let mut debug_interactions = Vec::new();
        if advance_world {
            self.tick = self.tick.checked_add(1).expect("tick counter exhausted");
        }
        if simulate_player {
            self.player_tick = self.player_tick.saturating_add(1);
        }
        let mut acks = Vec::new();
        let mut sounds = Vec::new();
        let mut due_tasks = Vec::new();
        let mut snapshot = None;
        for index in 0..self.schedule.len() {
            let phase = self.schedule[index];
            if let Some(watch) = self
                .java_redstone
                .as_mut()
                .and_then(|logic| logic.bud_watch.as_mut())
            {
                watch.phase = phase.into();
            }
            match phase {
                schedule::Phase::Commit => {
                    requests.sort_by_key(|request| (request.session, request.operation));
                    for group in requests
                        .chunk_by(|a, b| (a.session, a.operation) == (b.session, b.operation))
                    {
                        let result = self.commit(&group[0], group.len() > 1, &mut sounds);
                        if self.java_redstone.is_some() {
                            self.settle_redstone();
                            self.observe_java_bud();
                        }
                        #[cfg(any(debug_assertions, feature = "dev-tools"))]
                        if let Mutation::DebugUseBlock(interaction) = &group[0].mutation {
                            debug_interactions.push(DebugInteractionEvent {
                                position: interaction.position(),
                                action: interaction.action,
                                result: result.clone(),
                            });
                        }
                        acks.extend(group.iter().map(|request| Ack {
                            session: request.session,
                            operation: request.operation,
                            result: result.clone(),
                        }));
                    }
                    if simulate_player {
                        self.advance_mining(&mut sounds);
                    }
                    self.settle_redstone();
                    if advance_world {
                        while let Some((task, operation)) = self.tasks.pop_due(self.tick) {
                            let result = self.execute_scheduled_operation(operation);
                            self.settle_redstone();
                            self.observe_java_bud();
                            due_tasks.push(task);
                            self.tasks.events.push(TaskEvent::Due {
                                task,
                                operation,
                                result,
                            });
                        }
                    }
                }
                schedule::Phase::JavaScheduledBlocks if advance_world => {
                    self.run_java_scheduled_blocks()
                }
                schedule::Phase::JavaBlockEvents if advance_world => self.run_java_block_events(),
                schedule::Phase::SimulatePlayer if simulate_player => {
                    if let Some((id, mut player)) = self.player_body() {
                        let distance = self.rules.movement().step(&mut player, &self.dimension);
                        let stride = 1.35;
                        if distance > 1e-9 {
                            self.distance_since_step += distance;
                            if self.distance_since_step >= stride {
                                self.distance_since_step -= stride;
                                sounds.push(WorldSound {
                                    kind: SoundKind::Step,
                                    position: player.block_position(),
                                });
                            }
                        } else {
                            self.distance_since_step = 0.0;
                        }
                        self.dimension.entities.set_player(id, player);
                        if self.java_redstone.is_some() {
                            self.touch_java_pressure_plates(player);
                        }
                    }
                }
                schedule::Phase::JavaItemMotion if advance_world => self.tick_java_items(),
                schedule::Phase::JavaPistonMotion if advance_world => {
                    self.tick_java_piston_motion()
                }
                schedule::Phase::RefreshChunkView if simulate_player => {
                    if let Some(view) = &mut self.chunk_view
                        && let Some(player) = self
                            .player
                            .and_then(|id| self.dimension.entities.player(id))
                    {
                        view.refresh(&mut self.dimension, player);
                    }
                }
                schedule::Phase::Publish => snapshot = Some(self.snapshot()),
                schedule::Phase::JavaScheduledBlocks
                | schedule::Phase::JavaBlockEvents
                | schedule::Phase::JavaItemMotion
                | schedule::Phase::JavaPistonMotion
                | schedule::Phase::SimulatePlayer
                | schedule::Phase::RefreshChunkView => {}
            }
            self.observe_java_bud();
        }
        TickReport {
            acks,
            snapshot: snapshot.expect("snapshot phase"),
            sounds,
            due_tasks,
            task_events: std::mem::take(&mut self.tasks.events),
            piston_events: self
                .java_redstone
                .as_mut()
                .map_or_else(Vec::new, |logic| std::mem::take(&mut logic.trace)),
            #[cfg(any(debug_assertions, feature = "dev-tools"))]
            block_updates: self.dimension.take_block_updates(),
            #[cfg(any(debug_assertions, feature = "dev-tools"))]
            redstone_updates: std::mem::take(&mut self.dimension.redstone_updates),
            #[cfg(any(debug_assertions, feature = "dev-tools"))]
            debug_interactions,
        }
    }

    fn commit(
        &mut self,
        request: &Request,
        repeated: bool,
        sounds: &mut Vec<WorldSound>,
    ) -> Result<Effect, Rejection> {
        let session = self
            .sessions
            .get_mut(&request.session)
            .ok_or(Rejection::UnknownSession)?;
        if request.operation == 0 {
            return Err(Rejection::InvalidOperation);
        }
        if request.operation <= session.last_operation {
            return Err(Rejection::DuplicateOperation);
        }
        // Rejected requests consume their sequence number as well.
        session.last_operation = request.operation;
        if repeated {
            return Err(Rejection::DuplicateOperation);
        }
        if !session.access.allows(&request.mutation) {
            return Err(Rejection::MissingCapability);
        }

        let result = match &request.mutation {
            #[cfg(any(debug_assertions, feature = "dev-tools"))]
            Mutation::DebugUseBlock(interaction) => {
                return self.run_debug_interaction(*interaction, sounds);
            }
            Mutation::ControlPlayer(input) => {
                let input = input.validated().ok_or(Rejection::InvalidPlayerInput)?;
                let (id, mut player) = self.player_body().ok_or(Rejection::PlayerUnavailable)?;
                self.rules
                    .movement()
                    .set_input(&mut player, input, self.player_tick);
                self.dimension.entities.set_player(id, player);
                Effect::PlayerInputAccepted
            }
            Mutation::UseBlock { action, target } => {
                return self
                    .rules
                    .interaction()
                    .use_block(self, *action, *target, sounds);
            }
            Mutation::CancelMining => {
                self.mining = None;
                Effect::Unchanged
            }
            Mutation::EditInventory {
                action,
                expected_revision,
            } => {
                return self
                    .rules
                    .interaction()
                    .edit_inventory(self, *action, *expected_revision);
            }
            Mutation::PlayerCommand(command) => return self.player_command(command),
            Mutation::SetBlock {
                position,
                state,
                expected_revision,
            } => return self.set_block(*position, *state, *expected_revision),
            Mutation::NotifyJavaBlock { target, from, kind } => {
                return self.notify_java_block(*target, *from, *kind);
            }
            Mutation::NotifyJavaNeighborsExcept { source, excluded } => {
                return self.notify_java_neighbors_except(*source, *excluded);
            }
            Mutation::NotifyJavaComparatorOutput { source } => {
                return self.notify_java_comparator_output(*source);
            }
            Mutation::SetJavaEjectorInventory {
                position,
                expected_chunk_revision,
                slots,
            } => {
                return self.set_java_ejector_inventory(
                    *position,
                    *expected_chunk_revision,
                    *slots,
                );
            }
            Mutation::SetChunkActive { position, active } => {
                return self.set_chunk_active(*position, *active);
            }
            Mutation::SpawnEntity { position } => {
                if !self.dimension.is_active(position.chunk()) {
                    return Err(Rejection::InactiveChunk);
                }
                let id = self.allocate_entity();
                self.dimension.entities.spawn(id, *position);
                Effect::Spawned(id)
            }
            Mutation::DespawnEntity {
                entity,
                expected_revision,
            } => {
                if self.player == Some(*entity) {
                    return Err(Rejection::MissingCapability);
                }
                self.dimension
                    .entities
                    .despawn(*entity, *expected_revision)?;
                Effect::Despawned(*entity)
            }
            Mutation::SetComponent {
                entity,
                expected_revision,
                name,
                value,
            } => {
                let schema = self
                    .registry
                    .component(name)
                    .ok_or(Rejection::InvalidComponent)?;
                if schema.name == builtin::POSITION.name
                    || schema.version != value.version
                    || schema.fields.len() != value.fields.len()
                {
                    return Err(Rejection::InvalidComponent);
                }
                let changed = self.dimension.entities.set_component(
                    *entity,
                    *expected_revision,
                    schema.name,
                    value.clone(),
                )?;
                effect(changed, Effect::EntityChanged(*entity))
            }
        };
        Ok(result)
    }

    fn player_command(&mut self, command: &PlayerCommand) -> Result<Effect, Rejection> {
        match command {
            PlayerCommand::GameMode(mode) => {
                if self.player.is_none() {
                    return Err(Rejection::PlayerUnavailable);
                }
                self.configure_game_mode(Some(*mode));
                self.clear_player_input();
                Ok(Effect::GameModeChanged(*mode))
            }
            PlayerCommand::Teleport(position) => self.teleport_player(*position, None),
            PlayerCommand::TeleportFacing { position, rotation } => {
                self.teleport_player(*position, Some(*rotation))
            }
            PlayerCommand::Give { item, count } => {
                let block = self
                    .registry
                    .block_id(item)
                    .or_else(|| {
                        let name = item.strip_prefix("minecraft:").unwrap_or(item);
                        self.registry.block_id(&format!("rustcraft_java:{name}"))
                    })
                    .filter(|&block| !self.dimension.is_empty(block))
                    .ok_or(Rejection::UnknownBlock)?;
                let (id, mut inventory) = self.player_inventory()?;
                inventory.give(self.device_item(block), *count)?;
                self.dimension.entities.set_inventory(id, inventory);
                Ok(Effect::InventoryChanged)
            }
        }
    }

    fn teleport_player(
        &mut self,
        coordinates: [Coordinate; 3],
        rotation: Option<[Coordinate; 2]>,
    ) -> Result<Effect, Rejection> {
        let (id, mut player) = self.player_body().ok_or(Rejection::PlayerUnavailable)?;
        let current = player.motion();
        let resolve = |coordinate, previous| match coordinate {
            Coordinate::Absolute(value) => value,
            Coordinate::Relative(value) => previous + value,
            Coordinate::Local(_) => f64::NAN,
        };
        let position = if let [
            Coordinate::Local(left),
            Coordinate::Local(up),
            Coordinate::Local(forward),
        ] = coordinates
        {
            let (sy, cy) = current.yaw.sin_cos();
            let (sp, cp) = current.pitch.sin_cos();
            let [x, y, z] = current.position;
            [
                x + left * cy - up * sy * sp + forward * sy * cp,
                y + up * cp + forward * sp,
                z - left * sy - up * cy * sp + forward * cy * cp,
            ]
        } else {
            std::array::from_fn(|axis| resolve(coordinates[axis], current.position[axis]))
        };
        if position
            .iter()
            .any(|value| !(-30_000_000.0..30_000_000.0).contains(value))
        {
            return Err(Rejection::OutOfWorld);
        }
        player.teleport(position);
        if let Some([yaw, pitch]) = rotation {
            // Java command angles increase towards west and down.
            let yaw = resolve(yaw, -current.yaw.to_degrees());
            let pitch = resolve(pitch, -current.pitch.to_degrees());
            if !yaw.is_finite() || !pitch.is_finite() {
                return Err(Rejection::InvalidPlayerInput);
            }
            player.look(
                -yaw.rem_euclid(360.0).to_radians(),
                -pitch.clamp(-90.0, 90.0).to_radians(),
            );
        }
        self.dimension.prepare_player_location(player)?;
        self.dimension.entities.set_player(id, player);
        self.distance_since_step = 0.0;
        self.mining = None;
        Ok(Effect::PlayerTeleported)
    }

    fn set_block(
        &mut self,
        position: BlockPos,
        state: BlockStateId,
        expected_revision: u64,
    ) -> Result<Effect, Rejection> {
        self.registry.block(state).ok_or(Rejection::UnknownBlock)?;
        if !self.dimension.is_passable(state)
            && self
                .player_body()
                .is_some_and(|(_, player)| player.overlaps(position))
        {
            return Err(Rejection::PlacementBlocked);
        }
        let (state, arm_observer) = self.prepare_observer_write(position, state);
        let changed = self.edit_linked_block(
            position,
            state,
            expected_revision,
            java_redstone::EditNotification::HostSetBlock,
        )?;
        if changed && arm_observer {
            self.arm_java_observer(position);
        }
        Ok(effect(changed, Effect::ChunkChanged(position.chunk())))
    }

    fn set_chunk_active(&mut self, position: ChunkPos, active: bool) -> Result<Effect, Rejection> {
        let player = self.player_body().map(|(_, player)| player);
        let changed = self
            .dimension
            .set_active_checked(position, active, player)?;
        if changed && let Some(view) = &mut self.chunk_view {
            view.center = None;
        }
        Ok(effect(changed, Effect::ChunkChanged(position)))
    }

    fn allocate_entity(&mut self) -> EntityId {
        let id = EntityId::from_persistent_id(self.next_entity);
        self.next_entity = self
            .next_entity
            .checked_add(1)
            .expect("entity identity exhausted");
        id
    }

    fn player_body(&self) -> Option<(EntityId, Player)> {
        let id = self.player?;
        self.dimension
            .entities
            .player(id)
            .map(|player| (id, player))
    }

    fn player_inventory(&self) -> Result<(EntityId, Inventory), Rejection> {
        let id = self.player.ok_or(Rejection::PlayerUnavailable)?;
        let inventory = self
            .dimension
            .entities
            .inventory(id)
            .ok_or(Rejection::PlayerUnavailable)?;
        Ok((id, inventory.clone()))
    }
}

fn effect(changed: bool, effect: Effect) -> Effect {
    if changed { effect } else { Effect::Unchanged }
}
