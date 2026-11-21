#![forbid(unsafe_code)]

mod cbor;
#[cfg(all(test, target_os = "linux"))]
mod crash_tests;
mod java_redstone_record;
mod random_record;
mod records;
mod region;
mod task_record;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use rustcraft_authority::checkpoint::{Checkpoint, SavedChunk, SavedPlayer, SavedScheduledTasks};
use rustcraft_authority::{RulePreset, RuleVersion, Server};
use rustcraft_world::VerticalRange;
use serde::{Deserialize, Serialize};
use tempfile::{Builder, NamedTempFile};

use crate::java_redstone_record::JavaRedstoneRecord;
use crate::random_record::{RANDOM_STREAMS_SCHEMA, RandomStreamsRecord};
use crate::task_record::ScheduledTasksRecord;

const LEGACY_MANIFEST_SCHEMA: u64 = 1;
const GENERATOR_MANIFEST_SCHEMA: u64 = 2;
const SCHEDULE_MANIFEST_SCHEMA: u64 = 3;
const MOVEMENT_MANIFEST_SCHEMA: u64 = 4;
const BLOCK_INVENTORY_MANIFEST_SCHEMA: u64 = 5;
const SEED_DERIVATION_MANIFEST_SCHEMA: u64 = 6;
const RANDOM_STREAMS_MANIFEST_SCHEMA: u64 = 7;
const SCHEDULED_TASKS_MANIFEST_SCHEMA: u64 = 8;
const MANIFEST_SCHEMA: u64 = SCHEDULED_TASKS_MANIFEST_SCHEMA;
// The manifest can contain a full operation queue with canonical block names.
const MANIFEST_LIMIT: u64 = 4 * 1024 * 1024;
const PLAYER_LIMIT: u64 = 16 * 1024;

#[derive(Serialize, Deserialize)]
struct Manifest {
    schema: u64,
    generation: u64,
    seed: u64,
    tick: u64,
    height: [i64; 2],
    rules: Option<ManifestRules>,
    mods: Vec<(String, String)>,
    regions: Vec<[i64; 2]>,
    player_crc32c: u32,
    random_streams: Option<RandomStreamsRecord>,
    scheduled_tasks: Option<ScheduledTasksRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    java_redstone: Option<JavaRedstoneRecord>,
}

#[derive(Serialize, Deserialize)]
struct ManifestRules {
    preset: ManifestRuleVersion,
    world_generator: ManifestRuleVersion,
    tick_schedule: Option<ManifestRuleVersion>,
    player_movement: Option<ManifestRuleVersion>,
    block_inventory: Option<ManifestRuleVersion>,
    seed_derivation: Option<ManifestRuleVersion>,
    random_stream: Option<ManifestRuleVersion>,
}

impl From<RulePreset> for ManifestRules {
    fn from(preset: RulePreset) -> Self {
        Self {
            preset: preset.identity().into(),
            world_generator: preset.world_generator().into(),
            tick_schedule: Some(preset.tick_schedule().into()),
            player_movement: Some(preset.player_movement().into()),
            block_inventory: Some(preset.block_inventory().into()),
            seed_derivation: Some(preset.seed_derivation().into()),
            random_stream: Some(preset.random_stream().into()),
        }
    }
}

impl ManifestRules {
    fn resolve(&self, schema: u64) -> io::Result<RulePreset> {
        let preset = if schema < MANIFEST_SCHEMA {
            let legacy = RulePreset::NativeV1;
            self.preset.require(legacy.identity(), "rule preset")?;
            legacy
        } else {
            RulePreset::resolve(&self.preset.id, self.preset.version).map_err(invalid)?
        };
        self.world_generator
            .require(preset.world_generator(), "world generator")?;
        ManifestRuleVersion::require_since(
            self.tick_schedule.as_ref(),
            schema,
            SCHEDULE_MANIFEST_SCHEMA,
            preset.tick_schedule(),
            "tick schedule",
        )?;
        ManifestRuleVersion::require_since(
            self.player_movement.as_ref(),
            schema,
            MOVEMENT_MANIFEST_SCHEMA,
            preset.player_movement(),
            "player movement",
        )?;
        ManifestRuleVersion::require_since(
            self.block_inventory.as_ref(),
            schema,
            BLOCK_INVENTORY_MANIFEST_SCHEMA,
            preset.block_inventory(),
            "block and inventory rules",
        )?;
        ManifestRuleVersion::require_since(
            self.seed_derivation.as_ref(),
            schema,
            SEED_DERIVATION_MANIFEST_SCHEMA,
            preset.seed_derivation(),
            "seed derivation",
        )?;
        ManifestRuleVersion::require_since(
            self.random_stream.as_ref(),
            schema,
            RANDOM_STREAMS_MANIFEST_SCHEMA,
            preset.random_stream(),
            "random stream",
        )?;
        Ok(preset)
    }
}

