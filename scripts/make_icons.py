import json
import math
import shutil
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

Root = Path(__file__).resolve().parent.parent

Fine = 2
FineSize = 1024 * Fine

StemLeft = 256.0
StemRight = 448.0
StemTop = 192.0
StemBottom = 768.0
StrokeOrigin = 832.0
StemCenter = (StemLeft + StemRight) / 2
StemHalf = (StemRight - StemLeft) / 2
BarHalf = 80.0
TopY = StemTop + BarHalf
BowlX = 512.0
BowlY = 512.0
BowlRadius = BowlY - TopY
BottomY = BowlY + BowlRadius
TailEnd = 168.0
StemLength = StrokeOrigin - TopY
ArcStart = StemLength + (BowlX - StemCenter)
TailStart = ArcStart + math.pi * BowlRadius
StrokeEnd = TailStart + (BowlX - TailEnd)

PlateCenter = 512.0
PlateHalf = 448.0
PlateExponent = 5.0
MacScale = 824.0 / 896.0
RoundGlyphScale = 0.88
AdaptiveCanvas = 108.0
AdaptiveViewport = 72.0
AdaptiveGlyphRadius = 30.0

Levels = [(32, BowlX + 110.0, 40.0), (16, BowlX + 225.0, 18.0)]

PixelFamilies = [
    ((246, 242, 255), (222, 211, 255)),
    ((255, 255, 255), (238, 231, 255)),
    ((255, 252, 236), (255, 241, 200)),
    ((255, 244, 172), (255, 224, 112)),
    ((255, 230, 124), (255, 200, 76)),
]
FamilyBounds = [StemLength - 250.0, ArcStart - 80.0, ArcStart + 50.0, ArcStart + 160.0]
DitherReach = 26.0

ColorStops = [
    (0.0, (246, 240, 255)),
    (ArcStart - 40.0, (255, 246, 232)),
    (ArcStart + 150.0, (255, 206, 82)),
    (ArcStart + 480.0, (255, 150, 64)),
    (TailStart + 60.0, (255, 98, 92)),
    (StrokeEnd, (240, 60, 110)),
]

PlateTop = (152, 96, 255)
PlateBottom = (62, 28, 160)
ShadowColor = (24, 6, 70)

IcoSizes = [16, 24, 32, 48, 64, 128, 256]
IcnsSizes = [32, 64, 128, 256, 512, 1024]
LinuxSizes = [16, 24, 32, 48, 64, 128, 256, 512]
AssetSizes = [1024, 512, 256, 128, 64, 48, 32, 24, 16]
AndroidDensities = [("mdpi", 1.0), ("hdpi", 1.5), ("xhdpi", 2.0), ("xxhdpi", 3.0), ("xxxhdpi", 4.0)]

PixelInk = {
    "W": (255, 255, 255),
    "L": (238, 231, 255),
    "M": (222, 211, 255),
    "C": (255, 241, 200),
    "Y": (255, 224, 112),
    "A": (255, 200, 76),
    "O": (255, 150, 64),
    "R": (255, 98, 92),
    "P": (240, 64, 108),
    "p": (240, 64, 108, 0.55),
}
PixelShadow = 0.32

