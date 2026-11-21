use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::{FRAC_PI_2, TAU};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

#[cfg(any(debug_assertions, feature = "dev-tools"))]
use rustcraft_authority::JAVA_REDSTONE_TICK_GAME_TICKS;
use rustcraft_authority::{
    BlockAction, BlockTarget, Capabilities, Effect, GameMode, InventoryAction, InventoryButton,
    ItemStack, JAVA_SLOT_COUNT, Mutation, PlayerInput, PlayerSnapshot, Request, RulePreset, Server,
    Snapshot, ViewDistance,
};
use rustcraft_host::{LocalHost, TickReply, WorldPath};
use rustcraft_scene::assets::{NATIVE_BUNDLE, Pack, TextureAtlas};
use rustcraft_scene::draw::{
    BlockTextures, Colors, Crosshair, FLAT_SKY, Frame, HUD_ACCENT, HUD_TEXT, HudCorner,
    InventoryHit, InventoryLayout, ItemIcon, NATIVE_SKY, PAUSE_MENU_ROWS, PauseMenu, Vertex,
    append_block_outline, append_chunk_border, append_sky, append_terrain, colored_hud,
    command_hud, hotbar, material, pause_menu, perspective, terrain_model,
};
use rustcraft_scene::mesh::{ChunkMesh, MeshJob, MeshMaterials};
use rustcraft_vulkan::{ConsoleKey, Controls, DesktopFrame, FrameTiming, Interaction, MenuKey};
use rustcraft_world::{BlockPos, BlockStateId, ChunkPos, SECTION_EDGE, TerrainModel};

use super::audio::{self, SoundBank, SoundEffects};
use super::settings::{self, ClientSettings};

type Error = Box<dyn std::error::Error>;
const MESH_REBUILDS_PER_FRAME: usize = 4;

#[path = "benchmark.rs"]
mod benchmark;
#[path = "camera.rs"]
mod camera;
#[path = "console.rs"]
mod console;
pub(super) use benchmark::run as terrain_benchmark;
#[path = "streaming.rs"]
mod streaming;
pub(super) use streaming::run as streaming_benchmark;
#[cfg(any(debug_assertions, feature = "dev-tools"))]
#[path = "tick_replay.rs"]
mod tick_replay;

