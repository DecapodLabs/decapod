#!/usr/bin/env python3
"""Decapod's six-second brand film: deterministic motion + original synthesized music.

Source scenes communicate released concepts; this is a motion-graphics product film,
not fabricated terminal execution or a claim of successful hosted acceptance.
"""
from __future__ import annotations
import argparse
import json
import math
import os
from pathlib import Path
from io import BytesIO
import cairosvg
import shutil
import subprocess
import tempfile
import wave

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent
OUT = ROOT / "generated"
WIDTH, HEIGHT, FPS, FRAMES = 1920, 1080, 30, 198
DURATION = FRAMES / FPS
WHITE = (255, 255, 255)
INK = (20, 23, 25)
CORAL = (231, 85, 57)
CORAL_LIGHT = (248, 138, 103)
CORAL_SHADE = (185, 62, 45)
GRAY = (96, 103, 106)

# Fixed typography: install Ubuntu's fonts-lato package for identical renders.
FONT_DIR = Path(os.environ.get("DECAPOD_SHOWCASE_FONTS", "/usr/share/fonts/truetype/lato"))
FONT_HEAVY = FONT_DIR / "Lato-Heavy.ttf"
FONT_REG = FONT_DIR / "Lato-Regular.ttf"
FONT_MED = FONT_DIR / "Lato-Medium.ttf"
for required in (FONT_HEAVY, FONT_REG, FONT_MED):
    if not required.is_file():
        raise SystemExit(f"Required font missing: {required}; install fonts-lato")

# Human problem first. Then the agent-native answer. Explain the kernel before the brand.
# Seven scenes, seven notes, a single clean statement per screen.
SCENES = [
    {"start": 0,   "end": 36,  "lines": ("TIRED OF", "BABYSITTING", "YOUR AGENTS?"), "accent": "BABYSITTING", "align": "left", "crab": (1530, 760)},
    {"start": 36,  "end": 79,  "lines": ("GIVE IT A", "METACOGNITIVE", "GOVERNANCE KERNEL."), "accent": "METACOGNITIVE", "align": "right", "crab": (370, 760)},
    {"start": 79,  "end": 101, "lines": ("PERSISTENT", "STATE."), "accent": "PERSISTENT", "align": "left", "crab": (1530, 760)},
    {"start": 101, "end": 123, "lines": ("ENFORCED", "BOUNDARIES."), "accent": "ENFORCED", "align": "right", "crab": (370, 760)},
    {"start": 123, "end": 145, "lines": ("COMMIT-BOUND", "PROOF."), "accent": "PROOF.", "align": "left", "crab": (1530, 760)},
    {"start": 145, "end": 176, "lines": ("INIT ONCE", "PER PROJECT.", "KEEP USING", "YOUR AGENTS."), "accent": "KEEP USING", "align": "right", "crab": (370, 760)},
    {"start": 176, "end": 198, "kind": "outro", "crab": (1530, 760)},
]


def ease(p: float) -> float:
    p = max(0.0, min(1.0, p))
    return 1 - (1 - p) ** 3


def smooth(p: float) -> float:
    p = max(0.0, min(1.0, p))
    return p * p * (3 - 2 * p)


def font(file: Path, size: int) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(str(file), size)


def tracking(draw: ImageDraw.ImageDraw, x: float, y: float, message: str, fnt: ImageFont.FreeTypeFont, fill, spacing=4):
    xx = x
    for ch in message:
        draw.text((xx, y + fnt.getmetrics()[0]), ch, font=fnt, fill=fill, anchor="ls")
        xx += draw.textlength(ch, font=fnt) + spacing


_MASCOT = Image.open(BytesIO(cairosvg.svg2png(
    url=str(ROOT / "mascot.svg"), output_width=1040, output_height=880
))).convert("RGBA")


def crab_sprite(frame: int, scale: float, tilt: float = 0) -> Image.Image:
    # Render the original vector asset once. Affine transforms are deterministic.
    bob = math.sin((frame / FPS) * 10.8)
    rolled = _MASCOT.rotate(tilt + 1.5 * bob, Image.Resampling.BICUBIC, expand=True)
    target = (max(1, int(rolled.width * scale)), max(1, int(rolled.height * scale)))
    return rolled.resize(target, Image.Resampling.LANCZOS)


