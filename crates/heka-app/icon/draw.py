import io
import pathlib
import struct

from PIL import Image, ImageChops, ImageDraw

GRID = 16
SMOOTH = 16
BLUE = (0x2F, 0x6F, 0xE0, 0xFF)
WHITE = (0xFF, 0xFF, 0xFF, 0xFF)
SIZES = [16, 20, 24, 28, 30, 32, 36, 40, 42, 48, 56, 60, 64, 72, 96, 128, 256]
PICTURE = 96
LEFT_HALF = [
    [(8, 4), (7, 4), (6, 4), (5, 4)],
    [(5, 4), (3, 4), (2.3, 5.3), (2, 7)],
    [(2, 7), (1.65, 8.9), (1.4, 10.3), (1.55, 11.2)],
    [(1.55, 11.2), (1.8, 12.9), (3.5, 13.2), (4.5, 12.1)],
    [(4.5, 12.1), (5.3, 11.2), (5.6, 10), (6.8, 10)],
    [(6.8, 10), (7.2, 10), (7.6, 10), (8, 10)],
]


def mask(size, paint):
    layer = Image.new("L", (size * SMOOTH, size * SMOOTH), 0)
    unit = size * SMOOTH / GRID

    def box(left, top, right, bottom):
        return [left * unit, top * unit, right * unit - 1, bottom * unit - 1]

    def snapped(left, top, right, bottom):
        edges = [round(edge * size / GRID) for edge in (left, top, right, bottom)]
        return [edges[0] * SMOOTH, edges[1] * SMOOTH, edges[2] * SMOOTH - 1, edges[3] * SMOOTH - 1]

    paint(ImageDraw.Draw(layer), box, snapped, unit)
    return layer


def background(draw, box, snapped, unit):
    draw.rounded_rectangle(box(0, 0, 16, 16), radius=3.5 * unit, fill=255)


def curve(points, steps=64):
    start, first, second, end = points
    for step in range(steps + 1):
        t = step / steps
        weights = ((1 - t) ** 3, 3 * t * (1 - t) ** 2, 3 * t**2 * (1 - t), t**3)
        yield tuple(
            sum(w * p[axis] for w, p in zip(weights, (start, first, second, end)))
            for axis in (0, 1)
        )


def body(draw, box, snapped, unit):
    left = [point for part in LEFT_HALF for point in curve(part)]
    right = [(GRID - x, y) for x, y in reversed(left)]
    draw.polygon([(x * unit, y * unit) for x, y in left + right], fill=255)


def controls(draw, box, snapped, unit):
    draw.rectangle(snapped(4, 7, 7, 8), fill=255)
    draw.rectangle(snapped(5, 6, 6, 9), fill=255)
    draw.ellipse(box(9.35, 7.0, 11.05, 8.7), fill=255)
    draw.ellipse(box(10.95, 5.4, 12.65, 7.1), fill=255)


def render(size):
    shape = ImageChops.subtract(mask(size, body), mask(size, controls))
    large = (size * SMOOTH, size * SMOOTH)
    picture = Image.new("RGBA", large, BLUE[:3] + (0,))
    picture.paste(BLUE, mask=mask(size, background))
    picture.paste(WHITE, mask=shape)
    return picture.resize((size, size), Image.Resampling.BOX)


def bitmap(picture):
    size = picture.width
    header = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = picture.transpose(Image.Transpose.FLIP_TOP_BOTTOM).tobytes("raw", "BGRA")
    stride = (size + 31) // 32 * 4
    return header + pixels + bytes(stride * size)


def png(picture):
    out = io.BytesIO()
    picture.save(out, "PNG", optimize=True)
    return out.getvalue()


def icon(pictures):
    images = [png(p) if p.width >= 256 else bitmap(p) for p in pictures]
    offset = 6 + 16 * len(images)
    directory = struct.pack("<HHH", 0, 1, len(images))
    for picture, image in zip(pictures, images):
        side = picture.width % 256
        directory += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(image), offset)
        offset += len(image)
    return directory + b"".join(images)


def main():
    folder = pathlib.Path(__file__).resolve().parent
    (folder / "heka.ico").write_bytes(icon([render(size) for size in SIZES]))
    (folder / "heka.png").write_bytes(png(render(PICTURE)))


if __name__ == "__main__":
    main()