pub(super) fn run(
    smoke: bool,
    pack: Option<&Path>,
    world: Option<&WorldPath>,
    settings_path: Option<PathBuf>,
    preset: Option<RulePreset>,
    game_mode: Option<GameMode>,
) -> Result<(), Error> {
    let (generation, mut server) = match world.map(WorldPath::load).transpose()?.flatten() {
        Some((generation, server)) => {
            if preset.is_some_and(|preset| preset != server.rules()) {
                return Err("preset conflicts with the existing world".into());
            }
            eprintln!(
                "event=checkpoint_loaded generation={generation} tick={}",
                server.snapshot().tick
            );
            (Some(generation), server)
        }
        None => {
            let preset = preset.unwrap_or(RulePreset::NativeV1);
            (
                None,
                Server::playable_with_rules(42, preset.default_range(), preset)?,
            )
        }
    };
    if !smoke {
        let mode = server.configure_game_mode(game_mode);
        eprintln!("event=game_mode mode={}", mode.name());
    }
    let java_pack = Path::new("local-resources/minecraft-java/1.20.1/classic-flat.rpak");
    let pack_path = match pack {
        Some(path) => Some(path),
        None if server.rules().is_java_flat() => {
            if java_pack.is_file() {
                Some(java_pack)
            } else {
                return Err(format!(
                    "Java preset requires {}; run python3 -B tools/stage_java_flat_pack.py \
                     from the repository root or pass --pack FILE",
                    java_pack.display()
                )
                .into());
            }
        }
        None => None,
    };
    let bytes = read_pack(pack_path)?;
    let pack = open_pack(bytes.as_deref())?;
    eprintln!("event=resource_pack id={} path={:?}", pack.id(), pack_path);
    let mut scene = Scene::new(&server, &pack)?;
    let device = if smoke {
        None
    } else {
        match audio::open() {
            Ok(device) => Some(device),
            Err(error) => {
                eprintln!("event=audio_unavailable error={error:?}");
                None
            }
        }
    };
    let settings_path = settings_path.or_else(|| (!smoke).then(settings::default_path).flatten());
    let settings = settings::load_or_default(settings_path.as_deref());
    if !smoke {
        server.set_local_view_distance(settings.view_distance);
        log_view_distance(settings.view_distance);
    }
    let catalog = server.creative_items().collect();
    let host = LocalHost::start_world(server, None)?;
    let mut game = Game::new(&host, settings_path, settings)?;
    game.catalog = catalog;
    if let Some(device) = &device {
        game.sounds = Some(SoundEffects::new(
            device.mixer().clone(),
            SoundBank::new(&pack)?,
            game.settings.audio,
        ));
    }
    let result = rustcraft_vulkan::run_terrain(smoke, |controls, size| {
        let frame_timing = controls.frame_timing;
        let graphics = controls.graphics.clone();
        let surface = controls.surface_format.clone();
        game.frame_size = size;
        game.update(controls)?;
        let snapshot = game.rendered_snapshot();
        let player = snapshot.player.ok_or("missing local player")?;
        let camera = game.free_camera.unwrap_or_else(|| camera::Camera {
            speed: game.player_speed,
            ..camera::Camera::at_player(player)
        });
        let player_block = player.block_position();
        let center = camera.block().chunk();
        let pending_meshes = if smoke {
            scene.update_meshes(snapshot);
            0
        } else {
            scene.update_meshes_budgeted(snapshot, center)
        };
        let mut frame = scene.draw_frame(
            snapshot,
            size,
            camera.eye,
            camera.yaw,
            camera.pitch,
            game.crosshair(),
        );
        if game.show_chunk_border
            && let Some(height) = snapshot.height
        {
            append_chunk_border(&mut frame.terrain, player_block, height, camera.eye);
        }
        if game.menu.is_some() || game.replay_active() {
            frame.hud.clear();
        }
        if !game.replay_active()
            && game.free_camera.is_none()
            && let Some(target) = player.target
        {
            append_block_outline(&mut frame.terrain, target.position, camera.eye);
        }
        if game.show_debug || game.replay_active() {
            if !game.replay_active() {
                let rows = game.debug_player_rows(player, camera);
                frame
                    .hud
                    .extend(diagnostic_panel(size, &rows, HudCorner::TopLeft));
            }
            let mut rows = Vec::from(game.debug_render_rows(
                scene.meshes.len(),
                pending_meshes,
                frame_timing,
                graphics.as_deref(),
                surface.as_deref(),
            ));
            if game.replay_active() {
                let [x, y, z] = camera.eye;
                rows.push(format!(
                    "Camera {x:.2} {y:.2} {z:.2}  {:.2} blocks/s",
                    camera.speed
                ));
                rows.push(format!(
                    "{}  Yaw {:.1}  Pitch {:.1}",
                    camera.direction(),
                    camera.yaw.to_degrees(),
                    camera.pitch.to_degrees()
                ));
            }
            frame
                .hud
                .extend(diagnostic_panel(size, &rows, HudCorner::TopRight));
        }
        #[cfg(any(debug_assertions, feature = "dev-tools"))]
        if game.replay.active {
            if game.replay.at_live_tick() {
                let aimed = snapshot
                    .plan_debug_interaction(game.replay_view(camera), BlockAction::Place)
                    .is_some();
                frame.hud.extend(hotbar(
                    size,
                    &[],
                    0,
                    if aimed {
                        Crosshair::Targeted
                    } else {
                        Crosshair::Idle
                    },
                ));
            }
            game.replay.draw(
                &mut frame,
                size,
                camera.eye,
                host.is_paused(),
                &scene.block_names,
            );
        }
        if game.inventory_open
            && let Some(inventory) = &snapshot.inventory
        {
            let icon = |item: ItemStack| (scene.item_icon(item.block), item.count);
            let slots = inventory
                .slots()
                .iter()
                .map(|item| item.map(icon))
                .collect::<Vec<_>>();
            let catalog = if snapshot.game_mode == GameMode::Creative {
                game.catalog
                    .iter()
                    .map(|&(id, name)| {
                        (
                            name.split_once(':').map_or(name, |(_, local)| local),
                            scene.item_icon(id),
                        )
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            frame.hud = game.inventory_layout(inventory.slots().len()).draw(
                &slots,
                inventory.selected(),
                game.cursor,
                inventory.carried().map(icon),
                &catalog,
            );
        }
        if game.console.open {
            frame.hud.extend(command_hud(
                size,
                &game.console.rows(),
                game.console.inline(),
                game.console.cursor(),
            ));
        }
        if let Some(selection) = game.menu {
            frame.hud.extend(pause_menu(
                size,
                PauseMenu {
                    selected: selection,
                    sensitivity_percent: game.settings.sensitivity_percent,
                    invert_y: game.settings.invert_y,
                    volume_percent: game.settings.audio.volume_percent,
                    muted: game.settings.audio.muted,
                    paused: host.is_paused(),
                    view_distance: game.settings.view_distance.radius(),
                },
            ));
        } else if !game.ui_paused() {
            game.target_shown = player.target;
        }
        Ok(DesktopFrame {
            frame,
            paused: game.cursor_released(),
            console_open: game.console.open,
        })
    });
    let server = host.finish()?;
    result?;
    if let Some(world) = world {
        let saved = rustcraft_persistence::save(world.as_path(), generation, &server)?;
        eprintln!("event=checkpoint_saved generation={saved}");
    }
    Ok(())
}

struct Scene {
    colors: Colors,
    icons: BTreeMap<usize, ItemIcon>,
    empty: BlockStateId,
    meshes: BTreeMap<ChunkPos, ChunkMesh>,
    atlas: Option<Arc<TextureAtlas>>,
    textures: BTreeMap<usize, BlockTextures>,
    materials: MeshMaterials,
    models: BTreeMap<usize, Vec<Vertex>>,
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    block_names: Vec<&'static str>,
}

impl Scene {
    fn new(server: &Server, pack: &Pack<'_>) -> Result<Self, Error> {
        let mut colors: Colors = pack
            .colors()
            .into_iter()
            .filter_map(|(name, color)| {
                server
                    .registry()
                    .block_id(name)
                    .map(|id| (id.registry_index(), color))
            })
            .collect();
        let required = match server.rules() {
            RulePreset::NativeV1 => [
                "rustcraft_content:basalt",
                "rustcraft_content:lichenstone",
                "rustcraft_content:loam",
            ],
            RulePreset::JavaFlatV1 | RulePreset::JavaFlatV2 | RulePreset::JavaFlatV3 => [
                "rustcraft_java:bedrock",
                "rustcraft_java:grass_block",
                "rustcraft_java:dirt",
            ],
        };
        for name in required {
            if let Some(id) = server.registry().block_id(name)
                && !colors.contains_key(&id.registry_index())
            {
                eprintln!("event=missing_terrain_material name={name:?}");
            }
        }
        let empty = server
            .registry()
            .block_id("rustcraft:empty")
            .ok_or("empty block is not registered")?;
        let names: Vec<_> = (0..=u16::MAX)
            .map_while(|index| {
                server
                    .registry()
                    .block(BlockStateId::from_registry_index(index))
                    .map(|item| item.name)
            })
            .collect();
        let atlas = (pack.id() == "rustcraft_java:classic_flat")
            .then(|| pack.atlas().map(Arc::new))
            .transpose()?;
        let mut icons = BTreeMap::new();
        if let Some(atlas) = &atlas {
            for (id, name) in server.creative_items() {
                let Some(name) = name.strip_prefix("rustcraft_java:") else {
                    continue;
                };
                let tile = atlas
                    .tile(&format!("rustcraft_java:icon/{name}"))
                    .ok_or_else(|| {
                        format!(
                            "missing item icon {name}; run python3 -B tools/stage_java_flat_pack.py"
                        )
                    })?;
                icons.insert(id.registry_index(), ItemIcon::Texture(tile));
            }
        }
        let mut textures = BTreeMap::new();
        let mut materials = MeshMaterials::default();
        for (name, color) in [
            ("rustcraft_java:redstone_block", [0.77, 0.07, 0.06]),
            ("rustcraft_java:glass", [0.66, 0.83, 0.86]),
        ] {
            if let Some(id) = server.registry().block_id(name) {
                colors.entry(id.registry_index()).or_insert(color);
            }
        }
        for connection in server.wire_connections() {
            let index = connection.state.registry_index();
            materials.ports.insert(index, connection.faces);
            if connection.passable {
                materials.passable.insert(index);
            }
            if let Some(power) = connection.wire_power {
                materials.wires.insert(index);
                colors.insert(index, redstone_color(f32::from(power)));
            }
            if let Some(geometry) = connection.geometry {
                materials.wire_geometry.insert(index, geometry);
            }
        }
        materials.models.extend(
            server
                .terrain_models()
                .map(|(state, model)| (state.registry_index(), model)),
        );
        let models = materials
            .models
            .iter()
            .map(|(&id, &model)| {
                let color = match model {
                    TerrainModel::Lever { .. } => [0.68, 0.50, 0.29],
                    TerrainModel::Button { .. } | TerrainModel::PressurePlate { .. } => {
                        [0.60, 0.55, 0.48]
                    }
                    TerrainModel::RedstoneTorch { lit: true, .. } => [1.0, 0.31, 0.12],
                    TerrainModel::RedstoneTorch { lit: false, .. } => [0.52, 0.33, 0.16],
                    TerrainModel::Repeater { .. } => [0.72, 0.62, 0.57],
                    TerrainModel::Comparator { .. } => [0.72, 0.62, 0.57],
                    TerrainModel::Cake { .. } => [0.92, 0.83, 0.74],
                    TerrainModel::Ejector { .. } => [0.53, 0.51, 0.49],
                    _ => [0.72, 0.57, 0.32],
                };
                colors.entry(id).or_insert(color);
                (id, terrain_model(model, atlas.as_deref()))
            })
            .collect();
        for (index, name) in names.iter().enumerate() {
            if name.starts_with("rustcraft_java:observer") {
                colors.entry(index).or_insert([0.53, 0.51, 0.49]);
            }
            if materials.models.contains_key(&index) {
                continue;
            }
            let wire_tint = materials.wires.contains(&index).then(|| colors[&index]);
            let Some((tiles, color)) = atlas
                .as_ref()
                .and_then(|atlas| java_texture(atlas, name, wire_tint))
            else {
                continue;
            };
            textures.insert(index, tiles);
            if let Some(color) = color {
                colors.insert(index, color);
            }
        }
        Ok(Self {
            colors,
            icons,
            empty,
            meshes: BTreeMap::new(),
            atlas,
            textures,
            materials,
            models,
            #[cfg(any(debug_assertions, feature = "dev-tools"))]
            block_names: names,
        })
    }

    fn item_icon(&self, block: BlockStateId) -> ItemIcon {
        self.icons
            .get(&block.registry_index())
            .copied()
            .unwrap_or_else(|| ItemIcon::Solid(material(&self.colors, block)))
    }

    fn update_meshes(&mut self, snapshot: &Snapshot) {
        self.meshes
            .retain(|position, _| snapshot.chunks.contains_key(position));
        for &position in snapshot.chunks.keys() {
            self.ensure_mesh(snapshot, position);
        }
    }

    fn update_meshes_budgeted(&mut self, snapshot: &Snapshot, center: ChunkPos) -> usize {
        self.meshes
            .retain(|position, _| snapshot.chunks.contains_key(position));
        let mut dirty: Vec<_> = snapshot
            .chunks
            .keys()
            .copied()
            .filter(|position| {
                self.meshes
                    .get(position)
                    .is_none_or(|mesh| !mesh.is_current(&snapshot.chunks))
            })
            .collect();
        dirty.sort_unstable_by_key(|&position| {
            let x = position.x.abs_diff(center.x);
            let z = position.z.abs_diff(center.z);
            (x.max(z), x.saturating_add(z), position)
        });
        let pending = dirty.len().saturating_sub(MESH_REBUILDS_PER_FRAME);
        for position in dirty.into_iter().take(MESH_REBUILDS_PER_FRAME) {
            self.ensure_mesh(snapshot, position);
        }
        pending
    }

    fn ensure_mesh(&mut self, snapshot: &Snapshot, position: ChunkPos) {
        if self
            .meshes
            .get(&position)
            .is_some_and(|mesh| mesh.is_current(&snapshot.chunks))
        {
            return;
        }
        let replacement = MeshJob::capture(&snapshot.chunks, position, self.empty)
            .expect("active source chunk")
            .build_with_materials(&self.materials);
        self.meshes.insert(position, replacement);
    }

    fn frame(
        &mut self,
        snapshot: &Snapshot,
        size: [u32; 2],
        eye: [f64; 3],
        yaw: f64,
        pitch: f64,
        crosshair: Crosshair,
    ) -> Frame {
        self.update_meshes(snapshot);
        self.draw_frame(snapshot, size, eye, yaw, pitch, crosshair)
    }

    fn draw_frame(
        &self,
        snapshot: &Snapshot,
        size: [u32; 2],
        eye: [f64; 3],
        yaw: f64,
        pitch: f64,
        crosshair: Crosshair,
    ) -> Frame {
        let mut terrain = Vec::new();
        if let Some(atlas) = &self.atlas {
            append_sky(&mut terrain, atlas, eye, snapshot.tick);
        }
        for mesh in self.meshes.values() {
            append_terrain(
                &mut terrain,
                mesh,
                &self.colors,
                &self.textures,
                &self.models,
                eye,
            );
        }
        for (_, item) in &snapshot.dropped_items {
            let block = item.stack.block;
            let [x, y, z] = item.position.map(|axis| axis.floor() as i64);
            if snapshot
                .chunks
                .contains_key(&BlockPos::new(x, y, z).chunk())
            {
                rustcraft_scene::draw::append_dropped_block(
                    &mut terrain,
                    item.position,
                    eye,
                    material(&self.colors, block),
                    self.textures.get(&block.registry_index()),
                );
            }
        }

        let hud = snapshot
            .inventory
            .as_ref()
            .map_or_else(Vec::new, |inventory| {
                let slots = inventory
                    .hotbar()
                    .iter()
                    .map(|item| item.map(|item| (self.item_icon(item.block), item.count)))
                    .collect::<Vec<_>>();
                hotbar(size, &slots, inventory.selected(), crosshair)
            });
        Frame {
            terrain,
            hud,
            view_projection: perspective(yaw, pitch, size[0] as f32 / size[1] as f32),
            atlas: self.atlas.clone(),
            background: if self.atlas.is_some() {
                FLAT_SKY
            } else {
                NATIVE_SKY
            },
        }
    }
}

fn java_texture(
    atlas: &TextureAtlas,
    id: &str,
    wire_tint: Option<[f32; 3]>,
) -> Option<(BlockTextures, Option<[f32; 3]>)> {
    let name = id.strip_prefix("rustcraft_java:")?;
    let asset = |name: &str| atlas.tile(&format!("rustcraft_java:{name}"));
    let directional = observer_texture_faces(name, asset);
    let (top, sides, bottom, tint, icon) = if let Some(faces) = directional {
        (faces[3], faces[5], faces[2], [1.0; 3], None)
    } else if name == "grass_block" {
        (
            asset("block/grass_block_top")?,
            asset("grass_block")?,
            asset("dirt")?,
            [0.48, 0.76, 0.34],
            Some([0.38, 0.57, 0.22]),
        )
    } else if let Some(tint) = wire_tint {
        let tile = asset("block/redstone_dust_dot")?;
        (tile, tile, tile, tint, Some(tint))
    } else {
        let tile = asset(name)?;
        (tile, tile, tile, [1.0; 3], None)
    };
    Some((
        BlockTextures {
            top,
            sides,
            bottom,
            directional,
            top_tint: tint,
            side_overlay: (name == "grass_block")
                .then(|| asset("block/grass_block_side_overlay"))
                .flatten(),
            wire_line: wire_tint.and_then(|_| asset("block/redstone_dust_line0")),
        },
        icon,
    ))
}

fn observer_texture_faces(
    name: &str,
    asset: impl Fn(&str) -> Option<[f32; 4]>,
) -> Option<[[f32; 4]; 6]> {
    let suffix = name.strip_prefix("observer")?;
    let powered = suffix.ends_with("_on");
    let front = match suffix.strip_suffix("_on").unwrap_or(suffix) {
        "" => 5,
        "_north" => 4,
        "_down" => 2,
        "_up" => 3,
        "_west" => 0,
        "_east" => 1,
        _ => return None,
    };
    let mut faces = [asset("block/observer_side")?; 6];
    let top = asset("block/observer_top")?;
    let top_axis = if front / 2 == 1 { 2 } else { 1 };
    faces[top_axis * 2] = top;
    faces[top_axis * 2 + 1] = top;
    faces[front] = asset("block/observer_front")?;
    faces[front ^ 1] = asset(if powered {
        "block/observer_back_on"
    } else {
        "block/observer_back"
    })?;
    Some(faces)
}

#[cfg(test)]
#[test]
fn observer_uses_the_lit_texture_only_on_its_output_face() {
    let asset = |key: &str| match key {
        "block/observer_front" => Some([1.0; 4]),
        "block/observer_back" => Some([2.0; 4]),
        "block/observer_back_on" => Some([3.0; 4]),
        "block/observer_side" => Some([4.0; 4]),
        "block/observer_top" => Some([5.0; 4]),
        _ => None,
    };
    for (front, suffix) in ["_west", "_east", "_down", "_up", "_north", ""]
        .into_iter()
        .enumerate()
    {
        let off = observer_texture_faces(&format!("observer{suffix}"), asset).unwrap();
        let on = observer_texture_faces(&format!("observer{suffix}_on"), asset).unwrap();
        assert_eq!(off[front], [1.0; 4]);
        assert_eq!(off[front ^ 1], [2.0; 4]);
        assert_eq!(on[front ^ 1], [3.0; 4]);
        for face in 0..6 {
            if face != front ^ 1 {
                assert_eq!(on[face], off[face]);
            }
        }
    }
}

fn redstone_color(power: f32) -> [f32; 3] {
    [0.25 + power / 20.0, 0.035, 0.025]
}

struct Game<'a> {
    host: &'a LocalHost,
    snapshot: Snapshot,
    aim: PlayerInput,
    sent: Option<PlayerInput>,
    next_operation: u64,
    target_shown: Option<BlockTarget>,
    pending_clicks: BTreeSet<u64>,
    flash: Option<(Crosshair, Instant)>,
    menu: Option<usize>,
    settings: ClientSettings,
    settings_path: Option<PathBuf>,
    applied_view_distance: ViewDistance,
    sounds: Option<SoundEffects>,
    show_debug: bool,
    show_chunk_border: bool,
    inventory_open: bool,
    catalog: Vec<(BlockStateId, &'static str)>,
    inventory_pending: Option<(u64, bool)>,
    hotbar_target: Option<usize>,
    hotbar_pending: Option<u64>,
    inventory_close: bool,
    cursor: Option<[f64; 2]>,
    frame_size: [u32; 2],
    console: console::Console,
    console_pending: Option<u64>,
    tick_pending: Option<Receiver<TickReply>>,
    mining_target: Option<BlockPos>,
    mining_start: Option<u64>,
    free_camera: Option<camera::Camera>,
    cpu: String,
    player_speed: f64,
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    camera_before_replay: Option<camera::Camera>,
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    replay: tick_replay::TickReplay,
}

impl<'a> Game<'a> {
    fn new(
        host: &'a LocalHost,
        settings_path: Option<PathBuf>,
        settings: ClientSettings,
    ) -> Result<Self, Error> {
        let snapshot = host.initial.clone();
        let player = snapshot.player.ok_or("missing local player")?;
        let aim = PlayerInput {
            yaw: player.yaw,
            pitch: player.pitch,
            ..PlayerInput::default()
        };
        Ok(Self {
            host,
            snapshot,
            aim,
            sent: Some(aim),
            next_operation: 1,
            target_shown: None,
            pending_clicks: BTreeSet::new(),
            flash: None,
            menu: None,
            settings,
            settings_path,
            applied_view_distance: settings.view_distance,
            sounds: None,
            show_debug: false,
            show_chunk_border: false,
            inventory_open: false,
            catalog: Vec::new(),
            inventory_pending: None,
            hotbar_target: None,
            hotbar_pending: None,
            inventory_close: false,
            cursor: None,
            frame_size: [1920, 1080],
            console: console::Console::default(),
            console_pending: None,
            tick_pending: None,
            mining_target: None,
            mining_start: None,
            free_camera: None,
            cpu: cpu_name(),
            player_speed: 0.0,
            #[cfg(any(debug_assertions, feature = "dev-tools"))]
            camera_before_replay: None,
            #[cfg(any(debug_assertions, feature = "dev-tools"))]
            replay: tick_replay::TickReplay::default(),
        })
    }

    fn update(&mut self, mut controls: Controls) -> Result<(), Error> {
        self.show_debug ^= controls.toggle_debug;
        self.show_chunk_border ^= controls.toggle_chunk_border;
        for _ in 0..32 {
            match self.host.updates.try_recv() {
                Ok(report) => {
                    if !self.ui_paused()
                        && let Some(sounds) = &mut self.sounds
                    {
                        for sound in &report.sounds {
                            sounds.play(sound.kind);
                        }
                    }
                    for ack in &report.acks {
                        if matches!(ack.result, Ok(Effect::GameModeChanged(_))) {
                            self.mining_start = None;
                            self.mining_target = None;
                            self.sent = None;
                        }
                        if matches!(ack.result, Ok(Effect::PlayerTeleported))
                            && let Some(player) = report.snapshot.player
                        {
                            self.aim.yaw = player.yaw;
                            self.aim.pitch = player.pitch;
                            self.sent = None;
                        }
                        if self.mining_start == Some(ack.operation) {
                            self.mining_start = None;
                            if ack.result.is_err() {
                                self.mining_target = None;
                            }
                        }
                        if self.console_pending == Some(ack.operation) {
                            self.console_pending = None;
                            self.console.message = match &ack.result {
                                Ok(effect) => format!("OK: {effect:?}"),
                                Err(error) => format!("REJECTED: {error:?}"),
                            };
                        }
                        if self.hotbar_pending == Some(ack.operation) {
                            self.hotbar_pending = None;
                            if ack.result.as_ref().is_err_and(|error| {
                                !matches!(
                                    error,
                                    rustcraft_authority::Rejection::StaleInventory { .. }
                                )
                            }) {
                                self.hotbar_target = None;
                            }
                        }
                        if let Some((operation, closing)) = self.inventory_pending
                            && ack.operation == operation
                        {
                            self.inventory_pending = None;
                            if closing {
                                self.inventory_close = false;
                                self.inventory_open = ack.result.is_err();
                            }
                        }
                        if self.pending_clicks.remove(&ack.operation) && !self.ui_paused() {
                            self.flash(matches!(
                                ack.result,
                                Ok(Effect::ChunkChanged(_)
                                    | Effect::InventoryChanged
                                    | Effect::Unchanged)
                            ));
                        }
                        if ack.result.is_err() {
                            eprintln!(
                                "event=input_rejected operation={} result={:?}",
                                ack.operation, ack.result
                            );
                        }
                    }
                    #[cfg(any(debug_assertions, feature = "dev-tools"))]
                    self.replay.record(&report);
                    let elapsed_ticks = report.snapshot.tick.saturating_sub(self.snapshot.tick);
                    if let (Some(old), Some(new)) = (self.snapshot.player, report.snapshot.player)
                        && elapsed_ticks != 0
                    {
                        self.player_speed = if report
                            .acks
                            .iter()
                            .any(|ack| ack.result == Ok(Effect::PlayerTeleported))
                        {
                            0.0
                        } else {
                            let [x, y, z] =
                                std::array::from_fn(|axis| new.position[axis] - old.position[axis]);
                            x.hypot(y).hypot(z) * 20.0 / elapsed_ticks as f64
                        };
                    }
                    self.snapshot = report.snapshot;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    return Err("integrated authority stopped".into());
                }
            }
        }
        if let Some(reply) = &self.tick_pending {
            match reply.try_recv() {
                Ok(result) => {
                    self.tick_pending = None;
                    self.console.message = match result {
                        Ok(state) => format!(
                            "{:.2} GT/S  {}  STEP {}  SPRINT {}  LAST {:.2} MS",
                            state.rate,
                            if state.frozen { "FROZEN" } else { "RUNNING" },
                            state.stepping,
                            state.sprinting,
                            state.last_tick.as_secs_f64() * 1000.0,
                        ),
                        Err(message) => message.into(),
                    };
                }
                Err(TryRecvError::Disconnected) => {
                    self.tick_pending = None;
                    self.console.message = "AUTHORITY STOPPED".into();
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        let console_keys = std::mem::take(&mut controls.console);
        if self.console.open
            || (!self.inventory_open && self.menu.is_none() && !console_keys.is_empty())
        {
            let opening = !self.console.open;
            if opening {
                self.reset_player_controls()?;
            }
            if !self.host.is_paused() {
                self.submit_neutral_input()?;
            }
            for key in console_keys {
                if !self.console.open && !matches!(key, ConsoleKey::Open) {
                    continue;
                }
                if let Some(command) = self.console.edit(key) {
                    self.apply_console(command)?;
                }
            }
            return Ok(());
        }

        let inventory_toggle = controls.toggle_inventory
            && self.snapshot.inventory.as_ref().is_some_and(|inventory| {
                inventory.slots().len() == JAVA_SLOT_COUNT
                    || self.snapshot.game_mode == GameMode::Creative
            });
        if self.inventory_open || (inventory_toggle && !self.ui_paused()) {
            if !self.inventory_open {
                self.inventory_open = true;
                self.reset_player_controls()?;
                self.submit_neutral_input()?;
                return Ok(());
            }
            self.handle_inventory(&controls, inventory_toggle)?;
            return Ok(());
        }

        #[cfg(any(debug_assertions, feature = "dev-tools"))]
        if self.menu.is_none() {
            if controls.toggle_tick_replay {
                self.set_replay(!self.replay.active)?;
                return Ok(());
            }
            if self.replay.active {
                if self.replay.at_live_tick() {
                    self.select_hotbar(&controls)?;
                }
                self.move_free_camera(&controls);
                self.queue_replay_interaction(&controls);
                for &key in &controls.menu {
                    match key {
                        MenuKey::Toggle => {
                            self.set_replay(false)?;
                            break;
                        }
                        MenuKey::Left => self.replay.previous(),
                        MenuKey::Right => self.replay.next(),
                        MenuKey::Up => self.replay.page(true),
                        MenuKey::Down => self.replay.page(false),
                        MenuKey::Accept if self.replay.at_live_tick() => {
                            self.step_replay()?;
                        }
                        _ => {}
                    }
                }
                return Ok(());
            }
        }

        let was_paused = self.menu.is_some();
        for &key in &controls.menu {
            self.menu_key(key)?;
        }
        if was_paused || self.menu.is_some() {
            return Ok(());
        }

        self.select_hotbar(&controls)?;
        if self.free_camera.is_some() {
            self.submit_neutral_input()?;
            self.move_free_camera(&controls);
            return Ok(());
        }

        let java_survival = self.snapshot.game_mode == GameMode::Survival
            && self
                .snapshot
                .inventory
                .as_ref()
                .is_some_and(|inventory| inventory.slots().len() == JAVA_SLOT_COUNT);
        if java_survival {
            if !controls.break_held {
                self.cancel_mining()?;
            } else if let Some(target) = self.target_shown
                && self.mining_target != Some(target.position)
                && self.mining_start.is_none()
            {
                let operation = self.next_operation;
                if self.submit(Mutation::UseBlock {
                    action: BlockAction::Break,
                    target,
                })? {
                    self.mining_target = Some(target.position);
                    self.mining_start = Some(operation);
                }
            }
        }
        if let Some(action) = controls.interaction
            && !(java_survival && matches!(action, Interaction::Break))
        {
            let action = match action {
                Interaction::Break => BlockAction::Break,
                Interaction::Place => BlockAction::Place,
                Interaction::Pick => BlockAction::Pick,
            };
            let operation = self.next_operation;
            if let Some(target) = self.target_shown
                && self.submit(Mutation::UseBlock { action, target })?
            {
                self.pending_clicks.insert(operation);
            } else {
                self.flash(false);
            }
        }

        self.aim.forward = controls.forward;
        self.aim.strafe = controls.strafe;
        self.aim.jump = controls.jump;
        self.aim.descend = controls.descend;
        if controls.captured {
            self.aim.jump_presses = self
                .aim
                .jump_presses
                .saturating_add(controls.jump_presses)
                .min(2);
        } else {
            self.aim.jump_presses = 0;
        }
        let [yaw, pitch] = self.settings.look_delta(controls.mouse);
        self.aim.yaw = (self.aim.yaw + yaw).rem_euclid(TAU);
        self.aim.pitch = (self.aim.pitch + pitch).clamp(-FRAC_PI_2, FRAC_PI_2);
        if self.sent != Some(self.aim) && self.submit(Mutation::ControlPlayer(self.aim))? {
            self.aim.jump_presses = 0;
            self.sent = Some(self.aim);
        }
        Ok(())
    }

    fn ui_paused(&self) -> bool {
        self.replay_active() || self.menu.is_some() || self.inventory_open || self.console.open
    }

    fn replay_active(&self) -> bool {
        #[cfg(any(debug_assertions, feature = "dev-tools"))]
        return self.replay.active;
        #[cfg(not(any(debug_assertions, feature = "dev-tools")))]
        false
    }

    fn cursor_released(&self) -> bool {
        self.menu.is_some()
            || self.inventory_open
            || self.console.open
            || (self.free_camera.is_none() && self.ui_paused())
    }

    fn move_free_camera(&mut self, controls: &Controls) {
        if let Some(camera) = &mut self.free_camera {
            camera.update(controls, self.settings);
        }
    }

    fn set_free_camera(&mut self, enabled: bool) -> Result<(), Error> {
        if enabled != self.free_camera.is_some() {
            let player = self
                .rendered_snapshot()
                .player
                .ok_or("missing local player")?;
            self.free_camera = enabled.then(|| camera::Camera::at_player(player));
            self.reset_player_controls()?;
        }
        Ok(())
    }

    fn apply_console(&mut self, command: console::Command) -> Result<(), Error> {
        match command {
            console::Command::Player(command) => {
                let operation = self.next_operation;
                if self.submit(Mutation::PlayerCommand(command))? {
                    self.console_pending = Some(operation);
                    self.console.message = "WAITING FOR WORLD".into();
                } else {
                    self.console.message = "WORLD PAUSED OR COMMAND QUEUE FULL".into();
                }
            }
            console::Command::Redstone(action) => self.apply_debug_command(action)?,
            console::Command::Tick(command) => {
                self.tick_pending = Some(self.host.tick_command(command)?);
                self.console.message = "WAITING FOR TICK CLOCK".into();
            }
            console::Command::FreeCamera(request) => {
                self.set_free_camera(request.unwrap_or(self.free_camera.is_none()))?;
                self.console.open = false;
            }
        }
        Ok(())
    }

    fn apply_debug_command(&mut self, action: console::DebugAction) -> Result<(), Error> {
        #[cfg(any(debug_assertions, feature = "dev-tools"))]
        {
            use console::DebugAction;
            match action {
                DebugAction::On => self.set_replay(true)?,
                DebugAction::Off => self.set_replay(false)?,
                _ if !self.replay.active => {
                    self.console.message = "RUN REDSTONE ON FIRST".into();
                    return Ok(());
                }
                DebugAction::Previous => self.replay.previous(),
                DebugAction::Next => self.replay.next(),
                DebugAction::Step => {
                    if !self.step_replay()? {
                        self.console.message =
                            "STEP REQUIRES PAUSED LIVE GT WITH NO STEP IN PROGRESS".into();
                        return Ok(());
                    }
                }
            }
            self.console.open = false;
        }
        #[cfg(not(any(debug_assertions, feature = "dev-tools")))]
        {
            let _ = action;
            self.console.message = "REDSTONE DEBUG REQUIRES DEV-TOOLS".into();
        }
        Ok(())
    }

    fn submit_neutral_input(&mut self) -> io::Result<()> {
        self.cancel_mining()?;
        if self.sent != Some(self.aim) && self.submit(Mutation::ControlPlayer(self.aim))? {
            self.sent = Some(self.aim);
        }
        Ok(())
    }

    fn cancel_mining(&mut self) -> io::Result<()> {
        if self.mining_target.is_some()
            && (self.host.is_paused() || self.submit(Mutation::CancelMining)?)
        {
            self.mining_target = None;
            self.mining_start = None;
        }
        Ok(())
    }

    fn select_hotbar(&mut self, controls: &Controls) -> io::Result<()> {
        let Some(inventory) = &self.snapshot.inventory else {
            return Ok(());
        };
        let count = inventory.hotbar().len();
        if count == 0 {
            return Ok(());
        }
        if let Some(slot) = controls.hotbar.filter(|&slot| slot < count) {
            self.hotbar_target = Some(slot);
        }
        if controls.hotbar_scroll != 0 {
            let base = self.hotbar_target.unwrap_or(inventory.selected());
            self.hotbar_target = Some(
                (base as i64 - i64::from(controls.hotbar_scroll)).rem_euclid(count as i64) as usize,
            );
        }
        let Some(slot) = self.hotbar_target else {
            return Ok(());
        };
        if self.hotbar_pending.is_some() {
            return Ok(());
        }
        if slot == inventory.selected() {
            self.hotbar_target = None;
            return Ok(());
        }
        let revision = inventory.revision();
        let operation = self.next_operation;
        if self.submit(Mutation::EditInventory {
            action: InventoryAction::Select { slot },
            expected_revision: revision,
        })? {
            self.hotbar_pending = Some(operation);
        }
        Ok(())
    }

    fn handle_inventory(&mut self, controls: &Controls, toggle: bool) -> io::Result<()> {
        self.submit_neutral_input()?;
        self.cursor = controls.cursor;
        self.inventory_close |= toggle || controls.menu.contains(&MenuKey::Toggle);
        if self.inventory_pending.is_some() {
            return Ok(());
        }
        let inventory = self.snapshot.inventory.as_ref().expect("local inventory");
        let action = if self.inventory_close {
            if inventory.carried().is_none() {
                self.inventory_open = false;
                self.inventory_close = false;
                return Ok(());
            }
            Some(InventoryAction::ReturnCarried)
        } else {
            controls.ui_click.and_then(|(cursor, button)| {
                let hit = self.inventory_layout(inventory.slots().len()).hit(cursor)?;
                match (hit, button) {
                    (InventoryHit::Player(slot), Interaction::Break) => {
                        Some(InventoryAction::Click {
                            slot,
                            button: InventoryButton::Left,
                        })
                    }
                    (InventoryHit::Player(slot), Interaction::Place) => {
                        Some(InventoryAction::Click {
                            slot,
                            button: InventoryButton::Right,
                        })
                    }
                    (InventoryHit::Catalog(index), Interaction::Break | Interaction::Pick) => {
                        Some(InventoryAction::Catalog {
                            block: self.catalog[index].0,
                            stack: matches!(button, Interaction::Pick),
                        })
                    }
                    (InventoryHit::Delete, _) => Some(InventoryAction::DiscardCarried),
                    _ => None,
                }
            })
        };
        if let Some(action) = action {
            let operation = self.next_operation;
            if self.submit(Mutation::EditInventory {
                action,
                expected_revision: inventory.revision(),
            })? {
                self.inventory_pending = Some((operation, self.inventory_close));
            }
        }
        Ok(())
    }

    fn rendered_snapshot(&self) -> &Snapshot {
        #[cfg(any(debug_assertions, feature = "dev-tools"))]
        return self.replay.snapshot(&self.snapshot);
        #[cfg(not(any(debug_assertions, feature = "dev-tools")))]
        &self.snapshot
    }

    fn inventory_layout(&self, slots: usize) -> InventoryLayout {
        InventoryLayout::new(
            self.frame_size,
            slots,
            (self.snapshot.game_mode == GameMode::Creative).then_some(self.catalog.len()),
        )
    }

    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    fn set_replay(&mut self, active: bool) -> Result<(), Error> {
        if self.replay.active == active {
            return Ok(());
        }
        self.host.set_paused(active)?;
        if active {
            self.camera_before_replay = self.free_camera;
            self.set_free_camera(true)?;
        } else {
            self.free_camera = self.camera_before_replay.take();
        }
        self.replay.open(active);
        if active && let Some(sounds) = &mut self.sounds {
            sounds.stop();
        }
        self.reset_player_controls()
    }

    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    fn replay_view(&self, camera: camera::Camera) -> rustcraft_authority::DebugView {
        rustcraft_authority::DebugView {
            eye: camera.eye,
            yaw: camera.yaw,
            pitch: camera.pitch,
        }
    }

    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    fn queue_replay_interaction(&mut self, controls: &Controls) {
        if !controls.captured
            || !self.host.is_paused()
            || !self.replay.at_live_tick()
            || self.replay.pending_until.is_some()
            || self.replay.planned.len() >= tick_replay::MAX_PLANNED
        {
            return;
        }
        let Some(player) = self.snapshot.player else {
            return;
        };
        let camera = self
            .free_camera
            .unwrap_or_else(|| camera::Camera::at_player(player));
        let action = match controls.interaction {
            Some(Interaction::Place) => BlockAction::Place,
            Some(Interaction::Break) if self.snapshot.game_mode == GameMode::Creative => {
                BlockAction::Break
            }
            Some(Interaction::Pick) => BlockAction::Pick,
            _ => return,
        };
        if let Some(planned) = self
            .snapshot
            .plan_debug_interaction(self.replay_view(camera), action)
        {
            self.replay.planned.push(planned);
        }
    }

    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    fn step_replay(&mut self) -> io::Result<bool> {
        if !self.replay.at_live_tick() || self.replay.pending_until.is_some() {
            return Ok(false);
        }
        let Some(end) = self
            .snapshot
            .tick
            .checked_add(u64::from(JAVA_REDSTONE_TICK_GAME_TICKS))
        else {
            return Ok(false);
        };
        let len = self.replay.planned.len() as u64;
        let next = self
            .next_operation
            .checked_add(len)
            .expect("client sequence exhausted");
        let requests = (self.next_operation..next)
            .zip(self.replay.planned.iter().copied())
            .collect();
        if !self.host.step_redstone_paused(requests)? {
            return Ok(false);
        }
        self.replay.planned.clear();
        self.replay.pending_until = Some(end);
        self.next_operation = next;
        Ok(true)
    }

    fn debug_player_rows(&self, player: PlayerSnapshot, camera: camera::Camera) -> [String; 9] {
        let snapshot = self.rendered_snapshot();
        let [x, y, z] = player.position;
        let block = player.block_position();
        let chunk = block.chunk();
        let section = block.y.div_euclid(SECTION_EDGE as i64);
        let [cx, cy, cz] = camera.eye;
        let movement = if player.flying {
            "Flying"
        } else if player.grounded {
            if self.player_speed > 0.01 {
                "Walking"
            } else {
                "Standing"
            }
        } else {
            "Airborne"
        };
        let target = player.target.map_or_else(
            || "Target none".into(),
            |target| {
                let BlockPos { x, y, z } = target.position;
                format!("Target {x} {y} {z}  Rev {}", target.revision)
            },
        );
        [
            "Player and world".into(),
            format!(
                "{}  {movement}  {:.2} blocks/s",
                snapshot.game_mode.name(),
                self.player_speed
            ),
            format!("XYZ {x:.2} {y:.2} {z:.2}"),
            format!(
                "Chunk {} {}  Section {section}  GT {}",
                chunk.x, chunk.z, snapshot.tick
            ),
            target,
            if self.free_camera.is_some() {
                "Free camera".into()
            } else {
                "Player camera".into()
            },
            format!("XYZ {cx:.2} {cy:.2} {cz:.2}  {:.2} blocks/s", camera.speed),
            format!("Facing {}", camera.direction()),
            format!(
                "Yaw {:.1}  Pitch {:.1}",
                camera.yaw.to_degrees(),
                camera.pitch.to_degrees()
            ),
        ]
    }

    fn debug_render_rows(
        &self,
        meshes: usize,
        pending: usize,
        timing: FrameTiming,
        graphics: Option<&rustcraft_vulkan::GraphicsInfo>,
        surface: Option<&str>,
    ) -> [String; 9] {
        let fps = timing
            .average_interval
            .filter(|interval| !interval.is_zero())
            .map_or_else(
                || "--".into(),
                |interval| format!("{:.1}", 1.0 / interval.as_secs_f64()),
            );
        let interval = timing.last_interval.map_or_else(
            || "--".into(),
            |interval| format!("{:.2}", interval.as_secs_f64() * 1000.0),
        );
        let chunks = self.rendered_snapshot().chunks.len();
        let view = self.applied_view_distance.radius();
        let (gpu, driver, api) = graphics.map_or_else(
            || {
                (
                    "GPU --".to_owned(),
                    "Driver --".to_owned(),
                    "Vulkan --".to_owned(),
                )
            },
            |graphics| {
                let [major, minor, patch] = graphics.vulkan_version;
                (
                    format!("GPU {}", graphics.device),
                    format!("Driver {} {}", graphics.driver, graphics.driver_info),
                    format!(
                        "Vulkan {major}.{minor}.{patch}  Queue {}",
                        graphics.queue_family
                    ),
                )
            },
        );
        [
            "Hardware".into(),
            format!("CPU {}", self.cpu),
            gpu,
            driver,
            api,
            "Rendering".into(),
            format!(
                "Dynamic rendering  Terrain depth + HUD  FIFO  {}",
                surface.unwrap_or("--")
            ),
            format!("FPS {fps}  FRAME {interval} MS"),
            format!("View {view}  Chunks {chunks}  Mesh {meshes}  Pending {pending}"),
        ]
    }

    fn menu_key(&mut self, key: MenuKey) -> Result<(), Error> {
        let previous = self.settings;
        match (self.menu, key) {
            (None, MenuKey::Toggle) => self.set_menu(true)?,
            (Some(_), MenuKey::Toggle) | (Some(0), MenuKey::Accept) => self.set_menu(false)?,
            (Some(row), MenuKey::Up) => {
                self.menu = Some((row + PAUSE_MENU_ROWS - 1) % PAUSE_MENU_ROWS)
            }
            (Some(row), MenuKey::Down) => self.menu = Some((row + 1) % PAUSE_MENU_ROWS),
            (Some(1), MenuKey::Left | MenuKey::Right) => {
                self.settings.adjust_sensitivity(key == MenuKey::Right);
            }
            (Some(2), MenuKey::Left | MenuKey::Right | MenuKey::Accept) => {
                self.settings.invert_y = match key {
                    MenuKey::Accept => !self.settings.invert_y,
                    _ => key == MenuKey::Right,
                };
            }
            (Some(3), MenuKey::Left | MenuKey::Right) => {
                self.settings.audio.adjust_volume(key == MenuKey::Right);
            }
            (Some(4), MenuKey::Left | MenuKey::Right | MenuKey::Accept) => {
                self.settings.audio.muted = match key {
                    MenuKey::Accept => !self.settings.audio.muted,
                    _ => key == MenuKey::Right,
                };
            }
            (Some(5), MenuKey::Left | MenuKey::Right) => {
                self.settings.adjust_view_distance(key == MenuKey::Right);
            }
            _ => {}
        }
        if self.settings.audio != previous.audio
            && let Some(sounds) = &mut self.sounds
        {
            sounds.settings_changed(self.settings.audio);
        }
        if self.settings != previous
            && let Some(path) = &self.settings_path
            && let Err(error) = self.settings.save(path)
        {
            eprintln!("event=settings_save_failed path={path:?} error={error:?}");
        }
        Ok(())
    }

    fn set_menu(&mut self, pause: bool) -> Result<(), Error> {
        if !pause && self.applied_view_distance != self.settings.view_distance {
            self.host.set_view_distance(self.settings.view_distance)?;
            self.applied_view_distance = self.settings.view_distance;
            log_view_distance(self.applied_view_distance);
        }
        self.host.set_paused(pause)?;
        if pause && let Some(sounds) = &mut self.sounds {
            sounds.stop();
        }
        self.menu = pause.then_some(0);
        self.reset_player_controls()
    }

    fn reset_player_controls(&mut self) -> Result<(), Error> {
        self.target_shown = None;
        self.flash = None;
        let player = self.snapshot.player.ok_or("missing local player")?;
        self.aim = PlayerInput {
            yaw: player.yaw,
            pitch: player.pitch,
            ..PlayerInput::default()
        };
        self.sent = None;
        Ok(())
    }

    fn flash(&mut self, accepted: bool) {
        let (color, ms) = if accepted {
            (Crosshair::Accepted, 180)
        } else {
            (Crosshair::Rejected, 240)
        };
        self.flash = Some((color, Instant::now() + Duration::from_millis(ms)));
    }

    fn crosshair(&self) -> Crosshair {
        if let Some((color, until)) = self.flash
            && Instant::now() < until
        {
            color
        } else if self
            .snapshot
            .player
            .is_some_and(|player| player.target.is_some())
        {
            Crosshair::Targeted
        } else {
            Crosshair::Idle
        }
    }

    fn submit(&mut self, mutation: Mutation) -> io::Result<bool> {
        match self.host.submit(self.next_operation, mutation) {
            Ok(()) => {
                self.next_operation = self
                    .next_operation
                    .checked_add(1)
                    .expect("client sequence exhausted");
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(error) => Err(error),
        }
    }
}

fn log_view_distance(distance: ViewDistance) {
    eprintln!("event=view_distance_applied radius={}", distance.radius());
}

fn cpu_name() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|cpu| {
            cpu.lines()
                .filter_map(|row| row.split_once(':'))
                .find(|(name, _)| matches!(name.trim(), "model name" | "Hardware"))
                .map(|(_, model)| model.trim().to_owned())
        })
        .unwrap_or_else(|| std::env::consts::ARCH.into())
}

fn diagnostic_panel(
    size: [u32; 2],
    rows: &[String],
    corner: HudCorner,
) -> Vec<rustcraft_scene::draw::Vertex> {
    let lines: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            vec![(
                row.as_str(),
                if index == 0 || index == 5 {
                    HUD_ACCENT
                } else {
                    HUD_TEXT
                },
            )]
        })
        .collect();
    colored_hud(size, &lines, corner, true)
}

pub(super) fn preview(path: &Path, pack: Option<&Path>) -> Result<(), Error> {
    let bytes = read_pack(pack)?;
    let pack = open_pack(bytes.as_deref())?;
    let (server, eye, pitch) = if pack.id() == "rustcraft_java:classic_flat" {
        let preset = RulePreset::JavaFlatV1;
        let mut server = Server::playable_with_rules(42, preset.default_range(), preset)?;
        preview_blocks(&mut server)?;
        (server, [0.5, -56.0, 0.2], -0.58)
    } else {
        let mut server = Server::playable(42)?;
        preview_blocks(&mut server)?;
        (server, [0.5, 3.3, 0.2], -0.37)
    };
    let size = [960, 600];
    let mut scene = Scene::new(&server, &pack)?;
    let frame = scene.frame(&server.snapshot(), size, eye, 0.0, pitch, Crosshair::Idle);
    if frame.terrain.is_empty() {
        return Err("terrain preview mesh is empty".into());
    }
    let rgba = rustcraft_vulkan::render_terrain(&frame, size)?;
    let mut file = io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(file, "P6\n{} {}\n255", size[0], size[1])?;
    for pixel in rgba.as_chunks::<4>().0 {
        file.write_all(&pixel[..3])?;
    }
    file.flush()?;
    eprintln!(
        "event=terrain_preview_written path={:?} vertices={}",
        path,
        frame.terrain.len()
    );
    Ok(())
}

fn preview_blocks(server: &mut Server) -> Result<(), Error> {
    let layout: &[(BlockPos, &str)] = match server.rules() {
        RulePreset::NativeV1 => &[
            (BlockPos::new(-3, 0, 6), "rustcraft_content:basalt"),
            (BlockPos::new(-2, 0, 6), "rustcraft_content:basalt"),
            (BlockPos::new(-2, 1, 6), "rustcraft_content:basalt"),
            (BlockPos::new(-1, 0, 6), "rustcraft_content:basalt"),
            (BlockPos::new(1, 0, 6), "rustcraft_content:loam"),
            (BlockPos::new(2, 0, 6), "rustcraft_content:loam"),
            (BlockPos::new(2, 1, 6), "rustcraft_content:loam"),
            (BlockPos::new(3, 0, 6), "rustcraft_content:loam"),
        ],
        RulePreset::JavaFlatV1 | RulePreset::JavaFlatV2 | RulePreset::JavaFlatV3 => &[
            (BlockPos::new(-3, -60, 7), "rustcraft_java:lever_on"),
            (BlockPos::new(-2, -60, 7), "rustcraft_java:redstone"),
            (BlockPos::new(-1, -60, 7), "rustcraft_java:redstone"),
            (BlockPos::new(0, -60, 7), "rustcraft_java:redstone"),
            (BlockPos::new(0, -60, 8), "rustcraft_java:redstone"),
            (BlockPos::new(0, -60, 6), "rustcraft_java:redstone"),
            (BlockPos::new(1, -60, 6), "rustcraft_java:redstone"),
            (BlockPos::new(2, -60, 6), "rustcraft_java:repeater"),
            (BlockPos::new(1, -60, 7), "rustcraft_java:redstone"),
            (BlockPos::new(2, -60, 7), "rustcraft_java:grass_block"),
            (BlockPos::new(2, -59, 7), "rustcraft_java:redstone"),
            (BlockPos::new(0, -60, 4), "rustcraft_java:lever_on"),
            (BlockPos::new(1, -60, 4), "rustcraft_java:piston"),
            (BlockPos::new(2, -60, 8), "rustcraft_java:piston_north"),
        ],
    };
    let session = server.open_local_session(Capabilities::P1_HOST);
    for (operation, &(position, material)) in layout.iter().enumerate() {
        let snapshot = server.snapshot();
        let expected_revision = snapshot.chunks[&position.chunk()].revision();
        let state = server
            .registry()
            .block_id(material)
            .ok_or("preview material is not registered")?;
        let report = server.step(vec![Request {
            session,
            operation: operation as u64 + 1,
            mutation: Mutation::SetBlock {
                position,
                state,
                expected_revision,
            },
        }]);
        if report.acks[0].result != Ok(Effect::ChunkChanged(position.chunk())) {
            return Err(format!("preview block rejected: {:?}", report.acks[0].result).into());
        }
    }
    server.close_session(session);
    Ok(())
}

fn read_pack(path: Option<&Path>) -> io::Result<Option<Vec<u8>>> {
    path.map(|path| {
        if path.metadata()?.len() > 65 * 1024 * 1024 + 12 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "resource pack exceeds size limit",
            ));
        }
        std::fs::read(path)
    })
    .transpose()
}