def scene_for_frame(frame: int):
    return next(s for s in SCENES if s["start"] <= frame < s["end"])


def crab_position(frame: int, scene: dict):
    # Character blocking is independent from typography: clean entry/exit lanes.
    start = scene["start"]
    base_x, base_y = scene["crab"]
    age = frame - start
    if start == 0 and age < 22:
        # First scene: clatter in from the right, overshoot and settle.
        p = min(1.0, age / 21.0)
        x = base_x + 600 * (1-p)**2 - 38 * math.sin(2.7 * math.pi * p) * (1-p)
        y = base_y - 100 * max(0.0, math.sin(2.0 * math.pi*p))
        tilt = 12 * (1-p) * math.sin(4.5 * math.pi * p)
        return x, y, tilt, 1.0 + 0.08 * math.sin(2*math.pi*p)
    if scene.get("kind") == "outro" and age >= 14:
        # The URL appears before the crab exits along its nearest (right) edge.
        p = smooth((age - 14) / 7.0)
        return base_x + 630*p, base_y - 30*math.sin(math.pi*p), 14*p, 1.0
    if age < 11 and start != 0:
        p = smooth(age / 10.0)
        return (base_x, base_y - 82 * math.sin(math.pi * p),
                (1-p) * 7.0 * math.sin(math.pi*p),
                1.0 + 0.08 * math.sin(2*math.pi*p))
    tt = (age-11 if start else age-22) / FPS
    y = base_y - 9 * max(0.0, math.sin(2*math.pi*1.6*tt))
    return base_x, y, 2.2*math.sin(2*math.pi*1.4*tt), 1.0


def put_crab(im: Image.Image, scene: dict, frame: int):
    x,y,rot,sc = crab_position(frame, scene)
    # Floor-contact shadow follows the mascot through entry and exit.
    shadow = Image.new("RGBA", (480, 110), (0,0,0,0))
    sd = ImageDraw.Draw(shadow)
    sd.ellipse((60, 36, 420, 81), fill=(14, 17, 20, 35))
    shadow = shadow.filter(ImageFilter.GaussianBlur(25))
    im.paste(shadow, (int(x-240), 846), shadow)
    sprite = crab_sprite(frame, 0.43*sc if scene.get("kind") != "outro" else 0.39*sc, rot)
    im.paste(sprite, (int(x-sprite.width/2), int(y-sprite.height/2)), sprite)


def text_fade(frame: int, scene: dict) -> tuple[float,int]:
    age = frame - scene["start"]
    remaining = scene["end"] - frame
    enter = ease(age/5.0)
    exit = smooth(remaining/3.5)
    return min(enter,exit), int((1-enter)*59 - (1-exit)*34)


def layer_text(im: Image.Image, scene: dict, frame: int):
    a, yshift = text_fade(frame,scene)
    if a < 0.005: return
    layer = Image.new("RGBA", (WIDTH, HEIGHT), (0,0,0,0))
    draw = ImageDraw.Draw(layer)
    opacity = int(255*a)
    ink = (*INK, opacity)
    gray = (*GRAY, opacity)
    if scene.get("kind") in ("brand", "outro"):
        is_outro = scene["kind"] == "outro"
        title = "DECAPOD"
        f = font(FONT_HEAVY, 205 if is_outro else 228)
        box = draw.textbbox((0,0),title,font=f,anchor="lt")
        tw = box[2]-box[0]
        y = (342 if is_outro else 358) + yshift
        draw.text(((WIDTH-tw)/2,y),title,font=f,fill=(*CORAL, opacity) if is_outro else ink,anchor="lt")
        cap = "BUILT FOR AI AGENTS." if is_outro else "A GOVERNANCE KERNEL FOR AI AGENTS"
        ff = font(FONT_MED, 39)
        width = sum(draw.textlength(ch,font=ff)+5 for ch in cap)
        tracking(draw,(WIDTH-width)/2,608+yshift,cap,ff,(*GRAY,opacity),5)
        if is_outro:
            cmd = "github.com/DecapodLabs/decapod"
            small=font(FONT_REG,38)
            cbox = draw.textbbox((0,0),cmd,font=small,anchor="lt")
            draw.text(((WIDTH-(cbox[2]-cbox[0]))/2,792+yshift),cmd,font=small,fill=(*GRAY,opacity),anchor="lt")
    else:
        lines=scene["lines"]
        side=scene["align"]
        # One idea per cut, sized to a strict editorial column with generous margins.
        xs = 172 if side == "left" else 768
        max_width = 1130 if side == "left" else 990
        size = 185
        while size > 85:
            candidate = font(FONT_HEAVY, size)
            if all(draw.textlength(line, font=candidate) <= max_width for line in lines):
                break
            size -= 2
        f = font(FONT_HEAVY, size)
        gap = int(size * 1.12)
        first = int(500 - (len(lines)-1)*gap/2 - size/2)
        for i,line in enumerate(lines):
            fill = (*CORAL, opacity) if line == scene.get("accent") else ink
            draw.text((xs, first+i*gap+yshift), line, font=f, fill=fill, anchor="lt")
        # The mascot animates separately and every screen remains self-contained.
    im.alpha_composite(layer)