PixelArt = {
    16: [
        "................",
        "................",
        "...WWWWWCY......",
        "...WLLLLCYA.....",
        "...WLLLCYAAA....",
        "...WLL...YAAA...",
        "...WLL....AAA...",
        "...WLL....AAO...",
        "...WLL....AOO...",
        "...WLM....OOO...",
        "...WMM...OOOO...",
        "...WMMRRROOO....",
        ".pPPPRRRROO.....",
        ".....PRRRR......",
        "................",
        "................",
    ],
    24: [
        "........................",
        "........................",
        "........................",
        "....WWWWCCYYAA..........",
        "....WLLLLCCYYAAA........",
        "....WLLLLLCCYYAAA.......",
        "....WLLLLLLCCYYAAA......",
        "....WLLL......AAAAA.....",
        "....WLLL.......AAAA.....",
        "....WLLL........AAAA....",
        "....WLLL........AAAA....",
        "....WLLL........AAAA....",
        "....WLML........AOAO....",
        "....WMLM........OAOO....",
        "....WMMM........OOOO....",
        "....WMMM.......OOOO.....",
        "....WMMM......OOOOO.....",
        "....WMMMRRRROOOOOO......",
        ".ppPPPPPRRRRROOOO.......",
        ".....PPPRRRRRROO........",
        ".......PRRRRRR..........",
        "........................",
        "........................",
        "........................",
    ],
    32: [
        "................................",
        "................................",
        "................................",
        "................................",
        ".....WWWWWWCCYYYYAA.............",
        ".....WLLLLLLCCYYYYAAA...........",
        ".....WLLLLLLLCCYYYYAAAA.........",
        ".....WLLLLLLLLCCYYYYAAAA........",
        ".....WLLLLLLLLLCCYYYYAAAA.......",
        ".....WLLLLL.......AAAAAAAA......",
        ".....WLLLLL.........AAAAAA......",
        ".....WLLLLL..........AAAAAA.....",
        ".....WLLLLL..........AAAAAA.....",
        ".....WLLLLL...........AAAAA.....",
        ".....WLLLLL...........AAAAA.....",
        ".....WLLLLL...........AAAAA.....",
        ".....WMLMLM...........AOAOA.....",
        ".....WLMLML...........OAOAO.....",
        ".....WMMMMM...........OOOOO.....",
        ".....WMMMMM..........OOOOOO.....",
        ".....WMMMMM..........OOOOOO.....",
        ".....WMMMMM.........OOOOOO......",
        ".....WMMMMM.......OOOOOOOO......",
        ".....WMMMMMRRRRRROOOOOOOO.......",
        "....pPPPPPPRRRRRRROOOOOO........",
        ".ppPPPPPPPRRRRRRRROOOOO.........",
        "......PPPPRRRRRRRROOO...........",
        ".........PRRRRRRRRO.............",
        "................................",
        "................................",
        "................................",
        "................................",
    ],
}


def hash_noise(Index, Seed):
    Value = np.sin(np.asarray(Index, dtype=np.float64) * 12.9898 + Seed * 78.233) * 43758.5453
    return (Value - np.floor(Value)).astype(np.float32)


def gradient_color(S):
    Positions = np.array([Stop[0] for Stop in ColorStops], dtype=np.float32)
    Colors = np.array([Stop[1] for Stop in ColorStops], dtype=np.float32) / 255.0
    Result = np.empty(S.shape + (3,), dtype=np.float32)
    for Channel in range(3):
        Result[..., Channel] = np.interp(S, Positions, Colors[:, Channel])
    return Result


def bristles(V):
    Streak = np.zeros_like(V)
    for Index, (Frequency, Amplitude) in enumerate([(3.1, 0.45), (7.3, 0.3), (13.7, 0.2), (23.9, 0.12), (41.0, 0.08)]):
        Streak += Amplitude * np.sin(V * Frequency * math.pi + Index * 1.7 + 0.4)
    return Streak


def wobble(S):
    return 1.0 + 0.012 * np.sin(S * 0.031) + 0.008 * np.sin(S * 0.083 + 1.3)


def pixel_color(S, V, Checker):
    Family = np.zeros(S.shape, dtype=np.int64)
    for Bound in FamilyBounds:
        Shifted = S + np.where(Checker, DitherReach, -DitherReach) * (np.abs(S - Bound) < DitherReach)
        Family += (Shifted >= Bound).astype(np.int64)
    Tone = np.where(V < -0.6, 0, 1)
    Table = np.array(PixelFamilies, dtype=np.float32) / 255.0
    return Table[Family, Tone]


def grid(Size, Scale):
    Coordinates = (np.arange(Size, dtype=np.float32) + 0.5 - Size / 2) / Scale + PlateCenter
    return np.meshgrid(Coordinates, Coordinates)


def main_fields(X, Y, Pixel):
    InStem = (X >= StemLeft) & (X <= StemRight) & (Y >= StemTop) & (Y <= StemBottom)
    InTop = (X >= StemCenter) & (X <= BowlX) & (np.abs(Y - TopY) <= BarHalf)
    StraightS = StemLength + (X - StemCenter) - (Y - TopY)
    StraightV = np.where(InStem & ~InTop, (X - StemCenter) / StemHalf, (Y - TopY) / BarHalf)
    Radius = np.hypot(X - BowlX, Y - BowlY)
    Theta = np.arctan2(Y - BowlY, X - BowlX)
    ArcS = ArcStart + BowlRadius * (Theta + math.pi / 2)
    ArcV = (BowlRadius - Radius) / BarHalf
    ArcAlpha = np.clip(0.5 + (BarHalf * wobble(ArcS) - np.abs(Radius - BowlRadius)) / Pixel, 0.0, 1.0) * (X >= BowlX)
    Straight = InStem | InTop
    S = np.where(Straight, StraightS, ArcS)
    V = np.where(Straight, StraightV, ArcV)
    Alpha = np.where(Straight, 1.0, ArcAlpha).astype(np.float32)
    return S, V, Alpha


