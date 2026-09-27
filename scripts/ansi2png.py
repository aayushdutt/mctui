#!/usr/bin/env python3
"""Render ANSI (truecolor SGR) text to a PNG, terminal-style. Usage: ansi2png.py in.txt out.png"""
import re, sys, unicodedata
from PIL import Image, ImageDraw, ImageFont

import os
BG = (255, 255, 255) if os.environ.get("LIGHT") else (13, 13, 16)
if os.environ.get("BG"):  # e.g. BG=18181b: the background the frame's tints were derived from
    BG = tuple(int(os.environ["BG"][i:i + 2], 16) for i in (0, 2, 4))
FG = (30, 30, 35) if os.environ.get("LIGHT") else (220, 220, 225)
FONT_PATH = "/System/Library/Fonts/Menlo.ttc"
SIZE = 15
font = ImageFont.truetype(FONT_PATH, SIZE, index=0)
bold = ImageFont.truetype(FONT_PATH, SIZE, index=1)
italic = ImageFont.truetype(FONT_PATH, SIZE, index=2)
CW = round(font.getlength("M"))
CH = round(SIZE * 1.45)

ANSI16 = {30:(0,0,0),31:(205,49,49),32:(13,188,121),33:(229,229,16),34:(36,114,200),35:(188,63,188),36:(17,168,205),37:(229,229,229),
          90:(102,102,102),91:(241,76,76),92:(35,209,139),93:(245,245,67),94:(59,142,234),95:(214,112,214),96:(41,184,219),97:(255,255,255)}

def cells(ch):
    if unicodedata.east_asian_width(ch) in ("W", "F"):
        return 2
    return 1

def parse(text):
    rows = []
    for line in text.split("\n"):
        st = dict(fg=None, bg=None, b=False, f=False, i=False, u=False, r=False)
        row = []
        for tok in re.split(r"(\x1b\[[0-9;]*[A-Za-z])", line):
            m = re.fullmatch(r"\x1b\[([0-9;]*)([A-Za-z])", tok)
            if m:
                if m.group(2) != "m":
                    continue
                ps = [int(p) if p else 0 for p in m.group(1).split(";")] if m.group(1) else [0]
                k = 0
                while k < len(ps):
                    p = ps[k]
                    if p == 0: st = dict(fg=None, bg=None, b=False, f=False, i=False, u=False, r=False)
                    elif p == 1: st["b"] = True
                    elif p == 2: st["f"] = True
                    elif p == 3: st["i"] = True
                    elif p == 4: st["u"] = True
                    elif p == 7: st["r"] = True
                    elif p == 22: st["b"] = st["f"] = False
                    elif p == 23: st["i"] = False
                    elif p == 24: st["u"] = False
                    elif p == 27: st["r"] = False
                    elif p == 39: st["fg"] = None
                    elif p == 49: st["bg"] = None
                    elif p in (38, 48) and k + 1 < len(ps):
                        if ps[k+1] == 2 and k + 4 < len(ps):
                            col = tuple(ps[k+2:k+5]); k += 4
                        elif ps[k+1] == 5 and k + 2 < len(ps):
                            n = ps[k+2]; k += 2
                            col = (n, n, n)
                        else:
                            col = None
                        st["fg" if p == 38 else "bg"] = col
                    elif p in ANSI16: st["fg"] = ANSI16[p]
                    elif 40 <= p <= 47: st["bg"] = ANSI16[p - 10]
                    k += 1
                continue
            for ch in tok:
                row.append((ch, dict(st)))
        rows.append(row)
    return rows

def render(rows, out):
    width = max((sum(cells(c) for c, _ in r) for r in rows), default=1)
    pad = 16
    img = Image.new("RGB", (width * CW + 2 * pad, len(rows) * CH + 2 * pad), BG)
    d = ImageDraw.Draw(img)
    for y, row in enumerate(rows):
        x = 0
        for ch, st in row:
            w = cells(ch)
            fg = st["fg"] or FG
            bg = st["bg"]
            if st["r"]:
                fg, bg = (bg or BG), fg
            if st["f"]:
                fg = tuple(int(c * 0.6 + b * 0.4) for c, b in zip(fg, BG))
            px, py = pad + x * CW, pad + y * CH
            if bg:  # like terminals with line spacing: bg covers the text height only
                gap = (CH - round(SIZE * 1.25)) // 2
                d.rectangle([px, py + gap, px + w * CW - 1, py + CH - 1 - gap], fill=bg)
            if ch.strip():
                if ch in "─━":
                    cy = py + CH // 2
                    d.rectangle([px, cy, px + CW - 1, cy + (1 if ch == "━" else 0)], fill=fg)
                else:
                    f = bold if st["b"] else (italic if st["i"] else font)
                    d.text((px, py + 3), ch, font=f, fill=fg)
            if st["u"]:
                d.line([px, py + CH - 3, px + w * CW, py + CH - 3], fill=fg)
            x += w
    img.save(out)

if __name__ == "__main__":
    render(parse(open(sys.argv[1], encoding="utf-8").read()), sys.argv[2])