fn open_pack(bytes: Option<&[u8]>) -> Result<Pack<'_>, Error> {
    let pack = Pack::open(bytes.unwrap_or(NATIVE_BUNDLE))?;
    if !pack.dependencies().is_empty() {
        return Err("client requires a resource pack with resolved dependencies".into());
    }
    Ok(pack)
}

pub(super) fn audio_smoke(path: Option<&Path>) -> Result<(), Error> {
    let bytes = read_pack(path)?;
    let pack = open_pack(bytes.as_deref())?;
    audio::smoke(SoundBank::new(&pack)?)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::settings::BASE_SENSITIVITY;

    fn advance(
        game: &mut Game<'_>,
        controls: impl Fn() -> Controls,
        ready: impl Fn(&Snapshot) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !ready(&game.snapshot) {
            assert!(
                Instant::now() < deadline,
                "timed out awaiting authoritative response"
            );
            game.update(controls()).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn rapid_wheel_selection_wraps_and_waits_for_authoritative_inventory_revisions() {
        let preset = RulePreset::JavaFlatV3;
        let server = Server::playable_with_rules(42, preset.default_range(), preset).unwrap();
        let host = LocalHost::start_world(server, None).unwrap();
        let mut game = Game::new(&host, None, ClientSettings::default()).unwrap();
        let wheel = |steps| Controls {
            hotbar_scroll: steps,
            ..Controls::default()
        };
        game.select_hotbar(&wheel(1)).unwrap();
        let pending = game.hotbar_pending.unwrap();
        assert_eq!(game.hotbar_target, Some(8));
        game.select_hotbar(&wheel(1)).unwrap();
        assert_eq!(game.hotbar_pending, Some(pending));
        assert_eq!(game.hotbar_target, Some(7));
        advance(&mut game, Controls::default, |view| {
            view.inventory.as_ref().unwrap().selected() == 7
        });
        game.update(wheel(-11)).unwrap();
        advance(&mut game, Controls::default, |view| {
            view.inventory.as_ref().unwrap().selected() == 0
        });
        game.update(Controls {
            hotbar: Some(4),
            hotbar_scroll: -2,
            ..Controls::default()
        })
        .unwrap();
        advance(&mut game, Controls::default, |view| {
            view.inventory.as_ref().unwrap().selected() == 6
        });
        game.update(Controls {
            menu: vec![MenuKey::Toggle],
            ..Controls::default()
        })
        .unwrap();
        game.update(wheel(1)).unwrap();
        assert_eq!(game.snapshot.inventory.as_ref().unwrap().selected(), 6);
        assert!(game.hotbar_target.is_none());
    }

    #[test]
    #[ignore = "requires the locally extracted and staged Java resource pack"]
    fn renders_hud_item_icons_with_the_terrain_view_projection() {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../local-resources/minecraft-java/1.20.1/classic-flat.rpak"
        ))
        .unwrap();
        let pack = Pack::open(&bytes).unwrap();
        let preset = RulePreset::JavaFlatV1;
        let server = Server::playable_with_rules(42, preset.default_range(), preset).unwrap();
        let scene = Scene::new(&server, &pack).unwrap();
        let redstone = server
            .registry()
            .block_id("rustcraft_java:redstone")
            .unwrap();
        let icon = scene.item_icon(redstone);
        let mut view_projection = perspective(0.4, -0.3, 640.0 / 96.0);
        // If an item vertex were treated as terrain, this translation would
        // move it outside the 640x96 target. HUD vertices must ignore it.
        view_projection[3][0] = 10.0;
        let frame = Frame {
            terrain: Vec::new(),
            hud: hotbar([640, 96], &[Some((icon, 1))], 0, Crosshair::Idle),
            view_projection,
            atlas: scene.atlas.clone(),
            background: FLAT_SKY,
        };
        let pixels = rustcraft_vulkan::render_terrain(&frame, [640, 96]).unwrap();
        let red = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| {
                let [red, green, blue] = [pixel[0], pixel[1], pixel[2]].map(u16::from);
                red > 150 && red > green * 2 && red > blue * 2
            })
            .count();
        assert!(red > 10, "expected visible red item pixels, found {red}");
    }

    #[test]
    #[ignore = "requires the locally extracted and staged Java resource pack"]
    fn every_java_catalog_item_has_a_textured_icon_in_the_local_pack() {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../local-resources/minecraft-java/1.20.1/classic-flat.rpak"
        ))
        .unwrap();
        let pack = Pack::open(&bytes).unwrap();
        for preset in [
            RulePreset::JavaFlatV1,
            RulePreset::JavaFlatV2,
            RulePreset::JavaFlatV3,
        ] {
            let server = Server::playable_with_rules(42, preset.default_range(), preset).unwrap();
            let scene = Scene::new(&server, &pack).unwrap();
            let atlas = scene.atlas.as_ref().unwrap();
            let mut catalog = Vec::new();
            for (id, name) in server.creative_items() {
                let icon = scene.item_icon(id);
                let ItemIcon::Texture([x, y, w, h]) = icon else {
                    panic!("missing icon for {name}")
                };
                assert_eq!([w, h], [64.0, 64.0]);
                let alpha: Vec<_> = (y as u32..(y + h) as u32)
                    .flat_map(|row| {
                        (x as u32..(x + w) as u32).map(move |col| {
                            atlas.pixels[((row * atlas.width + col) * 4 + 3) as usize]
                        })
                    })
                    .collect();
                assert!(alpha.contains(&0) && alpha.contains(&255), "{name}");
                catalog.push((name, icon));
            }
            let slots = vec![Some((catalog[0].1, 64)); 9];
            let bar = hotbar([1024, 640], &slots, 0, Crosshair::Idle);
            assert_eq!(bar.iter().filter(|v| v.atlas[2] > 0.0).count(), 9 * 6);
            let panel = InventoryLayout::new([1024, 640], 9, Some(catalog.len())).draw(
                &slots,
                0,
                Some([500.0, 300.0]),
                Some((catalog[0].1, 1)),
                &catalog,
            );
            assert_eq!(
                panel.iter().filter(|v| v.atlas[2] > 0.0).count(),
                (9 + catalog.len() + 1) * 6
            );
        }
    }

    #[test]
    fn forwards_movement_hotbar_selection_and_placement_through_the_local_host() {
        let server = Server::playable(42).unwrap();
        let loam = server
            .registry()
            .block_id("rustcraft_content:loam")
            .unwrap();
        let host = LocalHost::start_world(server, None).unwrap();
        let mut game = Game::new(&host, None, ClientSettings::default()).unwrap();
        advance(&mut game, Controls::default, |view| {
            view.player.unwrap().grounded
        });
        let start = game.snapshot.player.unwrap().position[2];
        advance(
            &mut game,
            || Controls {
                forward: 1,
                ..Controls::default()
            },
            |view| view.player.unwrap().position[2] > start,
        );

        game.update(Controls {
            hotbar: Some(1),
            mouse: [0.0, 0.65 / BASE_SENSITIVITY],
            ..Controls::default()
        })
        .unwrap();
        advance(&mut game, Controls::default, |view| {
            view.inventory.as_ref().unwrap().selected() == 1
                && view.player.unwrap().target.is_some()
        });
        let target = game.snapshot.player.unwrap().target.unwrap();
        let placement = target.placement.unwrap().position;
        let (mixer, mut audio) = rodio::mixer::mixer(rodio::nz!(2), rodio::nz!(24000));
        game.sounds = Some(SoundEffects::new(
            mixer,
            SoundBank::new(&Pack::open(NATIVE_BUNDLE).unwrap()).unwrap(),
            game.settings.audio,
        ));
        game.target_shown = Some(target);
        game.update(Controls {
            interaction: Some(Interaction::Place),
            ..Controls::default()
        })
        .unwrap();
        advance(&mut game, Controls::default, |view| {
            view.chunks[&placement.chunk()].block(placement) == Some(loam)
        });
        assert_eq!(
            game.snapshot.inventory.as_ref().unwrap().slots()[1]
                .unwrap()
                .count,
            7
        );
        assert!(matches!(game.flash, Some((Crosshair::Accepted, _))));
        assert!(audio.by_ref().take(1200).any(|sample| sample.abs() > 0.02));
        audio.by_ref().take(12_000).for_each(drop);

        game.update(Controls {
            hotbar: Some(7),
            ..Controls::default()
        })
        .unwrap();
        advance(&mut game, Controls::default, |view| {
            view.inventory.as_ref().unwrap().selected() == 7
        });
        game.target_shown = game.snapshot.player.unwrap().target;
        let chunks = game.snapshot.chunks.clone();
        let operation = game.next_operation;
        game.update(Controls {
            interaction: Some(Interaction::Place),
            ..Controls::default()
        })
        .unwrap();
        assert!(game.pending_clicks.contains(&operation));
        let deadline = Instant::now() + Duration::from_secs(3);
        while game.pending_clicks.contains(&operation) {
            assert!(
                Instant::now() < deadline,
                "timed out awaiting place rejection"
            );
            game.update(Controls::default()).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(game.flash, Some((Crosshair::Rejected, _))));
        assert_eq!(game.snapshot.chunks, chunks);
        assert!(audio.take(1000).all(|sample| sample.abs() < 1e-7));
        host.shutdown().unwrap();
    }

    #[test]
    fn pause_menu_persists_mouse_settings_and_blocks_game_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("controls.json");
        let host = LocalHost::start_playable(42).unwrap();
        let mut game = Game::new(&host, Some(path.clone()), ClientSettings::default()).unwrap();
        game.update(Controls {
            forward: 1,
            hotbar: Some(1),
            mouse: [0.0, 100.0],
            menu: vec![
                MenuKey::Toggle,
                MenuKey::Down,
                MenuKey::Right,
                MenuKey::Down,
                MenuKey::Accept,
            ],
            ..Controls::default()
        })
        .unwrap();
        assert_eq!(game.menu, Some(2));
        let loaded = ClientSettings::load(&path).unwrap();
        assert_eq!(loaded.sensitivity_percent, 125);
        assert!(loaded.invert_y);
        advance(&mut game, Controls::default, |view| view.tick > 0);
        assert_eq!(game.snapshot.inventory.as_ref().unwrap().selected(), 0);
        assert_eq!(game.snapshot.player.unwrap().pitch, 0.0);
        assert_eq!(game.snapshot.player.unwrap().position[2], 0.5);

        game.update(Controls {
            menu: vec![MenuKey::Down, MenuKey::Left, MenuKey::Down, MenuKey::Accept],
            ..Controls::default()
        })
        .unwrap();
        let loaded = ClientSettings::load(&path).unwrap();
        assert_eq!(loaded.audio.volume_percent, 60);
        assert!(loaded.audio.muted);
        game.update(Controls {
            menu: vec![MenuKey::Down, MenuKey::Down, MenuKey::Accept],
            ..Controls::default()
        })
        .unwrap();
        assert!(game.menu.is_none());
        host.shutdown().unwrap();
    }

    #[test]
    fn toggles_flight_with_two_released_jump_presses_in_one_frame() {
        let host = LocalHost::start_playable(42).unwrap();
        let mut game = Game::new(&host, None, ClientSettings::default()).unwrap();
        advance(&mut game, Controls::default, |view| {
            view.player.unwrap().grounded
        });
        game.update(Controls {
            captured: true,
            jump_presses: 2,
            ..Controls::default()
        })
        .unwrap();
        advance(&mut game, Controls::default, |view| {
            view.player.unwrap().flying
        });
        assert!(game.snapshot.player.unwrap().position[1] > 0.0);
        game.update(Controls {
            captured: true,
            jump_presses: 2,
            ..Controls::default()
        })
        .unwrap();
        advance(&mut game, Controls::default, |view| {
            let player = view.player.unwrap();
            player.grounded && !player.flying
        });
        host.shutdown().unwrap();
    }

    #[test]
    fn applies_saved_view_distance_only_after_resuming() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("controls.json");
        let mut server = Server::playable(42).unwrap();
        let mut settings = ClientSettings::default();
        settings.view_distance = ViewDistance::from_radius(1).unwrap();
        server.set_local_view_distance(settings.view_distance);
        let host = LocalHost::start_world(server, None).unwrap();
        let mut game = Game::new(&host, Some(path.clone()), settings).unwrap();
        advance(&mut game, Controls::default, |view| view.chunks.len() == 9);
        game.update(Controls {
            menu: vec![
                MenuKey::Toggle,
                MenuKey::Up,
                MenuKey::Right,
                MenuKey::Right,
                MenuKey::Right,
            ],
            ..Controls::default()
        })
        .unwrap();
        assert_eq!(
            ClientSettings::load(&path).unwrap().view_distance.radius(),
            4
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while !host.is_paused() {
            assert!(Instant::now() < deadline);
            game.update(Controls::default()).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        game.update(Controls::default()).unwrap();
        let tick = game.snapshot.tick;
        std::thread::sleep(Duration::from_millis(120));
        game.update(Controls::default()).unwrap();
        assert_eq!(game.snapshot.tick, tick);
        assert_eq!(game.snapshot.chunks.len(), 9);

        game.update(Controls {
            menu: vec![MenuKey::Toggle],
            ..Controls::default()
        })
        .unwrap();
        advance(&mut game, Controls::default, |view| view.chunks.len() == 81);
        host.shutdown().unwrap();
    }

    #[test]
    fn keeps_drawable_meshes_until_their_rebuild_budget_is_available() {
        let mut server = Server::playable(42).unwrap();
        let mut scene = Scene::new(&server, &open_pack(None).unwrap()).unwrap();
        let before = server.snapshot();
        scene.update_meshes_budgeted(&before, ChunkPos::ORIGIN);
        assert_eq!(scene.meshes.len(), MESH_REBUILDS_PER_FRAME);
        assert!(scene.meshes.contains_key(&ChunkPos::ORIGIN));
        scene.update_meshes(&before);
        let position = BlockPos::new(0, 0, 1);
        let state = server
            .registry()
            .block_id("rustcraft_content:basalt")
            .unwrap();
        let session = server.open_local_session(Capabilities::P1_HOST);
        let report = server.step(vec![Request {
            session,
            operation: 1,
            mutation: Mutation::SetBlock {
                position,
                state,
                expected_revision: before.chunks[&ChunkPos::ORIGIN].revision(),
            },
        }]);
        assert_eq!(
            report.acks[0].result,
            Ok(Effect::ChunkChanged(ChunkPos::ORIGIN))
        );
        assert!(!scene.meshes[&ChunkPos::ORIGIN].is_current(&report.snapshot.chunks));
        assert_eq!(
            scene.update_meshes_budgeted(&report.snapshot, ChunkPos::ORIGIN),
            1
        );
        assert_eq!(scene.meshes.len(), report.snapshot.chunks.len());
        assert_eq!(
            scene
                .meshes
                .values()
                .filter(|mesh| !mesh.is_current(&report.snapshot.chunks))
                .count(),
            1
        );
        assert_eq!(
            scene.update_meshes_budgeted(&report.snapshot, ChunkPos::ORIGIN),
            0
        );
        assert!(
            scene
                .meshes
                .values()
                .all(|mesh| mesh.is_current(&report.snapshot.chunks))
        );
    }

    #[test]
    fn toggles_debug_views_independently_and_reports_negative_chunk_coordinates() {
        let host = LocalHost::start_playable(42).unwrap();
        let mut game = Game::new(&host, None, ClientSettings::default()).unwrap();
        game.update(Controls {
            toggle_chunk_border: true,
            ..Controls::default()
        })
        .unwrap();
        assert!(game.show_chunk_border);
        assert!(!game.show_debug);
        game.update(Controls {
            toggle_debug: true,
            ..Controls::default()
        })
        .unwrap();
        assert!(game.show_chunk_border && game.show_debug);

        let mut player = game.snapshot.player.unwrap();
        player.position = [-0.25, -0.5, -16.25];
        player.target = Some(BlockTarget {
            position: BlockPos::new(-1, -2, -17),
            revision: 12,
            placement: None,
        });
        let render = game.debug_render_rows(
            4,
            5,
            FrameTiming {
                last_interval: Some(Duration::from_millis(20)),
                average_interval: Some(Duration::from_millis(20)),
            },
            None,
            None,
        );
        let world = game.debug_player_rows(player, camera::Camera::at_player(player));
        assert_eq!(render[7], "FPS 50.0  FRAME 20.00 MS");
        assert!(world[3].starts_with("Chunk -1 -2  Section -1"));
        assert!(render[8].ends_with("Mesh 4  Pending 5"));
        assert_eq!(world[4], "Target -1 -2 -17  Rev 12");
        host.shutdown().unwrap();
    }
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    #[test]
    fn replay_moves_the_camera_only_and_restores_a_previous_free_camera() {
        let host = LocalHost::start_playable(42).unwrap();
        let mut game = Game::new(&host, None, ClientSettings::default()).unwrap();
        game.set_free_camera(true).unwrap();
        let original = game.free_camera.unwrap();
        game.set_replay(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !host.is_paused() {
            assert!(Instant::now() < deadline);
            game.update(Controls::default()).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        game.update(Controls::default()).unwrap();
        let player = game.snapshot.player.unwrap();
        game.update(Controls {
            captured: true,
            forward: 1,
            mouse: [3.0, 2.0],
            ..Controls::default()
        })
        .unwrap();
        let camera = game.free_camera.unwrap();
        assert_ne!(original.eye, camera.eye);
        assert_ne!(original.yaw, camera.yaw);
        assert_eq!(game.snapshot.player, Some(player));
        game.set_replay(false).unwrap();
        let restored = game.free_camera.unwrap();
        assert_eq!(original.eye, restored.eye);
        assert_eq!(original.yaw, restored.yaw);
        host.shutdown().unwrap();
    }
}
