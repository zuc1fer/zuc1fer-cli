"""Generate the packed edge-map binary for Ouroboros splash."""

import struct
from PIL import Image, ImageOps, ImageFilter

BASE_W = 400
BASE_H = int(BASE_W * 0.48)

img = Image.open(r"C:\Users\zinou\Downloads\ascii-art.png").convert("RGBA")
bg = Image.new("RGBA", img.size, (255, 255, 255, 255))
img = Image.alpha_composite(bg, img).convert("L")
img = ImageOps.autocontrast(img, cutoff=3)

# Sobel edge detection
k_h = [-1, 0, 1, -2, 0, 2, -1, 0, 1]
k_v = [-1, -2, -1, 0, 0, 0, 1, 2, 1]
e_h = img.filter(ImageFilter.Kernel((3, 3), k_h, scale=4))
e_v = img.filter(ImageFilter.Kernel((3, 3), k_v, scale=4))
edges = Image.blend(e_h, e_v, 0.5)
edges = edges.point(lambda p: min(255, int(p * 5)))
edges = edges.point(lambda p: 0 if p < 80 else p)

# Scale down to base resolution
tmp = edges.resize((BASE_W, BASE_H), Image.LANCZOS)
tmp = tmp.point(lambda p: 255 if p > 40 else 0)

pixels = list(tmp.getdata())
packed = bytearray()
for i in range(0, len(pixels), 8):
    byte = 0
    for j in range(8):
        if i + j < len(pixels) and pixels[i + j] > 0:
            byte |= 1 << (7 - j)
    packed.append(byte)

out_dir = r"C:\Users\zinou\Documents\Workspace\ophis-cli\crates\ophis-tui\src"
bin_path = f"{out_dir}\\ouroboros_edge.dat"
with open(bin_path, "wb") as f:
    f.write(struct.pack("<II", BASE_W, BASE_H))
    f.write(packed)

import os

print(f"Wrote {bin_path}")
print(f"  Size: {os.path.getsize(bin_path)} bytes")
print(f"  Resolution: {BASE_W}x{BASE_H}")

# Write a quick preview for verification
preview = []
for y in range(BASE_H):
    row = ""
    for x in range(BASE_W):
        p = pixels[y * BASE_W + x]
        row += "@" if p > 0 else " "
    preview.append(row)

with open(f"{out_dir}\\ouroboros_preview.txt", "w") as f:
    f.write("\n".join(preview))
print(f"  Preview written to ouroboros_preview.txt")
