use rustcraft_mod_api::Registry;
use rustcraft_world::VerticalRange;
use rustcraft_world::generation::{ClassicSuperflat, FlatTerrain, WorldGenerator};

use crate::interaction::BlockInventory;
use crate::player::PlayerMovement;
use crate::random::RandomAlgorithm;
use crate::schedule::{self, Phase};
use crate::seed::{SeedDerivation, WorldSeeds};

/// A resolved, supported combination of simulation rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RulePreset {
    NativeV1,
    JavaFlatV1,
    JavaFlatV2,
    JavaFlatV3,
}

/// The stable identity and revision of a preset or a rule implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleVersion {
    pub id: &'static str,
    pub version: u32,
}

impl RulePreset {
    pub const fn identity(self) -> RuleVersion {
        match self {
            Self::NativeV1 => RuleVersion {
                id: "rustcraft:native",
                version: 1,
            },
            Self::JavaFlatV1 | Self::JavaFlatV2 | Self::JavaFlatV3 => RuleVersion {
                id: "rustcraft:java_1_20_1_flat",
                version: match self {
                    Self::JavaFlatV3 => 3,
                    Self::JavaFlatV2 => 2,
                    _ => 1,
                },
            },
        }
    }

    pub fn resolve(id: &str, version: u32) -> Result<Self, String> {
        [
            Self::NativeV1,
            Self::JavaFlatV1,
            Self::JavaFlatV2,
            Self::JavaFlatV3,
        ]
        .into_iter()
        .find(|rule| {
            let identity = rule.identity();
            identity.id == id && identity.version == version
        })
        .ok_or_else(|| format!("unsupported rule preset {id:?} version {version}"))
    }

    pub fn from_cli(name: &str) -> Result<Self, String> {
        match name {
            "native" => Ok(Self::NativeV1),
            "java-1.20.1" => Ok(Self::JavaFlatV1),
            "java-1.20.1-reference" => Ok(Self::JavaFlatV2),
            "java-1.20.1-reference-v3" => Ok(Self::JavaFlatV3),
            _ => Err(format!(
                "unknown preset {name:?}; expected native, java-1.20.1, java-1.20.1-reference or java-1.20.1-reference-v3"
            )),
        }
    }

    pub const fn is_java_flat(self) -> bool {
        matches!(self, Self::JavaFlatV1 | Self::JavaFlatV2 | Self::JavaFlatV3)
    }

    pub const fn uses_java_block_events(self) -> bool {
        matches!(self, Self::JavaFlatV2 | Self::JavaFlatV3)
    }

    pub(crate) const fn uses_java_piston_save(self) -> bool {
        matches!(self, Self::JavaFlatV3)
    }

    pub const fn default_range(self) -> VerticalRange {
        match self {
            Self::NativeV1 => VerticalRange::FLAT_DEFAULT,
            Self::JavaFlatV1 | Self::JavaFlatV2 | Self::JavaFlatV3 => VerticalRange {
                min: ClassicSuperflat::BOTTOM,
                max_exclusive: 320,
            },
        }
    }

    pub const fn world_generator(self) -> RuleVersion {
        match self {
            Self::NativeV1 => RuleVersion {
                id: FlatTerrain::ID,
                version: FlatTerrain::VERSION,
            },
            Self::JavaFlatV1 | Self::JavaFlatV2 | Self::JavaFlatV3 => RuleVersion {
                id: ClassicSuperflat::ID,
                version: ClassicSuperflat::VERSION,
            },
        }
    }

    pub const fn tick_schedule(self) -> RuleVersion {
        RuleVersion {
            id: "rustcraft:tick_schedule",
            version: if self.uses_java_block_events() { 2 } else { 1 },
        }
    }

    pub const fn player_movement(self) -> RuleVersion {
        self.movement().identity()
    }

    pub(crate) const fn movement(self) -> PlayerMovement {
        PlayerMovement::NativeV1
    }

    pub const fn block_inventory(self) -> RuleVersion {
        self.interaction().identity()
    }

    pub(crate) const fn interaction(self) -> BlockInventory {
        match self {
            Self::NativeV1 => BlockInventory::NativeV1,
            Self::JavaFlatV1 | Self::JavaFlatV2 | Self::JavaFlatV3 => {
                BlockInventory::JavaSurvivalV1
            }
        }
    }

    pub(crate) const fn inventory_size(self) -> (usize, usize) {
        match self {
            Self::NativeV1 => (crate::SLOT_COUNT, crate::SLOT_COUNT),
            Self::JavaFlatV1 | Self::JavaFlatV2 | Self::JavaFlatV3 => {
                (crate::JAVA_SLOT_COUNT, crate::JAVA_HOTBAR_COUNT)
            }
        }
    }

    pub const fn seed_derivation(self) -> RuleVersion {
        self.seed_rule().identity()
    }

    pub const fn random_stream(self) -> RuleVersion {
        self.random_algorithm().identity()
    }

    pub(crate) const fn random_algorithm(self) -> RandomAlgorithm {
        RandomAlgorithm::NativeV1
    }

    pub const fn world_seeds(self, root: u64) -> WorldSeeds {
        self.seed_rule().for_world(root)
    }

    const fn seed_rule(self) -> SeedDerivation {
        SeedDerivation::NativeV1
    }

    pub(crate) fn build_tick_schedule(self) -> Result<Vec<Phase>, &'static str> {
        schedule::build(self.uses_java_block_events())
    }

    pub(crate) fn create_world_generator(self, registry: &Registry) -> Box<dyn WorldGenerator> {
        let block = |name| registry.block_id(name).expect("builtin terrain block");
        match self {
            Self::NativeV1 => Box::new(FlatTerrain {
                empty: block("rustcraft:empty"),
                bedrock: block("rustcraft_content:basalt"),
                soil: block("rustcraft_content:loam"),
                surface: block("rustcraft_content:lichenstone"),
            }),
            Self::JavaFlatV1 | Self::JavaFlatV2 | Self::JavaFlatV3 => Box::new(ClassicSuperflat {
                empty: block("rustcraft:empty"),
                bedrock: block("rustcraft_java:bedrock"),
                dirt: block("rustcraft_java:dirt"),
                grass: block("rustcraft_java:grass_block"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_v1_runs_the_locked_tick_order() {
        assert_eq!(
            RulePreset::NativeV1.build_tick_schedule().unwrap(),
            [
                Phase::Commit,
                Phase::SimulatePlayer,
                Phase::RefreshChunkView,
                Phase::Publish
            ],
        );
    }
}
