"""Bake Java item models into local GUI sprites without altering sources.

Generated items retain their original layer pixels. Block items use the inherited
model's elements, face UVs and GUI transform. Entity-backed models and models
without geometry use a representative texture so every registered item still
gets a stable 64x64 icon. Only referenced files are staged.
"""
import hashlib
import json
import math
from pathlib import Path
import shutil

from PIL import Image

ICON_SIZE = 64
ITEMS_FILE = Path(__file__).with_name("java_items_1_20_1.txt")
COLORS = (
    "light_blue",
    "light_gray",
    "white",
    "orange",
    "magenta",
    "yellow",
    "lime",
    "pink",
    "gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
)
FLAT_FALLBACKS = {
    "shield": ("entity/shield/base", None),
    "skeleton_skull": ("entity/skeleton/skeleton", (8, 8, 16, 16)),
    "wither_skeleton_skull": ("entity/skeleton/wither_skeleton", (8, 8, 16, 16)),
    "zombie_head": ("entity/zombie/zombie", (8, 8, 16, 16)),
    "creeper_head": ("entity/creeper/creeper", (8, 8, 16, 16)),
    "dragon_head": ("entity/enderdragon/dragon", (0, 0, 16, 16)),
    "player_head": ("entity/player/wide/steve", (8, 8, 16, 16)),
    "piglin_head": ("entity/piglin/piglin", (8, 8, 16, 16)),
}
CUBE_FALLBACKS = {
    "chest": "block/oak_planks",
    "trapped_chest": "block/oak_planks",
    "ender_chest": "block/obsidian",
    "conduit": "block/conduit",
    "decorated_pot": "block/terracotta",
    "shulker_box": "block/shulker_box",
}


def load_items(path=ITEMS_FILE):
    items = tuple(
        line.strip()
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    )
    if not items:
        raise ValueError(f"empty item list: {path}")
    if len(items) != len(set(items)):
        raise ValueError(f"duplicate item names in {path}")
    return items


ITEMS = load_items()


def resource_path(name):
    namespace, _, local = name.partition(":")
    if not local:
        local, namespace = namespace, "minecraft"
    path = Path(local)
    if namespace != "minecraft" or path.is_absolute() or ".." in path.parts:
        raise ValueError(f"unsupported item resource: {name}")
    return path


