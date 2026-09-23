#!/usr/bin/env python3
"""Run testkit/tui/run.py and save a colored PNG for every screen it dumps (macOS fonts).

Usage: venv/bin/python shoot.py <label>   -> $OUT/png/<label>/<name>-{dark,light}.png
pyte has no faint attr: SGR 2 is mapped onto `blink` and drawn as a 50% blend.
"""
import os
import sys
import pathlib
from pathlib import Path

import pyte
from pyte import graphics
from PIL import Image, ImageDraw, ImageFont

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "testkit" / "tui"))
graphics.TEXT[2] = "+blink"
graphics.TEXT[22] = "-bold"
_orig_draw = pyte.Screen.select_graphic_rendition


def sgr(self, *attrs, **kw):
    _orig_draw(self, *attrs, **kw)
    if 22 in attrs:
        self.cursor.attrs = self.cursor.attrs._replace(blink=False)


pyte.Screen.select_graphic_rendition = sgr

import run  # noqa: E402

LABEL = sys.argv[1] if len(sys.argv) > 1 else "shot"
PNG = run.OUT / "png" / LABEL
PNG.mkdir(parents=True, exist_ok=True)

MONO = ImageFont.truetype("/System/Library/Fonts/Menlo.ttc", 14)
CJK = ImageFont.truetype("/System/Library/Fonts/Hiragino Sans GB.ttc", 14)
SYM = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial Unicode.ttf", 13)
BOLD = ImageFont.truetype("/System/Library/Fonts/Menlo.ttc", 14, index=1)
CW, CH = 9, 18

THEMES = {
    "dark": {"fg": (220, 220, 225), "bg": (24, 24, 30)},
    "light": {"fg": (40, 40, 45), "bg": (250, 250, 248)},
}
BASIC = {
    "black": (0, 0, 0), "red": (205, 49, 49), "green": (13, 188, 121),
    "brown": (229, 229, 16), "yellow": (229, 229, 16), "blue": (36, 114, 200),
    "magenta": (188, 63, 188), "cyan": (17, 168, 205), "white": (229, 229, 229),
    "brightblack": (102, 102, 102), "brightred": (241, 76, 76),
    "brightgreen": (35, 209, 139), "brightbrown": (245, 245, 67),
    "brightyellow": (245, 245, 67), "brightblue": (59, 142, 234),
    "brightmagenta": (214, 112, 214), "brightcyan": (41, 184, 219),
    "brightwhite": (255, 255, 255),
}


def color(name, default):
    if name == "default":
        return default
    if name in BASIC:
        return BASIC[name]
    try:
        return tuple(int(name[i:i + 2], 16) for i in (0, 2, 4))
    except Exception:
        return default


def blend(a, b, t):
    return tuple(int(x + (y - x) * t) for x, y in zip(a, b))


def save(screen, name):
    for theme, base in THEMES.items():
        image = Image.new("RGB", (screen.columns * CW, screen.lines * CH), base["bg"])
        draw = ImageDraw.Draw(image)
        for row in range(screen.lines):
            line = screen.buffer[row]
            for col in range(screen.columns):
                char = line[col]
                fg = color(char.fg, base["fg"])
                bg = color(char.bg, base["bg"])
                if char.reverse:
                    fg, bg = bg, fg
                if char.blink:
                    fg = blend(fg, bg, 0.5)
                x, y = col * CW, row * CH
                wide = col + 1 < screen.columns and line[col + 1].data == ""
                if bg != base["bg"]:
                    draw.rectangle([x, y, x + CW * (2 if wide else 1) - 1, y + CH - 1], fill=bg)
                text = char.data
                if not text or text == " ":
                    continue
                font = BOLD if char.bold else MONO
                code = ord(text[0])
                if code > 0x2e80:
                    font = CJK
                elif 0x2190 <= code < 0x2500 or 0x25a0 <= code < 0x2800:
                    font = SYM
                draw.text((x, y + 1), text, fill=fg, font=font)
                if char.underscore:
                    draw.line([x, y + CH - 2, x + CW - 1, y + CH - 2], fill=fg)
        image.save(PNG / f"{name}-{theme}.png")


_orig_write = pathlib.Path.write_text


def write_text(self, data, *args, **kwargs):
    result = _orig_write(self, data, *args, **kwargs)
    screen = run._VIEW.get("screen")
    if self.suffix == ".txt" and self.parent == run.OUT and screen is not None:
        try:
            save(screen, self.stem)
        except Exception as error:  # never break the walk
            print("png failed", self.stem, error, file=sys.stderr)
    return result


pathlib.Path.write_text = write_text

if __name__ == "__main__":
    run.main()
