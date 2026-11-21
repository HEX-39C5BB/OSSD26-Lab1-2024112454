"""Stage and build the local Java 1.20.1 flat-world resource pack."""

import json
from pathlib import Path
import shutil
import subprocess

from pack_assets import write_pack
from java_item_icons import stage_icons


MATERIALS = {
    "bedrock": "bedrock",
    "dirt": "dirt",
    "grass_block": "grass_block_side",
    "smooth_stone": "smooth_stone",
    "redstone_lamp": "redstone_lamp",
    "redstone_lamp_on": "redstone_lamp_on",
    "repeater": "repeater",
    "repeater_on": "repeater_on",
    "piston": "piston_side",
}
IMAGES = (
    "block/grass_block_top", "block/grass_block_side_overlay", "colormap/grass",
    "block/lever", "block/redstone_dust_dot", "block/redstone_dust_line0",
    "block/redstone_dust_line1", "block/redstone_dust_overlay", "item/redstone",
    "item/repeater", "block/piston_bottom", "block/piston_inner", "block/piston_top",
    "block/piston_top_sticky",
    "block/observer_front", "block/observer_back", "block/observer_back_on",
    "block/observer_side", "block/observer_top",
    "block/comparator", "block/comparator_on",
    "block/cake_top", "block/cake_bottom", "block/cake_side", "block/cake_inner",
    "block/stone", "block/oak_planks", "block/gold_block", "block/iron_block",
    "block/dispenser_front", "block/dispenser_front_vertical",
    "block/dropper_front", "block/dropper_front_vertical",
    "block/furnace_side", "block/furnace_top",
    "block/cobblestone", "block/redstone_torch", "block/redstone_torch_off",
    "environment/clouds", "environment/sun", "environment/moon_phases",
)
AUDIO = {
    "rustcraft_content:block_place": "dig/stone1",
    "rustcraft_content:block_break": "dig/grass1",
    "rustcraft_content:footstep": "step/grass1",
    "rustcraft_java:lever_click": "random/click",
    "rustcraft_java:piston_retract": "tile/piston/in",
    "rustcraft_java:piston_extend": "tile/piston/out",
}


def copy_file(source, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)


def main():
    repository = Path(__file__).resolve().parent.parent
    local = repository / "local-resources" / "minecraft-java" / "1.20.1"
    extracted = json.loads((local / "extraction.json").read_text(encoding="utf-8"))
    if extracted["java_version"] != "1.20.1":
        raise ValueError("expected extracted Java 1.20.1 resources")
    source_pack = local / "flat-pack-source"
    assets = local / "client-assets" / "assets" / "minecraft"
    indexed = local / "indexed-assets" / "minecraft"

    materials = {f"rustcraft_java:{name}": f"textures/block/{texture}.png"
                 for name, texture in MATERIALS.items()}
    images = {f"rustcraft_java:{name}": f"textures/{name}.png" for name in IMAGES}
    for relative in sorted(set(materials.values()) | set(images.values())):
        copy_file(assets / relative, source_pack / relative)
    (source_pack / "materials.json").write_text(json.dumps(materials, indent=2) + "\n", encoding="utf-8")

    images.update(stage_icons(assets, source_pack))

    audio = {}
    for name, source in AUDIO.items():
        original = f"sounds/{source}.ogg"
        wav = f"sounds/{source}.wav"
        copy_file(indexed / original, source_pack / original)
        subprocess.run([
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
            "-i", str(source_pack / original), "-map_metadata", "-1", "-ar", "48000",
            "-ac", "1", "-c:a", "pcm_s16le", str(source_pack / wav),
        ], check=True)
        audio[name] = wav

    manifest = [
        'id = "rustcraft_java:classic_flat"', 'version = "1.20.1"',
        'materials = "materials.json"',
    ]
    for section, entries in (("images", images), ("audio", audio)):
        manifest.extend(("", f"[{section}]"))
        manifest.extend(f"{json.dumps(name)} = {json.dumps(path)}" for name, path in sorted(entries.items()))
    (source_pack / "manifest.toml").write_text("\n".join(manifest) + "\n", encoding="utf-8")
    output = local / "classic-flat.rpak"
    write_pack(source_pack, output)
    print(f"{output.relative_to(repository)}: {len(materials)} materials, {len(images)} images, {len(audio)} sounds")


if __name__ == "__main__":
    main()
