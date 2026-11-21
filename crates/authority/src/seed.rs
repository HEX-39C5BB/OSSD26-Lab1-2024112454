use rustcraft_world::ChunkPos;

use crate::RuleVersion;

/// Stable scope identifiers used to derive simulation seeds.
/// Dimension identifiers name saved instances, rather than runtime handles.
#[derive(Clone, Copy, Debug)]
pub enum SeedScope<'a> {
    World,
    Dimension(&'a str),
    Chunk {
        dimension: &'a str,
        position: ChunkPos,
    },
}

/// Stateless seed derivation for a world's root seed and locked rule preset.
#[derive(Clone, Copy, Debug)]
pub struct WorldSeeds {
    root: u64,
    rule: SeedDerivation,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SeedDerivation {
    NativeV1,
}

impl SeedDerivation {
    pub(crate) const fn identity(self) -> RuleVersion {
        match self {
            Self::NativeV1 => RuleVersion {
                id: "rustcraft:seed_derivation",
                version: 1,
            },
        }
    }

    pub(crate) const fn for_world(self, root: u64) -> WorldSeeds {
        WorldSeeds { root, rule: self }
    }
}

impl WorldSeeds {
    /// Derive a seed for a world, dimension or chunk without consuming RNG state.
    pub fn for_scope(self, scope: SeedScope<'_>) -> u64 {
        match self.rule {
            SeedDerivation::NativeV1 => scope_v1(self.root, scope),
        }
    }

    /// Derive a seed for a stable, namespaced system within the given scope.
    pub fn for_system(self, scope: SeedScope<'_>, system: &str) -> u64 {
        match self.rule {
            SeedDerivation::NativeV1 => named_v1(scope_v1(self.root, scope), 4, system),
        }
    }
}

fn scope_v1(root: u64, scope: SeedScope<'_>) -> u64 {
    let world = derive_v1(root, 1, []);
    match scope {
        SeedScope::World => world,
        SeedScope::Dimension(id) => named_v1(world, 2, id),
        SeedScope::Chunk {
            dimension,
            position,
        } => derive_v1(
            named_v1(world, 2, dimension),
            3,
            position
                .x
                .to_le_bytes()
                .into_iter()
                .chain(position.z.to_le_bytes()),
        ),
    }
}

fn named_v1(parent: u64, domain: u8, id: &str) -> u64 {
    derive_v1(
        parent,
        domain,
        (id.len() as u64)
            .to_le_bytes()
            .into_iter()
            .chain(id.bytes()),
    )
}

fn derive_v1(parent: u64, domain: u8, payload: impl IntoIterator<Item = u8>) -> u64 {
    // FNV-1a over the explicitly encoded key, followed by the SplitMix64 finalizer.
    let bytes = parent
        .to_le_bytes()
        .into_iter()
        .chain([domain])
        .chain(payload);
    let hash = bytes.fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    mix_v1(hash)
}

pub(crate) fn mix_v1(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RulePreset, Server};

    #[test]
    fn native_seed_derivation_matches_locked_vectors() {
        let seeds = Server::playable(42).unwrap().world_seeds();
        let world = SeedScope::World;
        let dimension = SeedScope::Dimension("rustcraft:overworld");
        let chunk = SeedScope::Chunk {
            dimension: "rustcraft:overworld",
            position: ChunkPos { x: -7, z: 10 },
        };
        assert_eq!(seeds.for_scope(world), 0x54ae_7685_c80e_0532);
        assert_eq!(seeds.for_scope(dimension), 0x06c2_2213_2233_18f1);
        assert_eq!(seeds.for_scope(chunk), 0x3a19_9845_4939_9752);
        assert_eq!(
            seeds.for_system(world, "rustcraft:features"),
            0xc075_f29c_130b_0ffc
        );
        assert_eq!(
            seeds.for_system(dimension, "rustcraft:features"),
            0xdf18_57d0_35fc_54ab
        );
        assert_eq!(
            seeds.for_system(chunk, "rustcraft:features"),
            0xdbe4_b4dd_d074_e606
        );
        assert_eq!(
            seeds.for_system(chunk, "rustcraft:loot"),
            0x5ef0_fc4c_ad9b_50aa
        );
        assert_eq!(
            seeds.for_scope(SeedScope::Dimension("rustcraft:洞穴")),
            0x9aa8_3b62_0006_a0c0
        );
        assert_eq!(seeds.for_scope(chunk), 0x3a19_9845_4939_9752);
        assert_eq!(
            RulePreset::NativeV1.world_seeds(u64::MAX).for_scope(world),
            0xc6c9_dfb1_0b40_c748
        );
    }
}
