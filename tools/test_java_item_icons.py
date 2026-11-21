"""Synthetic model fixtures; no Minecraft resources are required."""
import json
from pathlib import Path
import struct
import tempfile
import unittest

from PIL import Image
from java_item_icons import ITEMS, ModelAssets, render

PACK = (
    Path(__file__).resolve().parent.parent
    / "local-resources/minecraft-java/1.20.1/classic-flat.rpak"
)


class ItemIcons(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.source = root / "original"
        self.assets = ModelAssets(self.source, root / "staged")

    def model(self, name, value):
        path = self.source / "models" / (name + ".json")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value))

    def texture(self, name, image):
        path = self.source / "textures" / (name + ".png")
        path.parent.mkdir(parents=True, exist_ok=True)
        image.save(path)
        return path

    def test_generated_item_preserves_pixels_alpha_and_original(self):
        self.model("item/generated", {"parent": "builtin/generated"})
        self.model("item/test", {"parent": "item/generated", "textures": {"layer0": "item/test"}})
        image = Image.new("RGBA", (16, 16))
        image.putpixel((3, 7), (10, 20, 30, 255))
        path = self.texture("item/test", image)
        original = path.read_bytes()
        self.assertEqual(render(self.assets, "test", 16).tobytes(), image.tobytes())
        self.assertEqual(path.read_bytes(), original)

    def test_inherited_block_model_uses_child_texture_and_gui_scale(self):
        self.model("block/base", {
            "display": {"gui": {"scale": [.5, .5, .5]}},
            "textures": {"all": "#skin", "skin": "block/unused"},
            "elements": [{"from": [0, 0, 0], "to": [16, 16, 16],
                          "faces": {"south": {"texture": "#all"}}}],
        })
        self.model("item/test", {"parent": "block/base", "textures": {"skin": "block/test"}})
        self.texture("block/test", Image.new("RGBA", (16, 16), (200, 100, 50, 255)))
        icon = render(self.assets, "test", 16)
        self.assertEqual(icon.getbbox(), (4, 4, 12, 12))
        self.assertEqual(icon.getpixel((8, 8)), (160, 80, 40, 255))

    def test_front_transparency_preserves_geometry_behind_it(self):
        self.model("item/test", {"elements": [
            {"from": [0, 0, 0], "to": [16, 16, 1], "faces": {"south": {"texture": "block/back"}}},
            {"from": [0, 0, 2], "to": [16, 16, 3], "faces": {"south": {"texture": "block/front"}}},
        ]})
        self.texture("block/back", Image.new("RGBA", (16, 16), (200, 0, 0, 255)))
        front = Image.new("RGBA", (16, 16))
        front.putpixel((3, 7), (0, 200, 0, 255))
        self.texture("block/front", front)
        icon = render(self.assets, "test", 16)
        self.assertEqual(icon.getpixel((3, 7)), (0, 160, 0, 255))
        self.assertEqual(icon.getpixel((8, 8)), (160, 0, 0, 255))

    def test_rejects_model_cycle(self):
        self.model("item/test", {"parent": "item/other"})
        self.model("item/other", {"parent": "item/test"})
        with self.assertRaisesRegex(ValueError, "cyclic item model"):
            render(self.assets, "test")

    def test_entity_model_uses_particle_texture_fallback(self):
        self.model(
            "item/chest",
            {"parent": "builtin/entity", "textures": {"particle": "block/oak_planks"}},
        )
        self.texture("block/oak_planks", Image.new("RGBA", (16, 16), (162, 130, 78, 255)))
        icon = render(self.assets, "chest", 16)
        self.assertIsNotNone(icon.getbbox())
        self.assertEqual(icon.getchannel("A").getextrema(), (0, 255))

    def test_banner_fallback_keeps_its_dye_color(self):
        self.model("item/red_banner", {"parent": "builtin/entity"})
        self.texture("block/red_wool", Image.new("RGBA", (16, 16), (180, 35, 35, 255)))
        self.texture("block/oak_planks", Image.new("RGBA", (16, 16), (160, 130, 80, 255)))
        icon = render(self.assets, "red_banner", 16)
        data = icon.tobytes()
        red = any(
            r > 100 and r > g * 2 and r > b * 2
            for r, g, b in zip(data[0::4], data[1::4], data[2::4])
        )
        self.assertTrue(red)

    def test_animated_generated_texture_uses_first_frame(self):
        self.model("item/generated", {"parent": "builtin/generated"})
        self.model(
            "item/test",
            {"parent": "item/generated", "textures": {"layer0": "item/test"}},
        )
        image = Image.new("RGBA", (16, 32), (0, 0, 0, 0))
        image.paste((255, 0, 0, 255), (0, 0, 16, 16))
        image.paste((0, 0, 255, 255), (0, 16, 16, 16))
        self.texture("item/test", image)
        icon = render(self.assets, "test", 16)
        self.assertEqual(icon.getpixel((8, 8)), (255, 0, 0, 255))

    def test_item_list_has_every_java_1_20_1_registry_entry(self):
        self.assertEqual(len(ITEMS), 1255)
        self.assertEqual(len(set(ITEMS)), len(ITEMS))
        self.assertIn("bedrock", ITEMS)
        self.assertIn("dragon_head", ITEMS)
        self.assertIn("zombie_spawn_egg", ITEMS)

    @unittest.skipUnless(PACK.is_file(), "local Java resource pack is not built")
    def test_local_pack_contains_every_icon(self):
        raw = PACK.read_bytes()
        self.assertEqual(raw[:8], b"RCPACK1\0")
        length = struct.unpack("<I", raw[8:12])[0]
        index = json.loads(raw[12 : 12 + length])
        payload = raw[12 + length :]
        icons = {
            name: entry
            for name, entry in index["images"].items()
            if name.startswith("rustcraft_java:icon/")
        }
        self.assertEqual(len(icons), len(ITEMS))
        for item in ITEMS:
            entry = icons[f"rustcraft_java:icon/{item}"]
            self.assertEqual((entry["width"], entry["height"]), (64, 64))
            data = payload[entry["offset"] : entry["offset"] + entry["length"]]
            self.assertEqual(len(data), 64 * 64 * 4)
            self.assertTrue(any(data[3::4]), item)

        x = y = row_height = 0
        for entry in [*index["materials"].values(), *index["images"].values()]:
            if x + entry["width"] > 1024:
                x = 0
                y += row_height
                row_height = 1
            row_height = max(row_height, entry["height"])
            x += entry["width"]
        self.assertLessEqual(y + row_height, 8192)


if __name__ == "__main__":
    unittest.main()