#[derive(Serialize, Deserialize)]
struct ManifestRuleVersion {
    id: String,
    version: u32,
}

impl ManifestRuleVersion {
    fn require_since(
        declaration: Option<&Self>,
        schema: u64,
        introduced: u64,
        expected: RuleVersion,
        kind: &str,
    ) -> io::Result<()> {
        match (declaration, schema >= introduced) {
            (Some(rule), true) => rule.require(expected, kind),
            (None, true) => Err(invalid(format!("missing {kind} in manifest rules"))),
            (Some(_), false) => Err(invalid(format!(
                "manifest schema {schema} must not declare {kind}"
            ))),
            (None, false) => Ok(()),
        }
    }

    fn require(&self, expected: RuleVersion, kind: &str) -> io::Result<()> {
        if self.id != expected.id || self.version != expected.version {
            return Err(invalid(format!(
                "unsupported {kind} {:?} version {}, expected {:?} version {}",
                self.id, self.version, expected.id, expected.version
            )));
        }
        Ok(())
    }
}

impl From<RuleVersion> for ManifestRuleVersion {
    fn from(rule: RuleVersion) -> Self {
        Self {
            id: rule.id.into(),
            version: rule.version,
        }
    }
}

/// Open the current published checkpoint. A missing pointer denotes an unsaved world.
pub fn load(root: &Path) -> io::Result<Option<(u64, Server)>> {
    let Some(generation) = current(root)? else {
        return Ok(None);
    };
    let directory = checkpoint_directory(root, generation);
    Ok(Some((generation, load_checkpoint(&directory, generation)?)))
}

/// Open a published checkpoint and fail if the world has never been saved.
pub fn load_existing(root: &Path) -> io::Result<(u64, Server)> {
    load(root)?.ok_or_else(|| unpublished_world(root))
}

/// Read and validate a numbered checkpoint independently of CURRENT.
pub fn load_generation(root: &Path, generation: u64) -> io::Result<Server> {
    load_checkpoint(
        &numbered_checkpoint_directory(root, generation)?,
        generation,
    )
}

fn unpublished_world(root: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "no published checkpoint in {} (missing {})",
            root.display(),
            root.join("CURRENT").display()
        ),
    )
}

fn load_checkpoint(directory: &Path, generation: u64) -> io::Result<Server> {
    let (manifest, rules) = manifest(directory, generation)?;
    let player = read_player(directory, manifest.player_crc32c)?;
    let mut regions = BTreeSet::new();
    let mut chunks = Vec::new();
    for coordinates in manifest.regions {
        if !regions.insert(coordinates) {
            return Err(invalid("duplicate region in manifest"));
        }
        let name = region::file_name(coordinates);
        let saved = region::read(&directory.join(&name), coordinates).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("checkpoint {generation} region {name}: {error}"),
            )
        })?;
        chunks.extend(saved);
    }
    let [min, max_exclusive] = manifest.height;
    let checkpoint = Checkpoint {
        seed: manifest.seed,
        tick: manifest.tick,
        height: VerticalRange { min, max_exclusive },
        rules,
        mods: manifest.mods,
        chunks,
        player,
        random_streams: manifest.random_streams.map(Vec::from).unwrap_or_default(),
        scheduled_tasks: manifest
            .scheduled_tasks
            .map(SavedScheduledTasks::from)
            .unwrap_or_default(),
        java_redstone: manifest.java_redstone.map(Into::into),
    };
    Server::restore(checkpoint).map_err(invalid)
}

fn read_player(directory: &Path, checksum: u32) -> io::Result<SavedPlayer> {
    let bytes = read_limited(&directory.join("player.cbor"), PLAYER_LIMIT)?;
    if region::crc32c(&bytes) != checksum {
        return Err(invalid("CRC32C mismatch for player record"));
    }
    records::decode_player(&bytes)
}

