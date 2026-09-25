#!/usr/bin/env python3
"""Generate test media using Pillow and ffmpeg:
1. test_image_basic.png: Multi-colored rectangles, badge text, clear OCR words.
2. test_image_chart.png: Multi-bar chart with precise value labels and titles.
3. test_video_clip.mp4: 3 distinct temporal stages with clear countdown and phase markers.
"""

import os
import shutil
import subprocess
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

BENCH_DIR = Path(__file__).resolve().parent
BENCH_DIR.mkdir(parents=True, exist_ok=True)

IMG_BASIC = BENCH_DIR / "test_image_basic.png"
IMG_CHART = BENCH_DIR / "test_image_chart.png"
VID_CLIP = BENCH_DIR / "test_video_clip.mp4"

def get_font(size=24):
    try:
        # macOS default fonts
        for font_path in [
            "/System/Library/Fonts/Helvetica.ttc",
            "/System/Library/Fonts/SFNSText.ttf",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/System/Library/Fonts/Supplemental/Courier New.ttf",
        ]:
            if os.path.exists(font_path):
                return ImageFont.truetype(font_path, size)
    except Exception:
        pass
    return ImageFont.load_default()

def generate_basic_image():
    w, h = 640, 480
    im = Image.new("RGB", (w, h), color=(20, 30, 60))
    draw = ImageDraw.Draw(im)

    font_title = get_font(28)
    font_body = get_font(20)

    # Title banner
    draw.rectangle([20, 20, 620, 70], fill=(40, 60, 110), outline=(255, 255, 255), width=2)
    draw.text((60, 32), "GQY MULTIMODAL BENCHMARK", fill=(255, 255, 255), font=font_title)

    # Box 1: Gold Box
    draw.rectangle([50, 100, 280, 240], fill=(218, 165, 32), outline=(255, 215, 0), width=3)
    draw.text((65, 120), "Module: ALPHA", fill=(0, 0, 0), font=font_body)
    draw.text((65, 160), "Key: 9081-PASS", fill=(0, 0, 0), font=font_body)
    draw.text((65, 200), "Count: 42 units", fill=(0, 0, 0), font=font_body)

    # Box 2: Crimson Box
    draw.rectangle([340, 100, 580, 240], fill=(178, 34, 34), outline=(255, 99, 71), width=3)
    draw.text((355, 120), "Module: BETA", fill=(255, 255, 255), font=font_body)
    draw.text((355, 160), "Target: ZERO-ERR", fill=(255, 255, 255), font=font_body)
    draw.text((355, 200), "Score: 99.8%", fill=(255, 255, 255), font=font_body)

    # Bottom status card
    draw.rectangle([50, 280, 580, 440], fill=(30, 45, 80), outline=(100, 149, 237), width=2)
    draw.text((70, 300), "System Diagnostics Summary:", fill=(173, 216, 230), font=font_body)
    draw.text((70, 340), "- Tool Registry: Verified 46 modules", fill=(255, 255, 255), font=font_body)
    draw.text((70, 380), "- Memory Index: 100% Synced", fill=(255, 255, 255), font=font_body)

    im.save(IMG_BASIC)
    print(f"[MediaGen] Generated basic test image: {IMG_BASIC}")

def generate_chart_image():
    w, h = 800, 600
    im = Image.new("RGB", (w, h), color=(250, 250, 252))
    draw = ImageDraw.Draw(im)

    font_title = get_font(30)
    font_label = get_font(18)
    font_bold = get_font(22)

    # Title
    draw.text((160, 30), "Agent Capability Metrics (Q3 2026)", fill=(20, 20, 40), font=font_title)

    # Baseline axes
    draw.line([(80, 480), (720, 480)], fill=(120, 120, 120), width=2)
    draw.line([(80, 100), (80, 480)], fill=(120, 120, 120), width=2)

    bars = [
        ("Vision QA", 94.5, (65, 105, 225), 140),
        ("Tool Chain", 98.2, (46, 139, 87), 280),
        ("Long Memory", 96.8, (218, 112, 214), 420),
        ("Gate Dispatch", 100.0, (255, 140, 0), 560),
    ]

    for label, val, color, x in bars:
        bar_height = int((val / 100.0) * 320)
        top_y = 480 - bar_height
        draw.rectangle([x, top_y, x + 80, 480], fill=color, outline=(40, 40, 40), width=1)
        draw.text((x + 10, top_y - 30), f"{val}%", fill=(20, 20, 20), font=font_bold)
        draw.text((x - 5, 495), label, fill=(50, 50, 50), font=font_label)

    # Benchmark summary box
    draw.rectangle([100, 100, 380, 150], fill=(230, 240, 255), outline=(100, 149, 237), width=1)
    draw.text((110, 115), "Peak Category: Gate Dispatch (100%)", fill=(0, 50, 150), font=font_label)

    im.save(IMG_CHART)
    print(f"[MediaGen] Generated chart test image: {IMG_CHART}")

def generate_video_clip():
    frames_dir = BENCH_DIR / "temp_frames"
    if frames_dir.exists():
        shutil.rmtree(frames_dir)
    frames_dir.mkdir(parents=True, exist_ok=True)

    font_title = get_font(40)
    font_sub = get_font(28)

    stages = [
        {"bg": (220, 50, 50), "text": "PHASE 1: INITIALIZE", "code": "SIG-ALPHA-RED", "frames": 24},     # ~1 sec at 24fps
        {"bg": (34, 139, 34), "text": "PHASE 2: PROCESSING", "code": "SIG-BETA-GREEN", "frames": 24},
        {"bg": (128, 0, 128), "text": "PHASE 3: COMPLETE", "code": "SIG-GAMMA-PURPLE", "frames": 24},
    ]

    frame_idx = 0
    for s in stages:
        for f in range(s["frames"]):
            im = Image.new("RGB", (640, 360), color=s["bg"])
            draw = ImageDraw.Draw(im)
            draw.text((80, 120), s["text"], fill=(255, 255, 255), font=font_title)
            draw.text((140, 190), f"Token: {s['code']}", fill=(255, 255, 200), font=font_sub)
            draw.text((220, 260), f"Frame: {f+1}/{s['frames']}", fill=(230, 230, 230), font=font_sub)
            frame_path = frames_dir / f"frame_{frame_idx:04d}.png"
            im.save(frame_path)
            frame_idx += 1

    cmd = [
        "ffmpeg", "-y",
        "-framerate", "24",
        "-i", str(frames_dir / "frame_%04d.png"),
        "-c:v", "libx264",
        "-pix_fmt", "yuv420p",
        "-movflags", "+faststart",
        str(VID_CLIP)
    ]
    subprocess.run(cmd, check=True, capture_output=True)
    shutil.rmtree(frames_dir)
    print(f"[MediaGen] Generated temporal video clip: {VID_CLIP} ({frame_idx} frames, 3 phases)")

if __name__ == "__main__":
    generate_basic_image()
    generate_chart_image()
    generate_video_clip()
    print("[MediaGen] All test assets generated successfully.")
