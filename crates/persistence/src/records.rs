use std::io;

use rustcraft_authority::checkpoint::{
    PlayerMotion, SavedBlock, SavedChunk, SavedPlayer, SavedSection,
};
use rustcraft_authority::{JAVA_SLOT_COUNT, SLOT_COUNT};
use rustcraft_world::{ChunkPos, SECTION_VOLUME};

use crate::cbor::{Decoder, Encoder};
use crate::invalid;

const CHUNK_SCHEMA: u64 = 1;
const LEGACY_PLAYER_SCHEMA: u64 = 1;
const PLAYER_SCHEMA: u64 = 2;
const EXPANDED_PLAYER_SCHEMA: u64 = 3;
const GAME_MODE_PLAYER_SCHEMA: u64 = 4;

pub(crate) fn encode_chunk(chunk: &SavedChunk) -> Vec<u8> {
    let mut out = Encoder(Vec::new());
    out.array(6);
    out.unsigned(CHUNK_SCHEMA);
    out.signed(chunk.position.x);
    out.signed(chunk.position.z);
    out.unsigned(chunk.revision);
    out.unsigned(u64::from(chunk.active));
    out.array(chunk.sections.len());
    for section in &chunk.sections {
        out.array(3);
        out.signed(section.y);
        out.array(section.palette.len());
        for block in &section.palette {
            encode_block(&mut out, block);
        }
        let indices: Vec<_> = section
            .indices
            .iter()
            .flat_map(|index| index.to_le_bytes())
            .collect();
        out.bytes(&indices);
    }
    out.0
}

pub(crate) fn decode_chunk(bytes: &[u8]) -> io::Result<SavedChunk> {
    let mut input = Decoder(bytes);
    input.fixed(6)?;
    check_version(input.unsigned()?, CHUNK_SCHEMA, "chunk")?;
    let position = ChunkPos {
        x: input.signed()?,
        z: input.signed()?,
    };
    let revision = input.unsigned()?;
    let active = flag(&mut input)?;
    let count = input.array(8192)?;
    let mut sections = Vec::with_capacity(count);
    for _ in 0..count {
        input.fixed(3)?;
        let y = input.signed()?;
        let count = input.array(SECTION_VOLUME)?;
        let mut palette = Vec::with_capacity(count);
        for _ in 0..count {
            palette.push(decode_block(&mut input)?);
        }
        let index_bytes = input.bytes(SECTION_VOLUME * 2)?;
        let (indices, remainder) = index_bytes.as_chunks::<2>();
        if !remainder.is_empty() {
            return Err(invalid("palette indices have invalid byte length"));
        }
        let indices = indices
            .iter()
            .map(|&bytes| u16::from_le_bytes(bytes))
            .collect();
        sections.push(SavedSection {
            y,
            palette,
            indices,
        });
    }
    input.finish()?;
    Ok(SavedChunk {
        position,
        revision,
        active,
        sections,
    })
}

pub(crate) fn encode_player(player: &SavedPlayer) -> Vec<u8> {
    let mut out = Encoder(Vec::new());
    let expanded = player.slots.len() == JAVA_SLOT_COUNT;
    let (schema, fields) = match (player.game_mode, expanded) {
        (Some(_), _) => (GAME_MODE_PLAYER_SCHEMA, 8),
        (None, true) => (EXPANDED_PLAYER_SCHEMA, 7),
        (None, false) => (PLAYER_SCHEMA, 6),
    };
    out.array(fields);
    out.unsigned(schema);
    out.array(8);
    for value in player.motion.position {
        out.float_bits(value);
    }
    out.float_bits(player.motion.yaw);
    out.float_bits(player.motion.pitch);
    out.float_bits(player.motion.vertical_speed);
    out.unsigned(u64::from(player.motion.grounded));
    out.unsigned(u64::from(player.motion.flying));
    out.unsigned(player.entity_revision);
    out.unsigned(player.inventory_revision);
    out.unsigned(player.selected as u64);
    out.array(player.slots.len());
    for stack in &player.slots {
        encode_stack(&mut out, stack.as_ref());
    }
    if expanded || player.game_mode.is_some() {
        encode_stack(&mut out, player.carried.as_ref());
    }
    if let Some(mode) = player.game_mode {
        out.text(mode.name());
    }
    out.0
}

