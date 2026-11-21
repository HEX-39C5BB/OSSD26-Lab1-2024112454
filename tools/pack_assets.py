"""Build a deterministic RustCraft resource bundle from a source directory."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import tomllib
import wave

from PIL import Image


def inside(root, name, suffix):
    relative = Path(name)
    if relative.is_absolute() or relative.suffix != suffix:
        raise ValueError(f"expected a relative {suffix} path: {name}")
    path = (root / relative).resolve()
    if not path.is_relative_to(root):
        raise ValueError(f"path escapes asset pack: {name}")
    return path


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key {key}")
        result[key] = value
    return result


def build(root):
    root = root.resolve()
    with (root / "manifest.toml").open("rb") as file:
        manifest = tomllib.load(file)
    if not manifest["id"] or not manifest["version"]:
        raise ValueError("pack id and version are required")
    materials_path = inside(root, manifest["materials"], ".json")
    materials = json.loads(materials_path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)
    index = {
        "id": manifest["id"],
        "version": manifest["version"],
        "source": "manifest.toml",
        "dependencies": manifest.get("dependencies", {}),
        "materials": {},
        "audio": {},
    }
    payload = bytearray()

    def append(data):
        offset = len(payload)
        payload.extend(data)
        return {"offset": offset, "length": len(data), "sha256": hashlib.sha256(data).hexdigest()}

    def pack_images(sources, *, opaque):
        entries = {}
        for name, source in sorted(sources.items()):
            path = inside(root, source, ".png")
            with Image.open(path, formats=["PNG"]) as image:
                if image.n_frames != 1 or any(not 1 <= size <= 1024 for size in image.size):
                    raise ValueError(f"unsupported PNG dimensions or animation: {source}")
                rgba = image.convert("RGBA")
                pixels = rgba.tobytes()
                if opaque and min(pixels[3::4]) != 255:
                    raise ValueError(f"opaque terrain material required: {source}")
                entries[name] = {
                    **append(pixels),
                    "source": source,
                    "width": rgba.width,
                    "height": rgba.height,
                }
        return entries

    index["materials"] = pack_images(materials, opaque=True)
    if "images" in manifest:
        index["images"] = pack_images(manifest["images"], opaque=False)
    for name, source in sorted(manifest.get("audio", {}).items()):
        path = inside(root, source, ".wav")
        with wave.open(str(path), "rb") as sound:
            rate = sound.getframerate()
            count = sound.getnframes()
            channels = sound.getnchannels()
            if sound.getsampwidth() != 2 or channels not in (1, 2) or not 8000 <= rate <= 192000:
                raise ValueError(f"expected 16-bit mono or stereo PCM: {source}")
            if not 0 < count <= 60 * rate:
                raise ValueError(f"unsupported WAV duration: {source}")
            pcm = sound.readframes(count)
            if len(pcm) != count * channels * 2:
                raise ValueError(f"truncated WAV: {source}")
            index["audio"][name] = {**append(pcm), "source": source, "rate": rate, "channels": channels}
    if len(payload) > 64 * 1024 * 1024:
        raise ValueError("pack payload exceeds 64 MiB")
    header = json.dumps(index, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return b"RCPACK1\0" + struct.pack("<I", len(header)) + header + payload


def write_pack(source, output):
    content = build(source)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=output.parent, delete=False) as temp:
        name = Path(temp.name)
        try:
            temp.write(content)
            temp.close()
            name.replace(output)
        finally:
            name.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    write_pack(args.source, args.output)


if __name__ == "__main__":
    main()