#[derive(Debug)]
pub struct RecoveryCandidate {
    pub generation: u64,
    /// Tick from the checked manifest, or its read or validation error.
    pub tick: io::Result<u64>,
}

#[derive(Debug)]
pub struct RecoveryCandidates {
    pub current: io::Result<Option<u64>>,
    pub entries: Vec<RecoveryCandidate>,
}

/// List numbered checkpoint directories and read their manifests.
/// Verify or restore a candidate to validate its records and world state.
pub fn list_recovery_candidates(root: &Path) -> io::Result<RecoveryCandidates> {
    let current = current(root);
    let directory = root.join("checkpoints");
    let directories = directory.read_dir().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("listing {}: {error}", directory.display()),
        )
    })?;
    let mut entries = Vec::new();
    for entry in directories {
        let entry = entry?;
        let name = entry.file_name();
        let Some(generation) = name.to_str().and_then(|name| name.parse::<u64>().ok()) else {
            continue;
        };
        if generation == 0
            || name != generation.to_string().as_str()
            || !entry.file_type()?.is_dir()
        {
            continue;
        }
        let tick = manifest(&entry.path(), generation).map(|(manifest, _)| manifest.tick);
        entries.push(RecoveryCandidate { generation, tick });
    }
    entries.sort_unstable_by_key(|entry| entry.generation);
    Ok(RecoveryCandidates { current, entries })
}

/// Copy the published checkpoint into a new world directory. The same format
/// can be used to back up a world or restore a previously created backup.
pub fn copy_checkpoint(source: &Path, destination: &Path) -> io::Result<u64> {
    let generation = current(source)?.ok_or_else(|| unpublished_world(source))?;
    transfer_checkpoint(source, destination, generation, CheckpointTransfer::Copy)?;
    Ok(generation)
}

/// Validate and copy an explicitly selected checkpoint into a new world directory.
pub fn restore_generation(source: &Path, generation: u64, destination: &Path) -> io::Result<()> {
    numbered_checkpoint_directory(source, generation)?;
    transfer_checkpoint(source, destination, generation, CheckpointTransfer::Copy)
}

fn numbered_checkpoint_directory(root: &Path, generation: u64) -> io::Result<PathBuf> {
    if generation == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "generation must be positive",
        ));
    }
    let directory = checkpoint_directory(root, generation);
    let metadata = directory.symlink_metadata().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "checkpoint generation {generation} at {}: {error}",
                directory.display()
            ),
        )
    })?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("checkpoint generation {generation} must be a directory"),
        ));
    }
    Ok(directory)
}

/// Write the published checkpoint in the current format to a new world directory.
/// The source, generation, tick and resolved rule preset are preserved.
pub fn migrate_checkpoint(source: &Path, destination: &Path) -> io::Result<u64> {
    let generation = current(source)?.ok_or_else(|| unpublished_world(source))?;
    transfer_checkpoint(source, destination, generation, CheckpointTransfer::Migrate)?;
    Ok(generation)
}

enum CheckpointTransfer {
    Copy,
    Migrate,
}