def level_coordinate(X, Y):
    return np.where(X < BowlX, X, BowlX + BowlRadius * (np.arctan2(Y - BowlY, X - BowlX) + math.pi / 2))


def tail_fields(X, Y, Pixel):
    U = np.clip((BowlX - X) / (BowlX - TailEnd), 0.0, 1.0)
    S = TailStart + (BowlX - X)
    Half = BarHalf * wobble(S) * (1.0 - 0.66 * U ** 1.6)
    Middle = BottomY - 10.0 * U ** 2
    V = -(Y - Middle) / Half
    Edge = np.clip(0.5 + (Half - np.abs(Y - Middle)) / Pixel, 0.0, 1.0)
    Strand = np.floor((V + 1.0) * 0.5 * 34.0)
    Reach = 1.0 - 0.22 * hash_noise(Strand, 3.0) ** 1.5 - 0.05 * np.abs(V) ** 2
    Along = (BowlX - X) / (BowlX - TailEnd)
    EndFade = np.clip((Reach - Along) * (BowlX - TailEnd) / (Pixel * 1.5), 0.0, 1.0)
    StrandFade = 1.0 - np.clip((Along - Reach + 0.12) / 0.12, 0.0, 1.0) * 0.35 * hash_noise(Strand, 7.0)
    Alpha = Edge * EndFade * StrandFade * (X <= BowlX)
    return S, V, Alpha.astype(np.float32)


def shade(S, V):
    Base = gradient_color(S)
    Light = 1.0 + 0.06 * bristles(V) - 0.05 * np.clip(np.abs(V) - 0.7, 0.0, 1.0) / 0.3 - 0.07 * np.clip(V, 0.0, 1.0)
    Sheen = 0.10 * np.exp(-((V + 0.45) / 0.22) ** 2)
    return np.clip(Base * Light[..., None] + Sheen[..., None], 0.0, 1.0)


