#!/usr/bin/env python3
"""Render docs/demo.gif from real fusebox output in a synthetic HOME.

Usage: docs/demo/make-demo-home.sh DIR && python3 docs/demo/render-gif.py DIR
Needs Google Chrome (headless screenshots) and ffmpeg.
"""
import html, os, subprocess, sys, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HOME = Path(sys.argv[1]).resolve()
BIN = ROOT / "target/release/fusebox"
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
STEPS = [
    ["status"],
    ["find", "merge two PDFs", "--limit", "2"],
    ["find", "截个网页的图", "--limit", "1"],
    ["secret", "list"],
]

def run(args):
    out = subprocess.run([str(BIN), *args], env={**os.environ, "HOME": str(HOME)},
                         capture_output=True, text=True, check=True).stdout
    return out.replace(str(HOME), "~").rstrip("\n")

def shell_quote(a):
    return f'"{a}"' if " " in a or not a.isascii() else a

CSS = """body{margin:0;background:#0f1115;font:15px/1.5 'SF Mono',Menlo,monospace;color:#d7dae0}
.win{margin:18px;border-radius:12px;background:#171a21;box-shadow:0 10px 30px #0008;overflow:hidden;height:calc(100vh - 36px)}
.bar{height:34px;background:#1f232c;display:flex;align-items:center;padding:0 14px;gap:8px}
.dot{width:12px;height:12px;border-radius:50%}.t{color:#8b93a1;font-size:13px;margin-left:10px}
pre{margin:0;padding:16px 20px;white-space:pre-wrap}.p{color:#7ee787}.c{color:#ffffff}.k{color:#79c0ff}
.dim{color:#8b93a1}.cur{background:#d7dae0;color:#171a21}"""

def color(line):
    e = html.escape(line)
    if line.startswith("[skill]") or line.startswith("[mcp]"):
        tag, rest = e.split("]", 1)
        return f'<span class="k">{tag}]</span><span class="c">{rest}</span>'
    if line.lstrip().startswith("→"):
        return f'<span class="dim">{e}</span>'
    return e

def page(lines):
    body = "\n".join(lines)
    return (f"<html><head><meta charset='utf-8'><style>{CSS}</style></head><body><div class='win'><div class='bar'>"
            "<div class='dot' style='background:#ff5f57'></div><div class='dot' style='background:#febc2e'></div>"
            f"<div class='dot' style='background:#28c840'></div><span class='t'>fusebox</span></div><pre>{body}</pre></div></body></html>")

frames = []  # (lines, seconds)
screen = []
for args in STEPS:
    cmd = "fusebox " + " ".join(shell_quote(a) for a in args)
    for i in range(0, len(cmd) + 1, 3):
        frames.append((screen + [f'<span class="p">$</span> {html.escape(cmd[:i])}<span class="cur"> </span>'], 0.06))
    screen = screen + [f'<span class="p">$</span> {html.escape(cmd)}'] + [color(l) for l in run(args).splitlines()]
    frames.append((screen + [f'<span class="p">$</span> <span class="cur"> </span>'], 2.4))
    screen = screen[-14:]

with tempfile.TemporaryDirectory() as tmp:
    tmp = Path(tmp)
    listing = []
    for n, (lines, secs) in enumerate(frames):
        h, png = tmp / f"{n:03}.html", tmp / f"{n:03}.png"
        h.write_text(page(lines), encoding="utf-8")
        subprocess.run([CHROME, "--headless=new", "--disable-gpu", "--hide-scrollbars", f"--screenshot={png}",
                        "--window-size=860,560", "--force-device-scale-factor=1.5", h.as_uri()],
                       check=True, capture_output=True)
        listing.append(f"file '{png}'\nduration {secs}")
    listing.append(f"file '{png}'")
    (tmp / "list.txt").write_text("\n".join(listing))
    out = ROOT / "docs/demo.gif"
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", str(tmp / "list.txt"),
                    "-vf", "scale=860:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=96[p];[b][p]paletteuse=dither=none",
                    "-loop", "0", str(out)], check=True)
    print(out, out.stat().st_size // 1024, "KB,", len(frames), "frames")