fn transfer_checkpoint(
    source: &Path,
    destination: &Path,
    generation: u64,
    transfer: CheckpointTransfer,
) -> io::Result<()> {
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if parent.canonicalize()?.starts_with(source.canonicalize()?) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination must be outside the source world",
        ));
    }
    match destination.symlink_metadata() {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "destination already exists",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let directory = checkpoint_directory(source, generation);
    let mut header = read_manifest(&directory)?;
    let (source_manifest, rules) = decode_manifest(&header, generation)?;
    let stage = Builder::new()
        .prefix(".checkpoint-transfer-")
        .tempdir_in(parent)?;
    match transfer {
        CheckpointTransfer::Copy => {
            copy_synced(&directory, stage.path(), "manifest.json")?;
            copy_synced(&directory, stage.path(), "player.cbor")?;
        }
        CheckpointTransfer::Migrate => {
            let player = read_player(&directory, source_manifest.player_crc32c)?;
            let bytes = records::encode_player(&player);
            header["schema"] = MANIFEST_SCHEMA.into();
            header["rules"] = serde_json::to_value(ManifestRules::from(rules)).map_err(invalid)?;
            header["player_crc32c"] = region::crc32c(&bytes).into();
            if source_manifest.random_streams.is_none() {
                header["random_streams"] =
                    serde_json::to_value(RandomStreamsRecord::from(Vec::new())).map_err(invalid)?;
            }
            let tasks = source_manifest
                .scheduled_tasks
                .map(SavedScheduledTasks::from)
                .unwrap_or_default();
            header["scheduled_tasks"] =
                serde_json::to_value(ScheduledTasksRecord::from(tasks)).map_err(invalid)?;
            write_synced(&stage.path().join("player.cbor"), &bytes)?;
            write_manifest(stage.path(), &header)?;
        }
    }
    for coordinates in source_manifest.regions {
        copy_synced(&directory, stage.path(), &region::file_name(coordinates))?;
    }
    load_checkpoint(stage.path(), generation)?;
    File::open(stage.path())?.sync_all()?;

    fs::create_dir(destination)?;
    let _lock = lock_world(destination)?;
    let checkpoints = destination.join("checkpoints");
    fs::create_dir(&checkpoints)?;
    fs::rename(stage.path(), checkpoints.join(generation.to_string()))?;
    File::open(&checkpoints)?.sync_all()?;
    publish_current(destination, generation)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn checkpoint_directory(root: &Path, generation: u64) -> PathBuf {
    root.join("checkpoints").join(generation.to_string())
}

fn copy_synced(source: &Path, destination: &Path, name: &str) -> io::Result<()> {
    let copied = destination.join(name);
    fs::copy(source.join(name), &copied).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("copying checkpoint file {name}: {error}"),
        )
    })?;
    File::open(copied)?.sync_all()
}

/// Publish an immutable checkpoint from a tick boundary. The generation returned by
/// load or the prior save acts as a lease against concurrent or stale writers.
pub fn save(root: &Path, expected: Option<u64>, server: &Server) -> io::Result<u64> {
    let prepared = prepare_checkpoint(root, expected, server)?;
    publish_current(root, prepared.generation)?;
    Ok(prepared.generation)
}

struct PreparedCheckpoint {
    generation: u64,
    _world_lock: File,
}

fn prepare_checkpoint(
    root: &Path,
    expected: Option<u64>,
    server: &Server,
) -> io::Result<PreparedCheckpoint> {
    let checkpoint = server.checkpoint().map_err(invalid)?;
    fs::create_dir_all(root)?;
    let world_lock = lock_world(root)?;
    if current(root)? != expected {
        return Err(io::Error::other(
            "checkpoint generation changed since the world was opened",
        ));
    }

    let checkpoints = root.join("checkpoints");
    if let Some(previous) = expected {
        let (old, rules) = manifest(&checkpoints.join(previous.to_string()), previous)?;
        if checkpoint.tick < old.tick
            || checkpoint.seed != old.seed
            || [checkpoint.height.min, checkpoint.height.max_exclusive] != old.height
            || checkpoint.rules != rules
            || checkpoint.mods != old.mods
        {
            return Err(io::Error::other(
                "checkpoint does not extend the current world",
            ));
        }
    }
    fs::create_dir_all(&checkpoints)?;
    let generation = next_generation(&checkpoints)?;
    let stage = Builder::new()
        .prefix(".pending-")
        .tempdir_in(&checkpoints)?;
    let chunks = group_regions(&checkpoint.chunks);
    for (&coordinates, entries) in &chunks {
        region::write(&stage.path().join(region::file_name(coordinates)), entries)?;
    }
    let player = records::encode_player(&checkpoint.player);
    write_synced(&stage.path().join("player.cbor"), &player)?;
    let manifest = Manifest {
        schema: MANIFEST_SCHEMA,
        generation,
        seed: checkpoint.seed,
        tick: checkpoint.tick,
        height: [checkpoint.height.min, checkpoint.height.max_exclusive],
        rules: Some(checkpoint.rules.into()),
        mods: checkpoint.mods,
        regions: chunks.into_keys().collect(),
        player_crc32c: region::crc32c(&player),
        random_streams: Some(checkpoint.random_streams.into()),
        scheduled_tasks: Some(checkpoint.scheduled_tasks.into()),
        java_redstone: checkpoint.java_redstone.map(Into::into),
    };
    write_manifest(stage.path(), &manifest)?;

    File::open(stage.path())?.sync_all()?;
    fs::rename(stage.path(), checkpoints.join(generation.to_string()))?;
    File::open(&checkpoints)?.sync_all()?;
    Ok(PreparedCheckpoint {
        generation,
        _world_lock: world_lock,
    })
}