def block_mean(Array, Block):
    if Block == 1:
        return Array
    Height, Width = Array.shape[:2]
    Shape = (Height // Block, Block, Width // Block, Block) + Array.shape[2:]
    return Array.reshape(Shape).mean(axis=(1, 3))


def expand(Array, Block):
    return np.repeat(np.repeat(Array, Block, axis=0), Block, axis=1)


def premultiply(Color, Alpha):
    return np.concatenate([Color * Alpha[..., None], Alpha[..., None]], axis=-1).astype(np.float32)


def over(Top, Bottom):
    return Top + Bottom * (1.0 - Top[..., 3:4])


def render_glyph():
    X, Y = grid(FineSize, Fine)
    Pixel = 1.0 / Fine
    S, V, Alpha = main_fields(X, Y, Pixel)
    Smooth = premultiply(shade(S, V), Alpha)
    Result = Smooth.copy()
    Remaining = np.ones((FineSize, FineSize), dtype=bool)
    for Order, (Cell, Threshold, Jitter) in enumerate(Levels):
        Block = int(Cell * Fine)
        Count = FineSize // Block
        Mean = block_mean(Smooth, Block)
        Centers = (np.arange(Count, dtype=np.float32) + 0.5) * Cell
        CellX, CellY = np.meshgrid(Centers, Centers)
        Index = CellY / Cell * 131.0 + CellX / Cell
        Chosen = level_coordinate(CellX, CellY) + (hash_noise(Index, Order + 1.0) - 0.5) * 2.0 * Jitter < Threshold
        CellFields = main_fields(CellX, CellY, Cell)
        Checker = (np.add.outer(np.arange(Count), np.arange(Count)) % 2) == 0
        Solid = (Mean[..., 3] >= 0.5).astype(np.float32)
        Cells = premultiply(pixel_color(CellFields[0], CellFields[1], Checker), Solid)
        Mask = expand(Chosen, Block) & Remaining
        Result[Mask] = expand(Cells, Block)[Mask]
        Remaining &= ~Mask
    TailS, TailV, TailAlpha = tail_fields(X, Y, Pixel)
    return over(premultiply(shade(TailS, TailV), TailAlpha), Result)


def blurred(Alpha, Radius):
    AlphaImage = Image.fromarray(np.round(np.clip(Alpha, 0.0, 1.0) * 255.0).astype(np.uint8), "L")
    return np.asarray(AlphaImage.filter(ImageFilter.GaussianBlur(Radius)), dtype=np.float32) / 255.0


def shifted(Array, Offset):
    Result = np.zeros_like(Array)
    Result[Offset:] = Array[:-Offset]
    return Result


def with_shadow(Glyph):
    Shadow = shifted(blurred(Glyph[..., 3], 18 * Fine), 14 * Fine) * 0.55
    ShadowRgb = np.array(ShadowColor, dtype=np.float32) / 255.0
    return over(Glyph, premultiply(np.broadcast_to(ShadowRgb, Shadow.shape + (3,)), Shadow))


def plate(Size, Scale, Shape):
    X, Y = grid(Size, Scale)
    Pixel = 1.0 / Scale
    DeltaX = np.abs(X - PlateCenter)
    DeltaY = np.abs(Y - PlateCenter)
    if Shape == "squircle":
        Distance = ((DeltaX / PlateHalf) ** PlateExponent + (DeltaY / PlateHalf) ** PlateExponent) ** (1.0 / PlateExponent) * PlateHalf - PlateHalf
    elif Shape == "circle":
        Distance = np.hypot(DeltaX, DeltaY) - PlateHalf
    else:
        Distance = np.full_like(X, -1e6)
    Coverage = np.clip(0.5 - Distance / Pixel, 0.0, 1.0)
    T = np.clip(((X - PlateCenter) * 0.35 + (Y - PlateCenter)) / (2 * PlateHalf) + 0.5, 0.0, 1.0)
    Top = np.array(PlateTop, dtype=np.float32) / 255.0
    Bottom = np.array(PlateBottom, dtype=np.float32) / 255.0
    Color = Top + (Bottom - Top) * T[..., None]
    Glow = np.exp(-(((X - 470.0) / 360.0) ** 2 + ((Y - 430.0) / 330.0) ** 2))
    Color = Color + 0.10 * Glow[..., None] * np.array([0.7, 0.6, 1.0], dtype=np.float32)
    if Shape != "fill":
        RimWidth = max(6.0, 1.1 * Pixel)
        Rim = np.clip(1.0 - np.abs(Distance + RimWidth) / RimWidth, 0.0, 1.0) * np.clip((PlateCenter - Y) / PlateHalf + 0.2, 0.4, 1.0)
        Color = Color + 0.18 * Rim[..., None]
    return premultiply(np.clip(Color, 0.0, 1.0), Coverage.astype(np.float32))


def resample(Array, Size):
    if Array.shape[0] == Size:
        return Array
    if Array.shape[0] % Size == 0:
        return block_mean(Array, Array.shape[0] // Size)
    Channels = []
    for Channel in range(Array.shape[2]):
        Plane = Image.fromarray(np.ascontiguousarray(Array[..., Channel], dtype=np.float32), "F")
        Filter = Image.Resampling.BOX if Size < Array.shape[0] else Image.Resampling.LANCZOS
        Channels.append(np.asarray(Plane.resize((Size, Size), Filter), dtype=np.float32))
    return np.clip(np.stack(Channels, axis=-1), 0.0, 1.0)


def place(Layer, Size, Scale):
    Scaled = resample(Layer, max(1, round(1024 * Scale)))
    Result = np.zeros((Size, Size, 4), dtype=np.float32)
    Offset = round(Size / 2 - PlateCenter * Scale)
    SourceStart = max(0, -Offset)
    TargetStart = max(0, Offset)
    Length = min(Scaled.shape[0] - SourceStart, Size - TargetStart)
    Result[TargetStart:TargetStart + Length, TargetStart:TargetStart + Length] = Scaled[SourceStart:SourceStart + Length, SourceStart:SourceStart + Length]
    return Result


def compose(Layer, Size, PlateScale, GlyphScale, Shape):
    Work = Size * Fine
    Base = plate(Work, PlateScale * Fine, Shape)
    Glyph = place(Layer, Work, GlyphScale * Fine)
    return block_mean(over(Glyph * Base[..., 3:4], Base), Fine)


def to_image(Premultiplied, Mode="RGBA"):
    Alpha = Premultiplied[..., 3:4]
    Color = np.where(Alpha > 0, Premultiplied[..., :3] / np.maximum(Alpha, 1e-6), 0.0)
    Data = np.round(np.clip(np.concatenate([Color, Alpha], axis=-1), 0.0, 1.0) * 255.0).astype(np.uint8)
    Picture = Image.fromarray(Data, "RGBA")
    return Picture.convert("RGB") if Mode == "RGB" else Picture


def small_icon(Size, Master):
    if Size not in PixelArt:
        return resample(Master, Size)
    Base = block_mean(plate(Size * 8, Size * 8 / (2 * PlateHalf), "squircle"), 8)
    Ink = np.zeros((Size, Size, 4), dtype=np.float32)
    for RowIndex, Row in enumerate(PixelArt[Size]):
        for ColumnIndex, Symbol in enumerate(Row):
            if Symbol in PixelInk:
                Entry = PixelInk[Symbol]
                Opacity = Entry[3] if len(Entry) > 3 else 1.0
                Ink[RowIndex, ColumnIndex] = [Entry[0] / 255.0 * Opacity, Entry[1] / 255.0 * Opacity, Entry[2] / 255.0 * Opacity, Opacity]
    Shadow = shifted(Ink[..., 3], 1) * (1.0 - Ink[..., 3]) * PixelShadow
    ShadowRgb = np.array(ShadowColor, dtype=np.float32) / 255.0
    Layer = over(Ink, premultiply(np.broadcast_to(ShadowRgb, Shadow.shape + (3,)), Shadow))
    return over(Layer * Base[..., 3:4], Base)


def glyph_radius(Glyph):
    Rows, Columns = np.nonzero(Glyph[..., 3] > 0.02)
    return float(np.max(np.hypot((Columns + 0.5) / Fine - PlateCenter, (Rows + 0.5) / Fine - PlateCenter)))


def mac_icon(Layer):
    Body = compose(Layer, 1024, MacScale, MacScale, "squircle")
    Drop = shifted(blurred(Body[..., 3], 14), 10) * 0.32
    return over(Body, premultiply(np.zeros(Drop.shape + (3,), dtype=np.float32), Drop))


def tinted(Glyph, Size, Scale):
    Alpha = place(Glyph, Size, Scale)[..., 3]
    return premultiply(np.ones(Alpha.shape + (3,), dtype=np.float32), Alpha)


def save(Premultiplied, Destination, Mode="RGBA"):
    Destination.parent.mkdir(parents=True, exist_ok=True)
    to_image(Premultiplied, Mode).save(Destination, optimize=True)


def write_text(Destination, Text):
    Destination.parent.mkdir(parents=True, exist_ok=True)
    Destination.write_text(Text, encoding="utf-8")


def android_files(Res, Glyph, Layer, Master):
    Round = compose(Layer, 1024, 1.0, RoundGlyphScale, "circle")
    Work = int(AdaptiveCanvas * 16)
    GlyphScale = AdaptiveGlyphRadius * 16 / glyph_radius(Glyph)
    Foreground = place(Layer, Work, GlyphScale)
    Background = plate(Work, AdaptiveViewport / 2 * 16 / PlateHalf, "fill")
    Monochrome = tinted(Glyph, Work, GlyphScale)
    for Density, Factor in AndroidDensities:
        Folder = Res / f"mipmap-{Density}"
        Legacy = round(48 * Factor)
        Adaptive = round(AdaptiveCanvas * Factor)
        save(resample(Master, Legacy), Folder / "ic_launcher.png")
        save(resample(Round, Legacy), Folder / "ic_launcher_round.png")
        save(resample(Foreground, Adaptive), Folder / "ic_launcher_foreground.png")
        save(resample(Background, Adaptive), Folder / "ic_launcher_background.png")
        save(resample(Monochrome, Adaptive), Folder / "ic_launcher_monochrome.png")
    Adaptive = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
        '    <background android:drawable="@mipmap/ic_launcher_background" />\n'
        '    <foreground android:drawable="@mipmap/ic_launcher_foreground" />\n'
        '    <monochrome android:drawable="@mipmap/ic_launcher_monochrome" />\n'
        "</adaptive-icon>\n"
    )
    write_text(Res / "mipmap-anydpi-v26" / "ic_launcher.xml", Adaptive)
    write_text(Res / "mipmap-anydpi-v26" / "ic_launcher_round.xml", Adaptive)
    Middle = [round((Top + Bottom) / 2) for Top, Bottom in zip(PlateTop, PlateBottom)]
    write_text(
        Res / "values" / "ic_launcher_background.xml",
        '<?xml version="1.0" encoding="utf-8"?>\n'
        "<resources>\n"
        f'    <color name="ic_launcher_background">#{Middle[0]:02X}{Middle[1]:02X}{Middle[2]:02X}</color>\n'
        "</resources>\n",
    )


def ios_files(Folder, Layer):
    Scale = 1024.0 / (2 * PlateHalf)
    save(compose(Layer, 1024, Scale, Scale, "fill"), Folder / "icon-1024.png", "RGB")
    Contents = {
        "images": [{"filename": "icon-1024.png", "idiom": "universal", "platform": "ios", "size": "1024x1024"}],
        "info": {"author": "xcode", "version": 1},
    }
    write_text(Folder / "Contents.json", json.dumps(Contents, indent=2) + "\n")


def linux_files(Folder, Icons):
    for Size in LinuxSizes:
        save(Icons[Size], Folder / "hicolor" / f"{Size}x{Size}" / "apps" / "despite.png")
    write_text(
        Folder / "despite.desktop",
        "[Desktop Entry]\n"
        "Type=Application\n"
        "Name=Despite\n"
        "GenericName=Art Program\n"
        "Comment=Pixel art, painting, sprite sheets, scenes and shader effects\n"
        "Exec=despite %F\n"
        "Icon=despite\n"
        "Terminal=false\n"
        "Categories=Graphics;2DGraphics;RasterGraphics;\n"
        "MimeType=application/x-despite;image/png;\n"
        "Keywords=pixel;sprite;paint;drawing;animation;shader;\n"
        "StartupWMClass=despite\n",
    )
    write_text(
        Folder / "despite-mime.xml",
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">\n'
        '    <mime-type type="application/x-despite">\n'
        "        <comment>Despite project</comment>\n"
        '        <sub-class-of type="application/zip"/>\n'
        '        <glob pattern="*.despite"/>\n'
        '        <icon name="despite"/>\n'
        "    </mime-type>\n"
        "</mime-info>\n",
    )


def main():
    OutputRoot = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Root
    IconFolder = OutputRoot / "assets" / "icon"
    Platform = OutputRoot / "platform"
    Glyph = render_glyph()
    Layer = with_shadow(Glyph)
    Master = compose(Layer, 1024, 1.0, 1.0, "squircle")
    Icons = {Size: small_icon(Size, Master) if Size in PixelArt else resample(Master, Size) for Size in AssetSizes}
    for Size in AssetSizes:
        save(Icons[Size], IconFolder / f"despite-{Size}.png")
    IcoImages = [to_image(Icons[Size]) for Size in IcoSizes]
    IcoImages[-1].save(IconFolder / "despite.ico", sizes=[(Size, Size) for Size in IcoSizes], append_images=IcoImages[:-1])
    Mac = mac_icon(Layer)
    IcnsImages = [to_image(resample(Mac, Size)) for Size in IcnsSizes]
    IcnsImages[-1].save(IconFolder / "despite.icns", append_images=IcnsImages)
    (Platform / "windows").mkdir(parents=True, exist_ok=True)
    (Platform / "macos").mkdir(parents=True, exist_ok=True)
    shutil.copyfile(IconFolder / "despite.ico", Platform / "windows" / "despite.ico")
    shutil.copyfile(IconFolder / "despite.icns", Platform / "macos" / "despite.icns")
    ios_files(Platform / "ios" / "AppIcon.appiconset", Layer)
    android_files(Platform / "android" / "res", Glyph, Layer, Master)
    linux_files(Platform / "linux", Icons)


if __name__ == "__main__":
    main()