def frame_image(frame: int) -> Image.Image:
    s=scene_for_frame(frame)
    im=Image.new("RGBA",(WIDTH,HEIGHT),WHITE+(255,))
    layer_text(im,s,frame)
    put_crab(im,s,frame)
    return im.convert("RGB")


def synth_audio(path: Path):
    """Original seven-note synth theme, written procedurally; no licensed samples."""
    sr=44100
    n=round(DURATION*sr)
    mix=np.zeros((n,2),dtype=np.float64)
    def add(signal,at,pan=0):
        start=int(at*sr)
        if start>=n or start+len(signal)<=0:return
        lo=max(0,start); hi=min(n,start+len(signal))
        segment=signal[lo-start:hi-start]
        mix[lo:hi,0]+=segment*(.72-.30*pan)
        mix[lo:hi,1]+=segment*(.72+.30*pan)
    def pluck(freq,start,dur=.62,vol=.16,pan=0):
        t=np.arange(int(dur*sr))/sr
        env=(1-np.exp(-t*270))*np.exp(-t*6.6)*(1-np.exp(-(dur-t)*38))
        s=(np.sin(2*np.pi*freq*t)+.30*np.sin(2*np.pi*freq*2.005*t)+.12*np.sin(2*np.pi*freq*3.01*t))*env*vol
        add(s,start,pan)
        add(s*.20,start+.135,-pan)
    def pad(root,start,dur):
        t=np.arange(int(dur*sr))/sr
        env=np.minimum(1,t*9)*np.minimum(1,(dur-t)*6)
        signal=(np.sin(2*np.pi*root*t)+.35*np.sin(2*np.pi*(root*1.5)*t)+.24*np.sin(2*np.pi*(root*2.0)*t))*env*.036
        add(signal,start,-.3)
    def kick(start,vol=.13):
        t=np.arange(int(.25*sr))/sr
        phase=2*np.pi*(72*t+86*(1-np.exp(-t*32))/32)
        add(vol*np.sin(phase)*np.exp(-t*20),start)
    def tick(start,vol=.025):
        rng=np.random.default_rng(9821+int(start*1000))
        noise=rng.standard_normal(int(.055*sr))
        env=np.exp(-np.arange(len(noise))/sr*85)
        add(noise*env*vol,start,.6)
    # Seven scene arrivals, seven synchronized musical accents.
    cue=[s["start"]/FPS for s in SCENES]
    melody=[659.25,783.99,987.77,880.00,1174.66,987.77,1318.51]
    for i,(at,freq) in enumerate(zip(cue,melody)):
        pluck(freq,at+.06,vol=.145 if i<6 else .20,pan=(-.24 if i%2 else .26))
        pluck(freq/2,at+.062,vol=.082,pan=.0)
        tick(at+.03,.020)
    # Soft original harmonic underpinning and tightly controlled kick.
    for start,root,dur in [(0,164.81,1.7),(1.7,130.81,1.8),(3.5,146.83,1.8),(5.3,164.81,1.3)]:
        pad(root,start,min(dur,DURATION-start))
    for i in range(14):
        at=.08+i*.46
        if at<DURATION: kick(at,.080 if i%2 else .13)
        if i%2 and at+.23<DURATION: tick(at+.23,.022)
    # Fade exact end and remove DC.
    mix-=mix.mean(axis=0)
    fadein=np.minimum(1,np.arange(n)/(sr*.04))
    fadeout=np.minimum(1,(n-1-np.arange(n))/(sr*.14))
    mix*=np.minimum(fadein,fadeout)[:,None]
    peak=max(1e-12,float(np.max(np.abs(mix))))
    mix*=.88/peak
    pcm=(mix*32767).clip(-32768,32767).astype("<i2")
    with wave.open(str(path),"wb") as wav:
        wav.setnchannels(2);wav.setsampwidth(2);wav.setframerate(sr);wav.writeframes(pcm.tobytes())


