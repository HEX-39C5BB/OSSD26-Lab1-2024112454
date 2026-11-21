use std::collections::BTreeMap;
use std::io;

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const NATIVE_BUNDLE: &[u8] = include_bytes!("../../../assets/native.rpak");

#[derive(Deserialize)]
struct Index {
    id: String,
    version: String,
    source: String,
    dependencies: BTreeMap<String, String>,
    materials: BTreeMap<String, Material>,
    #[serde(default)]
    images: BTreeMap<String, Material>,
    audio: BTreeMap<String, Audio>,
}

#[derive(Deserialize)]
struct Entry {
    source: String,
    offset: usize,
    length: usize,
    sha256: String,
}

#[derive(Deserialize)]
struct Material {
    #[serde(flatten)]
    entry: Entry,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct Audio {
    #[serde(flatten)]
    entry: Entry,
    channels: u8,
    rate: u32,
}

pub struct Pack<'a> {
    index: Index,
    bytes: &'a [u8],
}

pub struct TextureAtlas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    tiles: BTreeMap<String, [f32; 4]>,
}

impl TextureAtlas {
    pub fn tile(&self, name: &str) -> Option<[f32; 4]> {
        self.tiles.get(name).copied()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AudioClip<'a> {
    /// Interleaved signed 16-bit little-endian samples.
    pub pcm: &'a [u8],
    pub channels: u8,
    pub sample_rate: u32,
}

impl<'a> Pack<'a> {
    pub fn open(bytes: &'a [u8]) -> io::Result<Self> {
        let body = bytes
            .strip_prefix(b"RCPACK1\0")
            .ok_or_else(|| invalid("unsupported pack header"))?;
        let (length, body) = body
            .split_at_checked(4)
            .ok_or_else(|| invalid("truncated pack"))?;
        let length = u32::from_le_bytes(length.try_into().unwrap()) as usize;
        let (index, bytes) = body
            .split_at_checked(length)
            .ok_or_else(|| invalid("truncated pack index"))?;
        if length > 1024 * 1024 || bytes.len() > 64 * 1024 * 1024 {
            return Err(invalid("pack exceeds size limits"));
        }
        let index: Index = serde_json::from_slice(index).map_err(invalid)?;
        if !namespaced(&index.id) || index.version.is_empty() || index.source != "manifest.toml" {
            return Err(invalid("invalid pack identity or source"));
        }
        if index
            .dependencies
            .iter()
            .any(|(id, version)| !namespaced(id) || version.is_empty() || id == &index.id)
        {
            return Err(invalid("invalid pack dependencies"));
        }
        let pack = Self { index, bytes };
        let mut entries = Vec::new();
        for (name, material) in pack.index.materials.iter().chain(&pack.index.images) {
            if !namespaced(name)
                || !(1..=1024).contains(&material.width)
                || !(1..=1024).contains(&material.height)
                || material.entry.length != material.width as usize * material.height as usize * 4
            {
                return Err(invalid("invalid image dimensions or resource id"));
            }
            entries.push(&material.entry);
        }
        for (name, audio) in &pack.index.audio {
            if !namespaced(name)
                || !(1..=2).contains(&audio.channels)
                || !(8000..=192000).contains(&audio.rate)
                || audio.entry.length == 0
                || !audio
                    .entry
                    .length
                    .is_multiple_of(2 * usize::from(audio.channels))
            {
                return Err(invalid("invalid PCM audio entry"));
            }
            entries.push(&audio.entry);
        }
        entries.sort_unstable_by_key(|entry| entry.offset);
        let mut next = 0;
        for entry in entries {
            if entry.offset != next || entry.source.is_empty() {
                return Err(invalid("non-contiguous or untracked resource entry"));
            }
            let data = pack.entry(entry)?;
            if format!("{:x}", Sha256::digest(data)) != entry.sha256 {
                return Err(invalid(format!(
                    "asset checksum mismatch: {}",
                    entry.source
                )));
            }
            next += entry.length;
        }
        if next != pack.bytes.len() {
            return Err(invalid("unindexed pack payload"));
        }
        Ok(pack)
    }

    pub fn dependencies(&self) -> &BTreeMap<String, String> {
        &self.index.dependencies
    }

    pub fn id(&self) -> &str {
        &self.index.id
    }

