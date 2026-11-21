use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;

use rustcraft_authority::checkpoint::SavedChunk;
use rustcraft_world::ChunkPos;

use crate::invalid;
use crate::records::{check_version, decode_chunk, encode_chunk};

const MAGIC: &[u8; 4] = b"RCRG";
const FORMAT_VERSION: u32 = 1;
const HEADER_SIZE: u64 = 12;
const INDEX_SIZE: u64 = 32;
const EDGE: i64 = 16;
const MAX_CHUNKS: u32 = (EDGE * EDGE) as u32;
const MAX_RECORD: u32 = 16 * 1024 * 1024;

pub(crate) fn coordinates(chunk: ChunkPos) -> [i64; 2] {
    [chunk.x.div_euclid(EDGE), chunk.z.div_euclid(EDGE)]
}

pub(crate) fn file_name([x, z]: [i64; 2]) -> String {
    format!("r.{x}.{z}.rcr")
}

pub(crate) fn write(path: &Path, chunks: &[&SavedChunk]) -> io::Result<()> {
    let count = u32::try_from(chunks.len()).map_err(|_| invalid("too many region chunks"))?;
    if !(1..=MAX_CHUNKS).contains(&count) {
        return Err(invalid("invalid region chunk count"));
    }
    let mut file = io::BufWriter::new(File::create(path)?);
    file.write_all(MAGIC)?;
    file.write_all(&FORMAT_VERSION.to_le_bytes())?;
    file.write_all(&count.to_le_bytes())?;
    let mut offset = HEADER_SIZE + u64::from(count) * INDEX_SIZE;
    let mut entries = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let bytes = encode_chunk(chunk);
        let length = u32::try_from(bytes.len()).map_err(|_| invalid("chunk record too large"))?;
        if length > MAX_RECORD {
            return Err(invalid("chunk record too large"));
        }
        file.write_all(&chunk.position.x.to_le_bytes())?;
        file.write_all(&chunk.position.z.to_le_bytes())?;
        file.write_all(&offset.to_le_bytes())?;
        file.write_all(&length.to_le_bytes())?;
        file.write_all(&crc32c(&bytes).to_le_bytes())?;
        entries.push(bytes);
        offset += u64::from(length);
    }
    for bytes in entries {
        file.write_all(&bytes)?;
    }
    file.flush()?;
    file.get_ref().sync_all()
}

pub(crate) fn read(path: &Path, region: [i64; 2]) -> io::Result<Vec<SavedChunk>> {
    let file = fs::File::open(path)?;
    let file_len = file.metadata()?.len();
    let mut file = io::BufReader::new(file);
    if &take::<4>(&mut file)? != MAGIC {
        return Err(invalid("invalid region magic"));
    }
    let version = u32::from_le_bytes(take(&mut file)?);
    check_version(u64::from(version), u64::from(FORMAT_VERSION), "region")?;
    let count = u32::from_le_bytes(take(&mut file)?);
    if !(1..=MAX_CHUNKS).contains(&count) {
        return Err(invalid("invalid region chunk count"));
    }

    let mut expected_offset = HEADER_SIZE + u64::from(count) * INDEX_SIZE;
    let mut previous = None;
    let mut entries = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let position = ChunkPos {
            x: i64::from_le_bytes(take(&mut file)?),
            z: i64::from_le_bytes(take(&mut file)?),
        };
        let offset = u64::from_le_bytes(take(&mut file)?);
        let length = u32::from_le_bytes(take(&mut file)?);
        let checksum = u32::from_le_bytes(take(&mut file)?);
        if coordinates(position) != region
            || previous.is_some_and(|prior| prior >= position)
            || offset != expected_offset
            || !(1..=MAX_RECORD).contains(&length)
        {
            return Err(invalid("invalid region index"));
        }
        previous = Some(position);
        expected_offset += u64::from(length);
        entries.push((position, length, checksum));
    }
    if expected_offset != file_len {
        return Err(invalid("region file length does not match index"));
    }

    let mut chunks = Vec::with_capacity(entries.len());
    for (position, length, checksum) in entries {
        let mut payload = vec![0; length as usize];
        file.read_exact(&mut payload)?;
        if crc32c(&payload) != checksum {
            return Err(invalid(format!("CRC32C mismatch for chunk {position:?}")));
        }
        let chunk = decode_chunk(&payload)?;
        if chunk.position != position {
            return Err(invalid("chunk position does not match region index"));
        }
        chunks.push(chunk);
    }
    Ok(chunks)
}

fn take<const N: usize>(file: &mut impl Read) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn crc32c(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0x82f6_3b78 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::crc32c;

    #[test]
    fn matches_crc32c_check_value() {
        assert_eq!(crc32c(b"123456789"), 0xe306_9283);
    }
}