def missing_icon(size=ICON_SIZE):
    image = Image.new("RGBA", (size, size), (0, 0, 0, 255))
    cell = max(1, size // 8)
    magenta = (248, 0, 248, 255)
    for y in range(0, size, cell):
        for x in range(0, size, cell):
            if (x // cell + y // cell) % 2 == 0:
                image.paste(magenta, (x, y, min(size, x + cell), min(size, y + cell)))
    return image


class ModelAssets:
    def __init__(self, source, staged):
        self.source = source
        self.staged = staged
        self.sources = {}
        self._models = {}
        self._textures = {}
        self._images = {}

    def file(self, relative):
        relative = Path(relative)
        key = str(relative)
        target = self.staged / relative
        if key not in self.sources:
            source = self.source / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            self.sources[key] = hashlib.sha256(source.read_bytes()).hexdigest()
        return target

    def model(self, name, ancestors=()):
        if name in ancestors:
            raise ValueError(f"cyclic item model: {name}")
        if name in self._models:
            return self._models[name]
        if name == "builtin/generated":
            result = {"generated": True}
        elif name == "builtin/entity":
            result = {"entity": True}
        else:
            relative = Path("models") / resource_path(name).with_suffix(".json")
            child = json.loads(self.file(relative).read_text(encoding="utf-8"))
            parent = self.model(child["parent"], (*ancestors, name)) if "parent" in child else {}
            result = parent | child
            for field in ("textures", "display"):
                result[field] = parent.get(field, {}) | child.get(field, {})
        self._models[name] = result
        return result

    def texture_name(self, model, name):
        seen = set()
        while name.startswith("#"):
            if name in seen:
                raise ValueError(f"cyclic texture alias: {name}")
            seen.add(name)
            name = model["textures"][name[1:]]
        return name

    def image(self, name):
        relative = Path("textures") / resource_path(name).with_suffix(".png")
        key = str(relative)
        if key not in self._images:
            with Image.open(self.file(relative)) as image:
                self._images[key] = image.convert("RGBA")
        return self._images[key]

    def texture(self, model, name):
        name = self.texture_name(model, name)
        key = name
        if key not in self._textures:
            rgba = self.image(name)
            if rgba.width != rgba.height:
                # Animated textures are vertical frame strips. Use the first
                # frame; the inventory does not animate icons.
                if rgba.width > 0 and rgba.height % rgba.width == 0:
                    rgba = rgba.crop((0, 0, rgba.width, rgba.width))
                else:
                    raise ValueError(f"nonsquare item texture: {name}")
            self._textures[key] = rgba
        return self._textures[key]


def rotate(point, axis, degrees):
    i, j = {"x": (1, 2), "y": (2, 0), "z": (0, 1)}[axis]
    result = list(point)
    sine, cosine = math.sin(math.radians(degrees)), math.cos(math.radians(degrees))
    result[i] = point[i] * cosine - point[j] * sine
    result[j] = point[i] * sine + point[j] * cosine
    return result


def face_vertices(name, lo, hi):
    x, y, z = lo
    X, Y, Z = hi
    # Each face starts at its texture's upper left, viewed from outside.
    return {
        "north": [(X, Y, z), (x, Y, z), (x, y, z), (X, y, z)],
        "south": [(x, Y, Z), (X, Y, Z), (X, y, Z), (x, y, Z)],
        "west": [(x, Y, z), (x, Y, Z), (x, y, Z), (x, y, z)],
        "east": [(X, Y, Z), (X, Y, z), (X, y, z), (X, y, Z)],
        "up": [(x, Y, z), (X, Y, z), (X, Y, Z), (x, Y, Z)],
        "down": [(x, y, Z), (X, y, Z), (X, y, z), (x, y, z)],
    }[name]


def default_uv(name, lo, hi):
    x, y, z = lo
    X, Y, Z = hi
    return {
        "down": [x, 16 - Z, X, 16 - z],
        "up": [x, z, X, Z],
        "north": [16 - X, 16 - Y, 16 - x, 16 - y],
        "south": [x, 16 - Y, X, 16 - y],
        "west": [z, 16 - Y, Z, 16 - y],
        "east": [16 - Z, 16 - Y, 16 - z, 16 - y],
    }[name]


def render_generated(assets, model, size):
    result = Image.new("RGBA", (size, size))
    for layer in range(5):
        if f"layer{layer}" not in model["textures"]:
            continue
        texture = assets.texture(model, f"#layer{layer}")
        result.alpha_composite(texture.resize((size, size), Image.Resampling.NEAREST))
    return result if result.getbbox() is not None else None


def render_elements(assets, name, model, size):
    gui = model.get("display", {}).get("gui", {})
    scale = gui.get("scale", [1, 1, 1])
    translation = gui.get("translation", [0, 0, 0])
    rotation = gui.get("rotation", [0, 0, 0])

    def transform(point, element):
        if turn := element.get("rotation"):
            origin = turn["origin"]
            point = [p - o for p, o in zip(point, origin)]
            point = rotate(point, turn["axis"], turn["angle"])
            if turn.get("rescale"):
                factor = 1 / math.cos(math.radians(turn["angle"]))
                point = [
                    p if "xyz"[i] == turn["axis"] else p * factor
                    for i, p in enumerate(point)
                ]
            point = [p + o for p, o in zip(point, origin)]
        point = [(p - 8) * s for p, s in zip(point, scale)]
        # Quaternion rotationXYZ applies the rightmost rotation first.
        for axis, angle in reversed(list(zip("xyz", rotation))):
            point = rotate(point, axis, angle)
        point = [p + t for p, t in zip(point, translation)]
        return [
            (8 + point[0]) * size / 16,
            (8 - point[1]) * size / 16,
            point[2],
        ]

    result = Image.new("RGBA", (size, size))
    pixels = result.load()
    depths = [-math.inf] * (size * size)
    for element in model.get("elements", []):
        lo, hi = element["from"], element["to"]
        for side, face in element["faces"].items():
            points = [transform(p, element) for p in face_vertices(side, lo, hi)]
            ax, ay = points[1][0] - points[0][0], points[1][1] - points[0][1]
            bx, by = points[3][0] - points[0][0], points[3][1] - points[0][1]
            determinant = ax * by - ay * bx
            if determinant <= 1e-8:
                continue
            texture = assets.texture(model, face["texture"])
            texels = texture.load()
            u0, v0, u1, v1 = face.get("uv", default_uv(side, lo, hi))
            turns = face.get("rotation", 0) // 90
            tint = (
                (0x91 / 255, 0xBD / 255, 0x59 / 255)
                if name == "grass_block" and "tintindex" in face
                else (1, 1, 1)
            )
            # GUI directional light preserves readable volume on small block icons.
            shade = {
                "up": 1.0,
                "down": 0.5,
                "north": 0.8,
                "south": 0.8,
                "east": 0.6,
                "west": 0.6,
            }[side]
            if model.get("gui_light") == "front" or not element.get("shade", True):
                shade = 1.0
            for y in range(
                max(0, math.floor(min(p[1] for p in points))),
                min(size, math.ceil(max(p[1] for p in points))),
            ):
                for x in range(
                    max(0, math.floor(min(p[0] for p in points))),
                    min(size, math.ceil(max(p[0] for p in points))),
                ):
                    dx, dy = x + 0.5 - points[0][0], y + 0.5 - points[0][1]
                    u, v = (dx * by - dy * bx) / determinant, (
                        ax * dy - ay * dx
                    ) / determinant
                    if not (0 <= u <= 1 and 0 <= v <= 1):
                        continue
                    depth = (
                        points[0][2]
                        + u * (points[1][2] - points[0][2])
                        + v * (points[3][2] - points[0][2])
                    )
                    if depth < depths[y * size + x] - 1e-7:
                        continue
                    for _ in range(turns):
                        u, v = v, 1 - u
                    tx = min(
                        texture.width - 1,
                        max(0, int((u0 + (u1 - u0) * u) * texture.width / 16)),
                    )
                    ty = min(
                        texture.height - 1,
                        max(0, int((v0 + (v1 - v0) * v) * texture.height / 16)),
                    )
                    r, g, b, a = texels[tx, ty]
                    if a < 128:
                        continue
                    pixels[x, y] = tuple(
                        round(c * t * shade) for c, t in zip((r, g, b), tint)
                    ) + (255,)
                    depths[y * size + x] = depth
    return result if result.getbbox() is not None else None


def resolved_texture(assets, model, name):
    name = assets.texture_name(model, name)
    relative = Path("textures") / resource_path(name).with_suffix(".png")
    return name if (assets.source / relative).is_file() else None


def fallback_texture(assets, name, model):
    if name in CUBE_FALLBACKS:
        return CUBE_FALLBACKS[name]
    for suffix, family in (("_banner", "wool"), ("_bed", "wool")):
        if name.endswith(suffix):
            color = name[: -len(suffix)]
            if color in COLORS:
                return f"block/{color}_{family}"
    if name == "shulker_box":
        return "block/shulker_box"
    if name.endswith("_shulker_box"):
        color = name[: -len("_shulker_box")]
        if color in COLORS:
            return f"block/{color}_shulker_box"
    textures = model.get("textures", {})
    if "particle" in textures:
        return resolved_texture(assets, model, "#particle")
    for value in textures.values():
        if resolved := resolved_texture(assets, model, value):
            return resolved
    return None


def fit_icon(image, size):
    bounds = image.getbbox()
    if bounds is None:
        return missing_icon(size)
    image = image.crop(bounds)
    scale = min(size / image.width, size / image.height)
    resized = image.resize(
        (
            max(1, round(image.width * scale)),
            max(1, round(image.height * scale)),
        ),
        Image.Resampling.NEAREST,
    )
    result = Image.new("RGBA", (size, size))
    result.alpha_composite(
        resized,
        ((size - resized.width) // 2, (size - resized.height) // 2),
    )
    return result


def flat_icon(assets, texture, crop, size):
    image = assets.image(texture)
    if crop is not None:
        image = image.crop(crop)
    return fit_icon(image, size)


def all_faces(texture):
    return {
        side: {"texture": texture}
        for side in ("north", "south", "west", "east", "up", "down")
    }


def custom_icon(assets, textures, elements, size, rotation=(30, 45, 0)):
    model = {
        "textures": textures,
        "elements": elements,
        "display": {
            "gui": {
                "rotation": list(rotation),
                "translation": [0, 0, 0],
                "scale": [0.625, 0.625, 0.625],
            }
        },
    }
    return render_elements(assets, "", model, size)


def cube_icon(assets, texture, size):
    return custom_icon(
        assets,
        {"all": texture},
        [{"from": [0, 0, 0], "to": [16, 16, 16], "faces": all_faces("#all")}],
        size,
    )


def banner_icon(assets, color, size):
    return custom_icon(
        assets,
        {"cloth": f"block/{color}_wool", "stick": "block/oak_planks"},
        [
            {"from": [7, 0, 8], "to": [9, 16, 9], "faces": all_faces("#stick")},
            {"from": [1, 4, 7.5], "to": [15, 16, 8.5], "faces": all_faces("#cloth")},
        ],
        size,
    )


def bed_icon(assets, color, size):
    return custom_icon(
        assets,
        {
            "wood": "block/oak_planks",
            "cloth": f"block/{color}_wool",
            "pillow": "block/white_wool",
        },
        [
            {"from": [1, 0, 1], "to": [15, 4, 15], "faces": all_faces("#wood")},
            {"from": [1, 4, 1], "to": [15, 8, 15], "faces": all_faces("#cloth")},
            {"from": [1, 8, 1], "to": [15, 11, 5], "faces": all_faces("#pillow")},
        ],
        size,
    )


def render_fallback(assets, name, model, size):
    if name in FLAT_FALLBACKS:
        texture, crop = FLAT_FALLBACKS[name]
        try:
            return flat_icon(assets, texture, crop, size)
        except (OSError, ValueError, KeyError):
            pass
    if name.endswith("_banner"):
        color = name[: -len("_banner")]
        if color in COLORS:
            try:
                return banner_icon(assets, color, size)
            except (OSError, ValueError, KeyError):
                pass
    if name.endswith("_bed"):
        color = name[: -len("_bed")]
        if color in COLORS:
            try:
                return bed_icon(assets, color, size)
            except (OSError, ValueError, KeyError):
                pass
    texture = fallback_texture(assets, name, model)
    if texture:
        try:
            return cube_icon(assets, texture, size)
        except (OSError, ValueError, KeyError):
            pass
    return missing_icon(size)


def render(assets, name, size=ICON_SIZE):
    model = assets.model(f"item/{name}")
    if model.get("generated"):
        icon = render_generated(assets, model, size)
        if icon is not None:
            return icon
    if model.get("entity") or not model.get("elements"):
        return render_fallback(assets, name, model, size)
    icon = render_elements(assets, name, model, size)
    if icon is not None:
        return icon
    return render_fallback(assets, name, model, size)


def stage_icons(source, staged):
    assets = ModelAssets(source, staged)
    images = {}
    fallbacks = []
    errors = {}
    for name in ITEMS:
        try:
            model = assets.model(f"item/{name}")
            fallback = not model.get("generated") and (
                model.get("entity") or not model.get("elements")
            )
            icon = render(assets, name)
        except (OSError, ValueError, KeyError, IndexError) as error:
            icon = missing_icon()
            fallback = True
            errors[name] = str(error)
        relative = f"textures/gui/items/{name}.png"
        output = staged / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        icon.save(output)
        images[f"rustcraft_java:icon/{name}"] = relative
        if fallback:
            fallbacks.append(name)
    (staged / "item-icon-sources.json").write_text(
        json.dumps(
            {
                "java_version": "1.20.1",
                "items": list(ITEMS),
                "size": ICON_SIZE,
                "fallbacks": fallbacks,
                "errors": errors,
                "sources_sha256": dict(sorted(assets.sources.items())),
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(
        f"item icons: {len(images)} generated, {len(fallbacks)} fallbacks, "
        f"{len(errors)} errors"
    )
    return images