fn lock_world(root: &Path) -> io::Result<File> {
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".lock"))?;
    lock.lock()?;
    Ok(lock)
}

fn publish_current(root: &Path, generation: u64) -> io::Result<()> {
    let mut pointer = NamedTempFile::new_in(root)?;
    writeln!(pointer, "{generation}")?;
    pointer.as_file().sync_all()?;
    pointer.persist(root.join("CURRENT"))?;
    File::open(root)?.sync_all()
}

fn manifest(directory: &Path, generation: u64) -> io::Result<(Manifest, RulePreset)> {
    decode_manifest(&read_manifest(directory)?, generation)
}

fn read_manifest(directory: &Path) -> io::Result<serde_json::Value> {
    let bytes = read_limited(&directory.join("manifest.json"), MANIFEST_LIMIT)?;
    serde_json::from_slice(&bytes).map_err(invalid)
}

fn write_manifest(directory: &Path, manifest: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(manifest).map_err(invalid)?;
    if bytes.len() as u64 > MANIFEST_LIMIT {
        return Err(invalid("serialized manifest exceeds its size limit"));
    }
    write_synced(&directory.join("manifest.json"), &bytes)
}

fn decode_manifest(
    header: &serde_json::Value,
    generation: u64,
) -> io::Result<(Manifest, RulePreset)> {
    let schema = header
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| invalid("missing manifest schema version"))?;
    match schema {
        LEGACY_MANIFEST_SCHEMA if header.get("rules").is_some() => {
            return Err(invalid("legacy manifest must not declare rules"));
        }
        LEGACY_MANIFEST_SCHEMA
        | GENERATOR_MANIFEST_SCHEMA
        | SCHEDULE_MANIFEST_SCHEMA
        | MOVEMENT_MANIFEST_SCHEMA
        | BLOCK_INVENTORY_MANIFEST_SCHEMA
        | SEED_DERIVATION_MANIFEST_SCHEMA
        | RANDOM_STREAMS_MANIFEST_SCHEMA => {}
        _ => records::check_version(schema, MANIFEST_SCHEMA, "manifest")?,
    }
    let manifest = Manifest::deserialize(header).map_err(invalid)?;
    if manifest.generation != generation {
        return Err(invalid(format!(
            "manifest generation {} does not match checkpoint generation {generation}",
            manifest.generation
        )));
    }
    let rules = match &manifest.rules {
        Some(rules) => rules.resolve(schema)?,
        None if schema == LEGACY_MANIFEST_SCHEMA => RulePreset::NativeV1,
        None => {
            return Err(invalid(format!(
                "missing rules in manifest schema {schema}"
            )));
        }
    };
    if schema >= RANDOM_STREAMS_MANIFEST_SCHEMA {
        let record = manifest
            .random_streams
            .as_ref()
            .ok_or_else(|| invalid("missing random streams record"))?;
        records::check_version(record.schema, RANDOM_STREAMS_SCHEMA, "random streams")?;
    } else if header.get("random_streams").is_some() {
        return Err(invalid(format!(
            "manifest schema {schema} must not declare random streams record"
        )));
    }
    if schema >= SCHEDULED_TASKS_MANIFEST_SCHEMA {
        let record = manifest
            .scheduled_tasks
            .as_ref()
            .ok_or_else(|| invalid("missing scheduled tasks record"))?;
        record.validate()?;
    } else if header.get("scheduled_tasks").is_some() {
        return Err(invalid(format!(
            "manifest schema {schema} must not declare scheduled tasks record"
        )));
    }
    match (&manifest.java_redstone, rules.uses_java_block_events()) {
        (Some(record), true) => record.validate()?,
        (None, true) => return Err(invalid("missing Java redstone record")),
        (Some(_), false) => return Err(invalid("unexpected Java redstone record")),
        (None, false) => {}
    }
    Ok((manifest, rules))
}