pub(crate) fn decode_player(bytes: &[u8]) -> io::Result<SavedPlayer> {
    let mut input = Decoder(bytes);
    let fields = input.array(8)?;
    let schema = input.unsigned()?;
    let (slot_count, current_motion) = match (schema, fields) {
        (LEGACY_PLAYER_SCHEMA, 6) => (Some(SLOT_COUNT), false),
        (PLAYER_SCHEMA, 6) => (Some(SLOT_COUNT), true),
        (EXPANDED_PLAYER_SCHEMA, 7) => (Some(JAVA_SLOT_COUNT), true),
        (GAME_MODE_PLAYER_SCHEMA, 8) => (None, true),
        _ => return Err(invalid("unsupported player schema or field count")),
    };
    input.fixed(if current_motion { 8 } else { 7 })?;
    let motion = PlayerMotion {
        position: [
            input.float_bits()?,
            input.float_bits()?,
            input.float_bits()?,
        ],
        yaw: input.float_bits()?,
        pitch: input.float_bits()?,
        vertical_speed: input.float_bits()?,
        grounded: flag(&mut input)?,
        flying: current_motion && flag(&mut input)?,
    };
    let entity_revision = input.unsigned()?;
    let inventory_revision = input.unsigned()?;
    let selected = usize::try_from(input.unsigned()?).map_err(|_| invalid("slot out of range"))?;
    let encoded_slots = input.array(JAVA_SLOT_COUNT)?;
    if slot_count.is_some_and(|count| count != encoded_slots)
        || ![SLOT_COUNT, JAVA_SLOT_COUNT].contains(&encoded_slots)
    {
        return Err(invalid("unexpected player slot count"));
    }
    let slot_count = encoded_slots;
    let slots = (0..slot_count)
        .map(|_| decode_stack(&mut input))
        .collect::<io::Result<_>>()?;
    let carried = if schema >= EXPANDED_PLAYER_SCHEMA {
        decode_stack(&mut input)?
    } else {
        None
    };
    let game_mode = if schema == GAME_MODE_PLAYER_SCHEMA {
        Some(input.text()?.parse().map_err(invalid)?)
    } else {
        None
    };
    input.finish()?;
    Ok(SavedPlayer {
        motion,
        entity_revision,
        inventory_revision,
        selected,
        slots,
        carried,
        game_mode,
    })
}

fn encode_stack(out: &mut Encoder, stack: Option<&(SavedBlock, u16)>) {
    if let Some((block, count)) = stack {
        out.array(2);
        encode_block(out, block);
        out.unsigned(u64::from(*count));
    } else {
        out.array(0);
    }
}

fn decode_stack(input: &mut Decoder<'_>) -> io::Result<Option<(SavedBlock, u16)>> {
    match input.array(2)? {
        0 => Ok(None),
        2 => {
            let block = decode_block(input)?;
            let count = u16::try_from(input.unsigned()?)
                .map_err(|_| invalid("stack quantity out of range"))?;
            Ok(Some((block, count)))
        }
        _ => Err(invalid("invalid inventory entry width")),
    }
}

fn encode_block(out: &mut Encoder, block: &SavedBlock) {
    out.array(2);
    out.text(&block.name);
    out.unsigned(u64::from(block.version));
}

fn decode_block(input: &mut Decoder<'_>) -> io::Result<SavedBlock> {
    input.fixed(2)?;
    Ok(SavedBlock {
        name: input.text()?,
        version: u32::try_from(input.unsigned()?)
            .map_err(|_| invalid("invalid block schema version"))?,
    })
}

fn flag(input: &mut Decoder<'_>) -> io::Result<bool> {
    match input.unsigned()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid("invalid CBOR flag")),
    }
}

pub(crate) fn check_version(actual: u64, expected: u64, record: &str) -> io::Result<()> {
    if actual != expected {
        return Err(invalid(format!(
            "unsupported {record} schema version {actual}"
        )));
    }
    Ok(())
}
