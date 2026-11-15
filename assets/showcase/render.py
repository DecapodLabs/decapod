#!/usr/bin/env python3
"""Render Decapod's six-second feature reel from a fixed, reviewable timeline.

Every caption describes an implemented product capability; the terminal-style
cards are explanatory diagrams, not captured commands or test results.
Requires Pillow and FFmpeg. Nothing uses network services or project secrets.
"""

import argparse
import math
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageFilter

W, H = 1280, 720
FPS = 24
FRAMES = 158                        # 6.5833 seconds, never 7 or more
SECTIONS = (0.0, 0.68, 1.64, 2.65, 3.65, 4.71, 5.71, FRAMES / FPS)
NAV = ("START", "CLAIM", "BOUND", "REFLECT", "PROVE", "RECOVER", "SHIP")
DARK = (7, 12, 22)
WHITE = (238, 249, 253)
MUTED = (140, 166, 180)
CYAN = (105, 233, 223)
ORANGE = (255, 155, 116)
GREEN = (156, 241, 177)
FONT_DIR = Path("/usr/share/fonts/truetype/dejavu")


def font(size, bold=False, mono=False):
    name = "DejaVuSansMono" if mono else "DejaVuSans"
    if bold:
        name += "-Bold"
    path = FONT_DIR / f"{name}.ttf"
    return ImageFont.truetype(str(path), size)


FLOGO = font(26, bold=True)
FBIG = font(71, bold=True)
FOUT = font(80, bold=True)
FSUB = font(28)
FMONO = font(23, mono=True)
FSMALL = font(18, bold=True, mono=True)
FCHIP = font(16, mono=True)

SCENES = (
    ("DECAPOD", "Metacognition for coding agents", "LOCAL-FIRST   /   DAEMONLESS   /   RUST"),
    ("CLAIM & ISOLATE", "Concurrent agents. Explicit ownership.", "task.claim      →  scoped custody"),
    ("BOUND AUTHORITY", "Permissions enforced at work boundaries.", "policy.check    →  scope admitted"),
    ("REFLECT & RESOLVE", "Evidence before unnecessary escalation.", "uncertainty     →  investigate → decide"),
    ("PROVE THE COMMIT", "Validation bound to exact source identity.", "commit SHA      →  checks + evidence"),
    ("RECOVER & HANDOFF", "Work history survives interrupted sessions.", "task history    →  preserve + resume"),
    ("GOVERN THE WORK", "Decapod is built with Decapod.", "cargo binstall decapod && decapod init"),
)


def smooth(x):
    x = min(1.0, max(0.0, x))
    return x * x * (3 - 2 * x)


def background():
    image = Image.new("RGB", (W, H), DARK)
    draw = ImageDraw.Draw(image, "RGB")
    # Static light fields keep the encode small and the animated text readable.
    for cx, cy, radius, col in ((1040, 220, 520, CYAN), (135, 575, 365, ORANGE)):
        for step in range(radius, 40, -8):
            alpha = (radius - step) / radius
            light = tuple(round(DARK[i] + (col[i] - DARK[i]) * (0.034 * alpha)) for i in range(3))
            draw.ellipse((cx-step, cy-step, cx+step, cy+step), fill=light)
    for x in range(72, W, 64):
        for y in range(110, 620, 64):
            draw.point((x, y), fill=(20, 37, 48))
    return image


BG = background()


def pill(draw, xy, text, fill, fg, xpad=17):
    tw = draw.textbbox((0, 0), text, font=FCHIP)[2]
    draw.rounded_rectangle((xy[0], xy[1], xy[0]+tw+2*xpad, xy[1]+35), radius=17, fill=fill)
    draw.text((xy[0]+xpad, xy[1]+7), text, font=FCHIP, fill=fg)


def crab(draw, x, y, scale=1.0):
    # Original vector silhouette; intentionally distinct from terminal typography.
    c = ORANGE
    def xx(a): return int(x+a*scale)
    def yy(a): return int(y+a*scale)
    for side in (-1, 1):
        for a, b in ((-25, -8), (-31, 6), (-25, 18)):
            draw.line((xx(side*19), yy(0), xx(side*32), yy(b), xx(side*44), yy(b+side*2)), fill=c, width=max(2,int(5*scale)), joint="curve")
        draw.line((xx(side*22), yy(-9), xx(side*42), yy(-26)), fill=c, width=max(2,int(7*scale)))
        draw.ellipse((xx(side*45-10), yy(-38), xx(side*45+10), yy(-17)), fill=c)
        draw.pieslice((xx(side*45-11),yy(-40),xx(side*45+12),yy(-17)), 270 if side>0 else 90, 45 if side>0 else 225, fill=DARK)
    draw.ellipse((xx(-32),yy(-23),xx(32), yy(22)), fill=c)
    for eye in (-13,13):
        draw.line((xx(eye), yy(-20), xx(eye), yy(-32)), fill=c, width=max(2,int(5*scale)))
        draw.ellipse((xx(eye-7), yy(-40), xx(eye+7), yy(-26)), fill=WHITE)
        draw.ellipse((xx(eye-2), yy(-37), xx(eye+3), yy(-31)), fill=DARK)
    draw.arc((xx(-12), yy(-4), xx(12), yy(14)), 10, 170, fill=DARK, width=max(2,int(3*scale)))


def section_at(t):
    for j in range(len(SCENES)):
        if SECTIONS[j] <= t < SECTIONS[j+1]:
            return j
    return len(SCENES)-1