def run():
    parser=argparse.ArgumentParser()
    parser.add_argument("--preview",action="store_true",help="render only poster/storyboard frames")
    args=parser.parse_args()
    OUT.mkdir(parents=True,exist_ok=True)
    poster=frame_image(181)
    poster.save(OUT/"decapod-showcase-poster.png",optimize=True)
    if args.preview:
        thumbs=[]
        for scene in SCENES:
            idx=min(scene["end"]-5,scene["start"]+12)
            im=frame_image(idx)
            im.thumbnail((640,360))
            thumbs.append(im)
        sheet=Image.new("RGB",(640*2,360*4),WHITE)
        for i,im in enumerate(thumbs):sheet.paste(im,((i%2)*640,(i//2)*360))
        sheet.save(OUT/"storyboard.png")
        return
    if not shutil.which("ffmpeg") or not shutil.which("ffprobe"):
        raise SystemExit("ffmpeg and ffprobe are required")
    with tempfile.TemporaryDirectory(prefix="decapod-showcase-") as tmp:
        tmp=Path(tmp)
        v=tmp/"silent.mp4"; a=tmp/"score.wav"
        synth_audio(a)
        cmd=["ffmpeg","-hide_banner","-loglevel","error","-y","-f","rawvideo","-pix_fmt","rgb24","-s",f"{WIDTH}x{HEIGHT}","-r",str(FPS),"-i","-","-an","-c:v","libx264","-preset","medium","-crf","17","-pix_fmt","yuv420p","-movflags","+faststart",str(v)]
        proc=subprocess.Popen(cmd,stdin=subprocess.PIPE)
        try:
            for i in range(FRAMES):
                im=frame_image(i)
                proc.stdin.write(im.tobytes())
        finally:
            proc.stdin.close()
        if proc.wait()!=0:raise RuntimeError("ffmpeg video encoding failed")
        outfile=OUT/"decapod-showcase.mp4"
        subprocess.run(["ffmpeg","-hide_banner","-loglevel","error","-y","-i",str(v),"-i",str(a),"-c:v","copy","-c:a","aac","-b:a","192k","-t",f"{DURATION:.4f}","-movflags","+faststart",str(outfile)],check=True)
        # Generate GIF from the same encoded video used for the MP4.
        # Pillow 11.3.0 collapsed shared-palette frames to one still on Ubuntu,
        # preventing artifacts from reaching the branch. FFmpeg retains motion.
        subprocess.run([
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
            "-i", str(v), "-an",
            "-vf", "fps=15,scale=960:-1:flags=lanczos",
            "-loop", "0", str(OUT/"decapod-showcase.gif")
        ], check=True)
    data=json.loads(subprocess.check_output(["ffprobe","-v","error","-show_entries","format=duration:stream=codec_type,codec_name","-of","json",str(outfile)],text=True))
    duration=float(data["format"]["duration"])
    assert 6.0 <= duration < 7.0,duration
    assert {s["codec_type"] for s in data["streams"]}=={"audio","video"},data
    print(f"Rendered {FRAMES} frames, {duration:.3f}s with original stereo score")
    print(f"Assets in {OUT}")

if __name__=="__main__":run()