fn current(root: &Path) -> io::Result<Option<u64>> {
    let pointer = root.join("CURRENT");
    match read_limited(&pointer, 24) {
        Ok(bytes) => {
            let text = std::str::from_utf8(&bytes).map_err(invalid)?;
            let generation = text
                .strip_suffix('\n')
                .and_then(|text| text.parse::<u64>().ok())
                .filter(|&n| n != 0)
                .ok_or_else(|| invalid("invalid CURRENT generation"))?;
            Ok(Some(generation))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match pointer.symlink_metadata() {
                Ok(metadata) if metadata.is_symlink() => {
                    Err(invalid("CURRENT points to a missing file"))
                }
                Ok(_) => Ok(None), // A writer may have published since the first read.
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

fn next_generation(directory: &Path) -> io::Result<u64> {
    let mut max = 0;
    for entry in directory.read_dir()? {
        let entry = entry?;
        if let Some(generation) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u64>().ok())
        {
            max = max.max(generation);
        }
    }
    max.checked_add(1)
        .ok_or_else(|| invalid("checkpoint generation exhausted"))
}

fn group_regions(chunks: &[SavedChunk]) -> BTreeMap<[i64; 2], Vec<&SavedChunk>> {
    let mut regions: BTreeMap<[i64; 2], Vec<&SavedChunk>> = BTreeMap::new();
    for chunk in chunks {
        regions
            .entry(region::coordinates(chunk.position))
            .or_default()
            .push(chunk);
    }
    for chunks in regions.values_mut() {
        chunks.sort_unstable_by_key(|chunk| chunk.position);
    }
    regions
}

fn read_limited(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid(format!(
            "{} exceeds its size limit",
            path.display()
        )));
    }
    Ok(bytes)
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub(crate) fn invalid(message: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_the_published_legacy_manifest_and_player_without_advancing_the_world() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        let mut server = Server::playable(42).unwrap();
        assert_eq!(save(&source, None, &server).unwrap(), 1);
        server.step(vec![]);
        let checkpoint = server.checkpoint().unwrap();
        assert_eq!(save(&source, Some(1), &server).unwrap(), 2);
        fs::write(source.join("checkpoints/1/manifest.json"), b"damaged").unwrap();

        let published = source.join("checkpoints/2");
        let mut header = read_manifest(&published).unwrap();
        let canonical_player = fs::read(published.join("player.cbor")).unwrap();
        let mut legacy_player = canonical_player.clone();
        assert_eq!(legacy_player[..3], [0x86, 2, 0x88]);
        legacy_player[1..3].copy_from_slice(&[1, 0x87]);
        // The six fixed-width motion values and grounded flag precede the flight flag.
        const FLIGHT_FLAG_OFFSET: usize = 3 + 6 * 9 + 1;
        assert_eq!(legacy_player.remove(FLIGHT_FLAG_OFFSET), 0);
        fs::write(published.join("player.cbor"), &legacy_player).unwrap();
        header["schema"] = LEGACY_MANIFEST_SCHEMA.into();
        header.as_object_mut().unwrap().remove("rules").unwrap();
        header
            .as_object_mut()
            .unwrap()
            .remove("random_streams")
            .unwrap();
        header
            .as_object_mut()
            .unwrap()
            .remove("scheduled_tasks")
            .unwrap();
        header["player_crc32c"] = region::crc32c(&legacy_player).into();
        let legacy_manifest = serde_json::to_vec(&header).unwrap();
        fs::write(published.join("manifest.json"), &legacy_manifest).unwrap();

        assert_eq!(migrate_checkpoint(&source, &destination).unwrap(), 2);
        let (generation, migrated) = load_existing(&destination).unwrap();
        assert_eq!(generation, 2);
        assert_eq!(migrated.checkpoint().unwrap(), checkpoint);
        assert!(!destination.join("checkpoints/1").exists());
        assert_eq!(current(&source).unwrap(), Some(2));
        assert_eq!(
            fs::read(published.join("manifest.json")).unwrap(),
            legacy_manifest
        );
        assert_eq!(
            fs::read(published.join("player.cbor")).unwrap(),
            legacy_player
        );

        let upgraded = destination.join("checkpoints/2");
        let (manifest, _) = manifest(&upgraded, 2).unwrap();
        assert_eq!(manifest.schema, MANIFEST_SCHEMA);
        assert_eq!(
            fs::read(upgraded.join("player.cbor")).unwrap(),
            canonical_player
        );
        for coordinates in manifest.regions {
            let name = region::file_name(coordinates);
            assert_eq!(
                fs::read(upgraded.join(&name)).unwrap(),
                fs::read(published.join(&name)).unwrap()
            );
        }
    }
}