def scene_layer(index, t):
    layer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer, "RGBA")
    title, subtitle, command = SCENES[index]
    local = t - SECTIONS[index]
    enter = smooth(local / 0.16)
    exit_progress = smooth((SECTIONS[index+1] - t) / 0.12)
    opacity = min(enter, exit_progress)
    shift = round((1-enter) * 42)
    x = 148 + shift
    if index in (0, 6):
        big = FOUT
        d.text((x, 241), title, font=big, fill=WHITE)
        d.rounded_rectangle((x+2, 345, x+333, 353), radius=3, fill=CYAN)
        d.text((x+2, 382), subtitle, font=FSUB, fill=WHITE)
        pill(d, (x+2, 457), command, (22, 48, 57), CYAN)
    else:
        d.text((x, 243), title, font=FBIG, fill=WHITE)
        d.text((x+2, 360), subtitle, font=FSUB, fill=WHITE)
        d.rounded_rectangle((x, 432, 1119, 503), radius=12, fill=(16, 35, 46), outline=(41, 83, 89), width=2)
        d.text((x+22, 450), "$", font=FMONO, fill=GREEN)
        d.text((x+66, 450), command, font=FMONO, fill=CYAN)
    if opacity < 1:
        layer.putalpha(layer.getchannel("A").point(lambda v: round(v*opacity)))
    return layer


def frame(idx):
    t = idx / FPS
    scene = section_at(t)
    canvas = BG.copy().convert("RGBA")
    d = ImageDraw.Draw(canvas, "RGBA")

    # Header is persistent while the content moves under it.
    d.rounded_rectangle((81, 67, 124, 110), radius=13, fill=(19, 51, 59), outline=CYAN, width=2)
    crab(d, 102, 91, .24)
    d.text((141, 74), "DECAPOD", font=FLOGO, fill=WHITE)
    d.text((1043, 80), "0.148 / SHOWCASE", font=FCHIP, fill=MUTED)

    d.rounded_rectangle((96, 159, 1184, 538), radius=24, fill=(9, 19, 31), outline=(45, 71, 83), width=2)
    for n, col in enumerate((ORANGE, (247, 211, 116), GREEN)):
        d.ellipse((127+n*21, 188, 137+n*21, 198), fill=col)
    d.text((941, 185), "GOVERNANCE / CLI", font=FCHIP, fill=(116, 157, 166))
    d.line((120, 221, 1160, 221), fill=(38, 61, 73), width=2)

    # Two overlapping layers make cuts crisp without a black frame.
    canvas.alpha_composite(scene_layer(scene, t))
    if (SECTIONS[scene+1] - t) < 0.12 and scene < len(SCENES)-1:
        canvas.alpha_composite(scene_layer(scene+1, t + .005))

    d = ImageDraw.Draw(canvas, "RGBA")
    nodes = (152, 316, 481, 646, 811, 975, 1137)
    y = 604
    d.line((nodes[0], y, nodes[-1], y), fill=(39, 63, 78), width=4)
    target = nodes[scene]
    start = nodes[max(0,scene-1)]
    local = t - SECTIONS[scene]
    progress = smooth(local/.32)
    cx = start + (target-start)*progress
    d.line((nodes[0], y, int(cx), y), fill=CYAN, width=4)
    for n, x in enumerate(nodes):
        active = n <= scene
        d.ellipse((x-6,y-6,x+6,y+6),fill=CYAN if active else (64,91,106))
        label = NAV[n]
        width = d.textbbox((0,0),label,font=FCHIP)[2]
        d.text((x-width/2, 635), label, font=FCHIP,fill=WHITE if n==scene else MUTED)
    hop = math.sin(min(1,local/.38)*math.pi)*15
    crab(d, int(cx), int(571-hop), .44)
    d.text((98, 689), "OPEN SOURCE  ·  RUST  ·  LOCAL-FIRST", font=FCHIP, fill=MUTED)
    return canvas.convert("RGB")


def run(args):
    out = Path(args.output)
    out.mkdir(parents=True, exist_ok=True)
    mp4 = out / "decapod-showcase.mp4"
    cmd = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-", "-an", "-c:v", "libx264", "-preset", "medium", "-crf", "23", "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(mp4)]
    process = subprocess.Popen(cmd, stdin=subprocess.PIPE)
    try:
        for idx in range(FRAMES):
            process.stdin.write(frame(idx).tobytes())
    finally:
        process.stdin.close()
    if process.wait() != 0:
        raise SystemExit("ffmpeg failed to render the video")

    frame(round(FPS*4.1)).save(out / "decapod-showcase-poster.png", optimize=True)
    # Optimized 12 fps GIF for Github README embeds. All original frames remain in MP4.
    gif = out / "decapod-showcase.gif"
    filtergraph = "fps=12,scale=760:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=56:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4"
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", str(mp4), "-filter_complex", filtergraph, "-loop", "0", str(gif)], check=True)
    probe = subprocess.check_output(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1", str(mp4)], text=True)
    duration = float(probe.strip())
    if not 6.0 <= duration < 7.0:
        raise SystemExit(f"showcase violates duration gate: {duration:.4f}s")
    print(f"Decapod showcase: {duration:.3f}s / {FRAMES} frames / {FPS} fps")
    for f in (mp4, gif, out / "decapod-showcase-poster.png"):
        print(f"  {f} ({f.stat().st_size:,} bytes)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", default="assets/showcase/generated")
    run(parser.parse_args())