    pub fn atlas(&self) -> io::Result<TextureAtlas> {
        const WIDTH: u32 = 1024;
        // The Java 1.20.1 icon set uses 1,255 64x64 sprites. Sixteen fit per
        // 1024-pixel row, so the atlas needs roughly 82 rows (5,248 pixels)
        // before the terrain materials are added.
        const MAX_HEIGHT: u32 = 8192;
        let mut x = 0;
        let mut y = 0;
        let mut row_height = 1;
        let mut tiles = BTreeMap::new();
        let mut placements = Vec::new();
        for (name, image) in self.index.materials.iter().chain(&self.index.images) {
            if x + image.width > WIDTH {
                x = 0;
                y += row_height;
                row_height = 1;
            }
            row_height = row_height.max(image.height);
            if y + row_height > MAX_HEIGHT {
                return Err(invalid("resource pack textures exceed atlas capacity"));
            }
            if tiles
                .insert(
                    name.clone(),
                    [x as f32, y as f32, image.width as f32, image.height as f32],
                )
                .is_some()
            {
                return Err(invalid("duplicate texture ID across materials and images"));
            }
            placements.push((x, y, image));
            x += image.width;
        }
        let height = y + row_height;
        let mut pixels = vec![0; (WIDTH * height * 4) as usize];
        for (x, y, image) in placements {
            let image_stride = image.width as usize * 4;
            let data = self.entry(&image.entry)?;
            for (row, source) in data.chunks_exact(image_stride).enumerate() {
                let offset = ((y as usize + row) * WIDTH as usize + x as usize) * 4;
                pixels[offset..offset + image_stride].copy_from_slice(source);
            }
        }
        Ok(TextureAtlas {
            width: WIDTH,
            height,
            pixels,
            tiles,
        })
    }

    pub fn colors(&self) -> BTreeMap<&str, [f32; 3]> {
        self.index
            .materials
            .iter()
            .map(|(name, material)| {
                let data = self.entry(&material.entry).expect("validated material");
                let mut sums = [0_u32; 3];
                for pixel in data.as_chunks::<4>().0 {
                    for (sum, channel) in sums.iter_mut().zip(pixel) {
                        *sum += u32::from(*channel);
                    }
                }
                let scale = 1.0 / (255.0 * (data.len() / 4) as f32);
                (name.as_str(), sums.map(|sum| sum as f32 * scale))
            })
            .collect()
    }

    pub fn audio(&self, name: &str) -> Option<AudioClip<'_>> {
        let audio = self.index.audio.get(name)?;
        Some(AudioClip {
            pcm: self.entry(&audio.entry).expect("validated audio entry"),
            channels: audio.channels,
            sample_rate: audio.rate,
        })
    }

    fn entry(&self, entry: &Entry) -> io::Result<&[u8]> {
        entry
            .offset
            .checked_add(entry.length)
            .and_then(|end| self.bytes.get(entry.offset..end))
            .ok_or_else(|| invalid("invalid asset byte range"))
    }
}

fn namespaced(id: &str) -> bool {
    id.split_once(':').is_some_and(|(namespace, path)| {
        !namespace.is_empty()
            && !path.is_empty()
            && [namespace, path].into_iter().all(|part| {
                part.bytes().all(|c| {
                    c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || matches!(c, b'_' | b'-' | b'.' | b'/')
                })
            })
    })
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_native_pack_and_rejects_changed_payload() {
        let pack = Pack::open(NATIVE_BUNDLE).unwrap();
        assert_eq!(pack.colors().len(), 3);
        assert!(pack.colors()["rustcraft_content:basalt"][2] > 0.7);
        let mut corrupt = NATIVE_BUNDLE.to_vec();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(Pack::open(&corrupt).is_err());
    }

    #[test]
    fn loads_the_three_native_gameplay_clips() {
        let pack = Pack::open(NATIVE_BUNDLE).unwrap();
        for (name, bytes) in [
            ("rustcraft_content:block_place", 5280),
            ("rustcraft_content:block_break", 7680),
            ("rustcraft_content:footstep", 4560),
        ] {
            let clip = pack.audio(name).unwrap();
            assert_eq!(
                (clip.channels, clip.sample_rate, clip.pcm.len()),
                (1, 24000, bytes)
            );
            assert!(clip.pcm.iter().any(|&byte| byte != 0));
        }
    }
}
