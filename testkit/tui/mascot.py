#!/usr/bin/env python3
"""开屏吉祥物：把透明底立绘裁成头肩、缩成终端半格字符画，并出深/浅底预览。

    python3 testkit/tui/mascot.py [源图] [宽度,宽度…] [输出目录]
    python3 testkit/tui/mascot.py --export-portrait   # 重新生成 assets/mascot/portrait.png
    默认：assets/mascot/gqy-mascot-source.png  32,24  ./mascot-out

每个宽度产出：
    w<宽>.ansi  直接 `cat` 进终端看真实效果（真彩）
    w<宽>.png   放大的深底 / 浅底并排预览

半格字符 `▀` 一格上下两个像素，前景色画上半、背景色画下半，所以像素是方的，
按原图比例缩放即可。裁剪比例是对着 09-23 那张立绘调的（到领口，脸最大），
换图要重调 CROP。依赖 Pillow，只在开发机上跑，不进构建。
"""

import sys
from pathlib import Path

from PIL import Image, ImageEnhance

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_SOURCE = ROOT / "assets/mascot/gqy-mascot-source.png"
# 相对透明边裁掉后的外框：左、上、右、下
CROP = (0.10, 0.04, 0.90, 0.72)
CONTRAST = 1.1
SHARPNESS = 1.4
ALPHA_CUT = 128
DARK_BG = (30, 30, 36)
LIGHT_BG = (250, 250, 250)


def bust(source):
    image = Image.open(source).convert("RGBA")
    image = image.crop(image.getchannel("A").point(lambda v: 255 if v > 40 else 0).getbbox())
    w, h = image.size
    left, top, right, bottom = CROP
    image = image.crop((int(w * left), int(h * top), int(w * right), int(h * bottom)))
    rgb = ImageEnhance.Contrast(image.convert("RGB")).enhance(CONTRAST)
    rgb = ImageEnhance.Sharpness(rgb).enhance(SHARPNESS)
    rgb.putalpha(image.getchannel("A"))
    return rgb


def shrink(image, width):
    height = round(image.height * width / image.width)
    height += height % 2  # 半格要偶数行
    return image.resize((width, height), Image.Resampling.BOX)


def ansi(image):
    px = image.load()
    lines = []
    for y in range(0, image.height, 2):
        line = ""
        for x in range(image.width):
            top, bottom = px[x, y], px[x, y + 1]
            show_top, show_bottom = top[3] >= ALPHA_CUT, bottom[3] >= ALPHA_CUT
            if show_top and show_bottom:
                line += f"\x1b[38;2;{top[0]};{top[1]};{top[2]}m\x1b[48;2;{bottom[0]};{bottom[1]};{bottom[2]}m▀"
            elif show_top:
                line += f"\x1b[49m\x1b[38;2;{top[0]};{top[1]};{top[2]}m▀"
            elif show_bottom:
                line += f"\x1b[49m\x1b[38;2;{bottom[0]};{bottom[1]};{bottom[2]}m▄"
            else:
                line += "\x1b[0m "
        lines.append(line + "\x1b[0m")
    return "\n".join(lines) + "\n"


def preview(image, scale):
    mask = image.getchannel("A").point(lambda v: 255 if v >= ALPHA_CUT else 0)
    tiles = []
    for bg in (DARK_BG, LIGHT_BG):
        tile = Image.new("RGB", image.size, bg)
        tile.paste(image.convert("RGB"), (0, 0), mask)
        tiles.append(tile.resize((image.width * scale, image.height * scale), Image.Resampling.NEAREST))
    both = Image.new("RGB", (tiles[0].width * 2 + 16, tiles[0].height), (128, 128, 128))
    both.paste(tiles[0], (0, 0))
    both.paste(tiles[1], (tiles[0].width + 16, 0))
    return both


PORTRAIT_OUT = ROOT / "assets/mascot/portrait.png"
PORTRAIT_WIDTH = 256


def export_portrait(source=DEFAULT_SOURCE, out=PORTRAIT_OUT):
    """编译进程序的头像：裁好的头肩、宽 256 像素、透明底。

    程序运行时从它生成半格字符画（32 / 24 列）和 kitty 贴图，所以这里只做
    裁剪与调色，不做缩到终端格子那一步。源图 1.4MB 不直接嵌。
    """
    image = bust(source)
    height = round(image.height * PORTRAIT_WIDTH / image.width)
    image = image.resize((PORTRAIT_WIDTH, height), Image.Resampling.LANCZOS)
    image.save(out, optimize=True)
    print(f"导出 {out}（{image.width}×{image.height}，{out.stat().st_size} 字节）")


def main(argv):
    if argv[:1] == ["--export-portrait"]:
        export_portrait()
        return
    source = Path(argv[0]) if len(argv) > 0 else DEFAULT_SOURCE
    widths = [int(w) for w in (argv[1] if len(argv) > 1 else "32,24").split(",")]
    out = Path(argv[2]) if len(argv) > 2 else Path("mascot-out")
    out.mkdir(parents=True, exist_ok=True)
    base = bust(source)
    for width in widths:
        small = shrink(base, width)
        (out / f"w{width}.ansi").write_text(ansi(small), encoding="utf-8")
        preview(small, max(4, 384 // width)).save(out / f"w{width}.png")
        print(f"w{width}: 终端 {width} 列 x {small.height // 2} 行 → {out}/w{width}.ansi")


if __name__ == "__main__":
    main(sys.argv[1:])
