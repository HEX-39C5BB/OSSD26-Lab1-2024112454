use rustcraft_mod_api::{
    BlockStateSchema, BuiltinMod, ComponentSchema, Dependency, Version, VersionReq,
};

pub(crate) const POSITION: ComponentSchema = ComponentSchema {
    name: "rustcraft:block_position",
    version: 1,
    fields: &["x", "y", "z"],
};

pub(crate) fn modules() -> [BuiltinMod; 2] {
    [
        BuiltinMod {
            id: "rustcraft",
            version: Version::new(0, 1, 0),
            dependencies: vec![],
            register: |registry| {
                registry.block(BlockStateSchema {
                    name: "rustcraft:empty",
                    version: 1,
                })?;
                registry.component(POSITION)
            },
        },
        BuiltinMod {
            id: "rustcraft_content",
            version: Version::new(0, 2, 0),
            dependencies: vec![Dependency {
                id: "rustcraft",
                version: VersionReq::parse("=0.1.0").expect("builtin core version requirement"),
            }],
            register: |registry| {
                registry.block(BlockStateSchema {
                    name: "rustcraft_content:basalt",
                    version: 1,
                })?;
                for name in ["rustcraft_content:loam", "rustcraft_content:lichenstone"] {
                    registry.block(BlockStateSchema { name, version: 1 })?;
                }
                registry.component(ComponentSchema {
                    name: "rustcraft_content:marker",
                    version: 1,
                    fields: &["value"],
                })
            },
        },
    ]
}

pub(crate) fn java_content(reference: bool) -> BuiltinMod {
    fn common(registry: &mut rustcraft_mod_api::Registrar<'_>) -> Result<(), String> {
        for name in [
            "rustcraft_java:bedrock",
            "rustcraft_java:dirt",
            "rustcraft_java:grass_block",
            "rustcraft_java:smooth_stone",
        ] {
            registry.block(BlockStateSchema { name, version: 1 })?;
        }
        crate::redstone::register(registry)
    }
    BuiltinMod {
        id: "rustcraft_java",
        version: Version::new(0, if reference { 2 } else { 1 }, 0),
        dependencies: vec![Dependency {
            id: "rustcraft",
            version: VersionReq::parse("=0.1.0").expect("builtin core version requirement"),
        }],
        register: if reference {
            |registry| {
                common(registry)?;
                crate::redstone::register_java_reference(registry)
            }
        } else {
            common
        },
    }
}
