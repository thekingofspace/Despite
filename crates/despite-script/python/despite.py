import atexit
import json
import math
import numbers
import os
import shutil
import struct
import sys
import traceback
import zlib

__all__ = [
    "BlendModes",
    "Color",
    "Frame",
    "GeneratedImage",
    "Image",
    "Layer",
    "Mask",
    "MaxDimension",
    "NameNotFound",
    "Output",
    "OutputModes",
    "Pixels",
    "Project",
    "Scene",
    "SceneItem",
    "blend_mode",
    "clamp",
    "color",
    "has_numpy",
    "has_pil",
    "hsv",
    "image",
    "inputs",
    "lerp",
    "mix",
    "output",
    "output_mode",
    "param",
    "params",
    "project",
    "smoothstep",
]

MaxDimension = 16384
ManifestVersion = 1
BlendModes = (
    "Normal",
    "Multiply",
    "Screen",
    "Overlay",
    "Darken",
    "Lighten",
    "ColorDodge",
    "ColorBurn",
    "HardLight",
    "SoftLight",
    "Difference",
    "Exclusion",
    "Add",
    "Subtract",
)
OutputModes = ("Image", "SpriteSheet", "Gif", "Video", "Scene")

_Root = os.path.abspath(os.environ.get("DESPITE_WORKDIR") or os.path.dirname(os.path.abspath(__file__)))
_InputDirectory = os.path.join(_Root, "input")
_OutputDirectory = os.path.join(_Root, "output")
_Unset = object()
_NumpyModule = _Unset
_PilModule = _Unset
_ManifestData = _Unset
_ProjectInstance = None
_ParamsInstance = None
_InputsInstance = None
_OutputInstance = None
_ScriptFailed = False
_PreviousExceptHook = sys.excepthook
_BooleanTable = bytes([0] + [1] * 255)
_BrushCache = {}
_NamedColors = {
    "transparent": (0, 0, 0, 0),
    "black": (0, 0, 0, 255),
    "white": (255, 255, 255, 255),
    "red": (255, 0, 0, 255),
    "green": (0, 255, 0, 255),
    "blue": (0, 0, 255, 255),
    "yellow": (255, 255, 0, 255),
    "cyan": (0, 255, 255, 255),
    "magenta": (255, 0, 255, 255),
    "gray": (128, 128, 128, 255),
    "grey": (128, 128, 128, 255),
    "orange": (255, 165, 0, 255),
    "purple": (128, 0, 128, 255),
    "pink": (255, 192, 203, 255),
    "brown": (139, 69, 19, 255),
}


def _except_hook(Kind, Value, Traceback):
    global _ScriptFailed
    _ScriptFailed = True
    _PreviousExceptHook(Kind, Value, Traceback)


sys.excepthook = _except_hook


class NameNotFound(KeyError):
    def __str__(self):
        return str(self.args[0]) if self.args else "name not found"


def _squash(Text):
    return "".join(Character for Character in str(Text).lower() if Character.isalnum())


_BlendLookup = {_squash(Name): Name for Name in BlendModes}
_ModeLookup = {_squash(Name): Name for Name in OutputModes}


def blend_mode(Name):
    Found = _BlendLookup.get(_squash(Name))
    if Found is None:
        raise ValueError("Unknown blend mode %r. Choose one of: %s" % (Name, ", ".join(BlendModes)))
    return Found


def output_mode(Name):
    Found = _ModeLookup.get(_squash(Name))
    if Found is None:
        raise ValueError("Unknown output mode %r. Choose one of: %s" % (Name, ", ".join(OutputModes)))
    return Found


def _numpy():
    global _NumpyModule
    if _NumpyModule is _Unset:
        try:
            import numpy as Module
        except Exception:
            Module = None
        _NumpyModule = Module
    return _NumpyModule


def _pil():
    global _PilModule
    if _PilModule is _Unset:
        try:
            from PIL import Image as Module
        except Exception:
            Module = None
        _PilModule = Module
    return _PilModule


def has_numpy():
    return _numpy() is not None


def has_pil():
    return _pil() is not None


def _require_numpy():
    Module = _numpy()
    if Module is None:
        raise ImportError(
            "numpy is not installed for this Python (%s). Install it with: \"%s\" -m pip install numpy"
            % (sys.executable, sys.executable)
        )
    return Module


def _require_pil():
    Module = _pil()
    if Module is None:
        raise ImportError(
            "Pillow is not installed for this Python (%s). Install it with: \"%s\" -m pip install Pillow"
            % (sys.executable, sys.executable)
        )
    return Module


def clamp(Value, Low=0.0, High=1.0):
    if Value != Value:
        return Low
    if Value < Low:
        return Low
    if Value > High:
        return High
    return Value


def lerp(Start, End, T):
    return Start + (End - Start) * T


def smoothstep(Edge0, Edge1, Value):
    if Edge0 == Edge1:
        return 0.0 if Value < Edge0 else 1.0
    T = clamp((Value - Edge0) / (Edge1 - Edge0))
    return T * T * (3.0 - 2.0 * T)


def _to_byte(Value):
    if Value != Value or Value <= 0:
        return 0
    if Value >= 255:
        return 255
    return int(Value + 0.5)


def _unit_byte(Value):
    if Value != Value or Value <= 0.0:
        return 0
    if Value >= 1.0:
        return 255
    return int(Value * 255.0 + 0.5)


def _unit(Value):
    return float(clamp(float(Value), 0.0, 1.0))


def _is_whole(Value):
    return isinstance(Value, numbers.Integral)


class Color(tuple):
    __slots__ = ()

    def __new__(cls, R=0, G=0, B=0, A=255):
        return tuple.__new__(cls, (_to_byte(R), _to_byte(G), _to_byte(B), _to_byte(A)))

    @property
    def R(self):
        return self[0]

    @property
    def G(self):
        return self[1]

    @property
    def B(self):
        return self[2]

    @property
    def A(self):
        return self[3]

    @classmethod
    def from_floats(cls, R, G, B, A=1.0):
        return cls(R * 255.0, G * 255.0, B * 255.0, A * 255.0)

    @classmethod
    def from_hsv(cls, Hue, Saturation, Value, Alpha=1.0):
        Saturation = clamp(Saturation)
        Value = clamp(Value)
        Chroma = Value * Saturation
        Sector = (Hue % 360.0) / 60.0
        Second = Chroma * (1.0 - abs(Sector % 2.0 - 1.0))
        Index = int(Sector) % 6
        Red, Green, Blue = (
            (Chroma, Second, 0.0),
            (Second, Chroma, 0.0),
            (0.0, Chroma, Second),
            (0.0, Second, Chroma),
            (Second, 0.0, Chroma),
            (Chroma, 0.0, Second),
        )[Index]
        Offset = Value - Chroma
        return cls.from_floats(Red + Offset, Green + Offset, Blue + Offset, clamp(Alpha))

    @classmethod
    def parse(cls, Value, *Rest):
        if Rest:
            Value = (Value,) + tuple(Rest)
        if isinstance(Value, Color):
            return Value
        if isinstance(Value, str):
            return cls._parse_text(Value)
        if isinstance(Value, numbers.Number):
            raise TypeError(
                "A color needs 3 or 4 channels such as (255, 128, 0), (255, 128, 0, 255), (1.0, 0.5, 0.0) or '#ff8000', got %r"
                % (Value,)
            )
        try:
            Values = list(Value)
        except TypeError:
            raise TypeError(
                "Expected a color such as (255, 128, 0), (255, 128, 0, 255), (1.0, 0.5, 0.0) or '#ff8000', got %r"
                % (Value,)
            ) from None
        if len(Values) not in (3, 4):
            raise ValueError("A color needs 3 or 4 channels (R, G, B[, A]), got %d: %r" % (len(Values), Value))
        try:
            Unit = any(not _is_whole(Channel) for Channel in Values) and all(Channel <= 1.0 for Channel in Values[:3])
            Scale = 255.0 if Unit else 1.0
            Red = Values[0] * Scale
            Green = Values[1] * Scale
            Blue = Values[2] * Scale
            Alpha = 255
            if len(Values) == 4:
                Alpha = Values[3]
                if (Unit or not _is_whole(Alpha)) and Alpha <= 1.0:
                    Alpha = Alpha * 255.0
        except TypeError:
            raise TypeError("Color channels must be numbers, got %r" % (Value,)) from None
        return cls(Red, Green, Blue, Alpha)

    @classmethod
    def _parse_text(cls, Text):
        Cleaned = Text.strip().lower()
        if Cleaned in _NamedColors:
            return cls(*_NamedColors[Cleaned])
        Digits = Cleaned[1:] if Cleaned.startswith("#") else Cleaned
        try:
            if len(Digits) in (3, 4):
                Channels = [int(Digit * 2, 16) for Digit in Digits]
            elif len(Digits) in (6, 8):
                Channels = [int(Digits[Index:Index + 2], 16) for Index in range(0, len(Digits), 2)]
            else:
                raise ValueError
        except ValueError:
            raise ValueError(
                "Could not read color %r. Use '#rgb', '#rgba', '#rrggbb', '#rrggbbaa' or a name like 'red'" % (Text,)
            ) from None
        return cls(*Channels)

    def to_floats(self):
        return (self[0] / 255.0, self[1] / 255.0, self[2] / 255.0, self[3] / 255.0)

    def to_hsv(self):
        Red, Green, Blue, Alpha = self.to_floats()
        Maximum = max(Red, Green, Blue)
        Minimum = min(Red, Green, Blue)
        Delta = Maximum - Minimum
        if Delta <= 1e-12:
            Hue = 0.0
        elif Maximum == Red:
            Hue = 60.0 * (((Green - Blue) / Delta) % 6.0)
        elif Maximum == Green:
            Hue = 60.0 * ((Blue - Red) / Delta + 2.0)
        else:
            Hue = 60.0 * ((Red - Green) / Delta + 4.0)
        Saturation = 0.0 if Maximum <= 1e-12 else Delta / Maximum
        return (Hue, Saturation, Maximum, Alpha)

    def to_hex(self):
        if self[3] == 255:
            return "#%02x%02x%02x" % (self[0], self[1], self[2])
        return "#%02x%02x%02x%02x" % tuple(self)

    def with_alpha(self, Alpha):
        if not _is_whole(Alpha) and Alpha <= 1.0:
            Alpha = Alpha * 255.0
        return Color(self[0], self[1], self[2], Alpha)

    def lerp(self, Other, T):
        Target = Color.parse(Other)
        T = clamp(float(T))
        StartAlpha = self[3] / 255.0
        EndAlpha = Target[3] / 255.0
        Alpha = StartAlpha + (EndAlpha - StartAlpha) * T
        if Alpha <= 1e-9:
            return Color(0, 0, 0, 0)
        Channels = []
        for Index in range(3):
            Start = self[Index] * StartAlpha
            End = Target[Index] * EndAlpha
            Channels.append((Start + (End - Start) * T) / Alpha)
        return Color(Channels[0], Channels[1], Channels[2], Alpha * 255.0)

    def __repr__(self):
        return "Color(%d, %d, %d, %d)" % tuple(self)


def color(Value, *Rest):
    return Color.parse(Value, *Rest)


def hsv(Hue, Saturation, Value, Alpha=1.0):
    return Color.from_hsv(Hue, Saturation, Value, Alpha)


def mix(First, Second, T):
    return Color.parse(First).lerp(Second, T)


def _pixel_bytes(Value):
    if Value.__class__ is Color:
        return Value
    if Value.__class__ is tuple and len(Value) == 4:
        Red, Green, Blue, Alpha = Value
        if (
            Red.__class__ is int
            and Green.__class__ is int
            and Blue.__class__ is int
            and Alpha.__class__ is int
            and 0 <= Red <= 255
            and 0 <= Green <= 255
            and 0 <= Blue <= 255
            and 0 <= Alpha <= 255
        ):
            return Value
    return Color.parse(Value)


def _dimension(Value, Label, Minimum=0):
    try:
        Number = int(Value)
    except (TypeError, ValueError):
        raise TypeError("%s must be a whole number, got %r" % (Label, Value)) from None
    if Number != Value:
        raise ValueError("%s must be a whole number, got %r" % (Label, Value))
    if Number < Minimum or Number > MaxDimension:
        raise ValueError("%s must be between %d and %d, got %d" % (Label, Minimum, MaxDimension, Number))
    return Number


def _finite(Value, Label):
    try:
        Number = float(Value)
    except (TypeError, ValueError):
        raise TypeError("%s must be a number, got %r" % (Label, Value)) from None
    if not math.isfinite(Number):
        raise ValueError("%s must be a finite number, got %r" % (Label, Value))
    return Number


def _whole(Value, Label, Minimum=0, Maximum=2 ** 31 - 1):
    Number = _finite(Value, Label)
    Rounded = int(round(Number))
    if Rounded < Minimum or Rounded > Maximum:
        raise ValueError("%s must be between %d and %d, got %r" % (Label, Minimum, Maximum, Value))
    return Rounded


def _name(Value, Label="Name"):
    if not isinstance(Value, str):
        raise TypeError("%s must be text, got %r" % (Label, Value))
    Cleaned = Value.strip()
    if not Cleaned:
        raise ValueError("%s must not be empty" % Label)
    return Cleaned


class _ScalarOps:
    @staticmethod
    def where(Condition, WhenTrue, WhenFalse):
        return WhenTrue if Condition else WhenFalse

    @staticmethod
    def minimum(First, Second):
        return First if First < Second else Second

    @staticmethod
    def maximum(First, Second):
        return First if First > Second else Second

    @staticmethod
    def sqrt(Value):
        return math.sqrt(Value) if Value > 0.0 else 0.0

    @staticmethod
    def absolute(Value):
        return abs(Value)


class _ArrayOps:
    @staticmethod
    def where(Condition, WhenTrue, WhenFalse):
        return _numpy().where(Condition, WhenTrue, WhenFalse)

    @staticmethod
    def minimum(First, Second):
        return _numpy().minimum(First, Second)

    @staticmethod
    def maximum(First, Second):
        return _numpy().maximum(First, Second)

    @staticmethod
    def sqrt(Value):
        Numpy = _numpy()
        return Numpy.sqrt(Numpy.maximum(Value, 0.0))

    @staticmethod
    def absolute(Value):
        return _numpy().abs(Value)


def _hard_light(Ops, Backdrop, Source):
    Scaled = 2.0 * Source - 1.0
    return Ops.where(Source <= 0.5, Backdrop * 2.0 * Source, Backdrop + Scaled - Backdrop * Scaled)


def _blend_multiply(Ops, Backdrop, Source):
    return Backdrop * Source


def _blend_screen(Ops, Backdrop, Source):
    return Backdrop + Source - Backdrop * Source


def _blend_overlay(Ops, Backdrop, Source):
    return _hard_light(Ops, Source, Backdrop)


def _blend_darken(Ops, Backdrop, Source):
    return Ops.minimum(Backdrop, Source)


def _blend_lighten(Ops, Backdrop, Source):
    return Ops.maximum(Backdrop, Source)


def _blend_color_dodge(Ops, Backdrop, Source):
    Ratio = Ops.minimum(Backdrop / Ops.maximum(1.0 - Source, 1e-12), 1.0)
    return Ops.where(Backdrop <= 0.0, 0.0, Ops.where(Source >= 1.0, 1.0, Ratio))


def _blend_color_burn(Ops, Backdrop, Source):
    Ratio = 1.0 - Ops.minimum((1.0 - Backdrop) / Ops.maximum(Source, 1e-12), 1.0)
    return Ops.where(Backdrop >= 1.0, 1.0, Ops.where(Source <= 0.0, 0.0, Ratio))


def _blend_hard_light(Ops, Backdrop, Source):
    return _hard_light(Ops, Backdrop, Source)


def _blend_soft_light(Ops, Backdrop, Source):
    Curve = Ops.where(Backdrop <= 0.25, ((16.0 * Backdrop - 12.0) * Backdrop + 4.0) * Backdrop, Ops.sqrt(Backdrop))
    return Ops.where(
        Source <= 0.5,
        Backdrop - (1.0 - 2.0 * Source) * Backdrop * (1.0 - Backdrop),
        Backdrop + (2.0 * Source - 1.0) * (Curve - Backdrop),
    )


def _blend_difference(Ops, Backdrop, Source):
    return Ops.absolute(Backdrop - Source)


def _blend_exclusion(Ops, Backdrop, Source):
    return Backdrop + Source - 2.0 * Backdrop * Source


def _blend_add(Ops, Backdrop, Source):
    return Ops.minimum(Backdrop + Source, 1.0)


def _blend_subtract(Ops, Backdrop, Source):
    return Ops.maximum(Backdrop - Source, 0.0)


_BlendFunctions = {
    "Normal": None,
    "Multiply": _blend_multiply,
    "Screen": _blend_screen,
    "Overlay": _blend_overlay,
    "Darken": _blend_darken,
    "Lighten": _blend_lighten,
    "ColorDodge": _blend_color_dodge,
    "ColorBurn": _blend_color_burn,
    "HardLight": _blend_hard_light,
    "SoftLight": _blend_soft_light,
    "Difference": _blend_difference,
    "Exclusion": _blend_exclusion,
    "Add": _blend_add,
    "Subtract": _blend_subtract,
}


def _composite_pixel(Backdrop, Source, Opacity, Function):
    SourceAlpha = Source[3] * Opacity / 255.0
    if SourceAlpha <= 0.0:
        return Backdrop
    if Function is None and Source[3] == 255 and Opacity >= 1.0:
        return Source
    BackdropAlpha = Backdrop[3] / 255.0
    OutAlpha = SourceAlpha + BackdropAlpha * (1.0 - SourceAlpha)
    if OutAlpha <= 0.0:
        return (0, 0, 0, 0)
    Remainder = BackdropAlpha * (1.0 - SourceAlpha)
    Result = [0, 0, 0, _unit_byte(OutAlpha)]
    for Channel in (0, 1, 2):
        SourceValue = Source[Channel] / 255.0
        BackdropValue = Backdrop[Channel] / 255.0
        if Function is None:
            Mixed = SourceValue
        else:
            Mixed = (1.0 - BackdropAlpha) * SourceValue + BackdropAlpha * Function(_ScalarOps, BackdropValue, SourceValue)
        Result[Channel] = _unit_byte((SourceAlpha * Mixed + Remainder * BackdropValue) / OutAlpha)
    return Result


def _composite_arrays(Numpy, Region, Patch, Opacity, Function):
    BackdropAlpha = Region[..., 3] / 255.0
    SourceAlpha = Patch[..., 3] * Opacity / 255.0
    OutAlpha = SourceAlpha + BackdropAlpha * (1.0 - SourceAlpha)
    BackdropColor = Region[..., :3] / 255.0
    SourceColor = Patch[..., :3] / 255.0
    if Function is None:
        Mixed = SourceColor
    else:
        Weight = BackdropAlpha[..., None]
        Mixed = (1.0 - Weight) * SourceColor + Weight * Function(_ArrayOps, BackdropColor, SourceColor)
    Remainder = (BackdropAlpha * (1.0 - SourceAlpha))[..., None]
    SafeAlpha = Numpy.where(OutAlpha > 0.0, OutAlpha, 1.0)[..., None]
    OutColor = (SourceAlpha[..., None] * Mixed + Remainder * BackdropColor) / SafeAlpha
    Result = Numpy.empty(Region.shape, dtype=Numpy.uint8)
    Result[..., :3] = Numpy.floor(Numpy.clip(OutColor, 0.0, 1.0) * 255.0 + 0.5)
    Result[..., 3] = Numpy.floor(Numpy.clip(OutAlpha, 0.0, 1.0) * 255.0 + 0.5)
    Result[OutAlpha <= 0.0] = 0
    Keep = SourceAlpha <= 0.0
    Result[Keep] = Region[Keep]
    if Function is None and Opacity >= 1.0:
        Opaque = Patch[..., 3] == 255
        Result[Opaque] = Patch[Opaque]
    Region[...] = Result


def _scan_fill(Region, Width, Height, Seeds):
    Filled = bytearray(len(Region))
    Total = len(Region)
    Stack = list(Seeds)
    while Stack:
        Start = Stack.pop()
        if not Region[Start]:
            continue
        Row = Start - Start % Width
        RowEnd = Row + Width
        Found = Region.rfind(0, Row, Start)
        Left = Row if Found < 0 else Found + 1
        Found = Region.find(0, Start, RowEnd)
        Right = RowEnd if Found < 0 else Found
        Length = Right - Left
        Region[Left:Right] = bytes(Length)
        Filled[Left:Right] = b"\x01" * Length
        for NeighborRow in (Row - Width, Row + Width):
            if NeighborRow < 0 or NeighborRow >= Total:
                continue
            Position = NeighborRow + (Left - Row)
            End = NeighborRow + (Right - Row)
            while Position < End:
                Position = Region.find(1, Position, End)
                if Position < 0:
                    break
                Stack.append(Position)
                Gap = Region.find(0, Position, End)
                if Gap < 0:
                    break
                Position = Gap
    return Filled


def _brush(Thickness):
    Spans = _BrushCache.get(Thickness)
    if Spans is None:
        Low = Thickness // 2
        Shift = 0.0 if Thickness % 2 else 0.5
        Limit = (Thickness / 2.0) ** 2
        Spans = []
        for OffsetY in range(-Low, Thickness - Low):
            Columns = [
                OffsetX
                for OffsetX in range(-Low, Thickness - Low)
                if (OffsetX + Shift) ** 2 + (OffsetY + Shift) ** 2 <= Limit
            ]
            if Columns:
                Spans.append((OffsetY, Columns[0], Columns[-1] + 1))
        _BrushCache[Thickness] = Spans
    return Spans


def _png_chunk(Kind, Payload):
    return struct.pack(">I", len(Payload)) + Kind + Payload + struct.pack(">I", zlib.crc32(Kind + Payload) & 0xFFFFFFFF)


class Pixels:
    __slots__ = ("Width", "Height", "_Data", "_Source")

    def __init__(self, Width, Height, Data=None):
        self.Width = _dimension(Width, "Width")
        self.Height = _dimension(Height, "Height")
        self._Source = None
        Expected = self.Width * self.Height * 4
        if Data is None:
            self._Data = bytearray(Expected)
        else:
            if not isinstance(Data, bytearray):
                Data = bytearray(Data)
            if len(Data) != Expected:
                raise ValueError(
                    "Pixel data for a %dx%d image must be %d bytes of RGBA, got %d"
                    % (self.Width, self.Height, Expected, len(Data))
                )
            self._Data = Data

    @classmethod
    def _deferred(cls, Width, Height, Path):
        Instance = cls.__new__(cls)
        Instance.Width = int(Width)
        Instance.Height = int(Height)
        Instance._Data = None
        Instance._Source = Path
        return Instance

    def _load(self):
        if self._Data is None:
            Expected = self.Width * self.Height * 4
            Buffer = bytearray(Expected)
            View = memoryview(Buffer)
            Filled = 0
            with open(self._Source, "rb") as Handle:
                while Filled < Expected:
                    Count = Handle.readinto(View[Filled:])
                    if not Count:
                        break
                    Filled += Count
                Extra = Handle.read(1)
            View.release()
            if Filled != Expected or Extra:
                raise IOError(
                    "Pixel file %s does not hold %dx%d RGBA data (%d bytes expected)"
                    % (self._Source, self.Width, self.Height, Expected)
                )
            self._Data = Buffer
        return self._Data

    @property
    def Data(self):
        Data = self._Data
        if Data is None:
            Data = self._load()
        return Data

    @Data.setter
    def Data(self, Value):
        if not isinstance(Value, bytearray):
            Value = bytearray(Value)
        Expected = self.Width * self.Height * 4
        if len(Value) != Expected:
            raise ValueError("Pixel data for a %dx%d image must be %d bytes, got %d" % (self.Width, self.Height, Expected, len(Value)))
        self._Data = Value
        self._Source = None

    @property
    def Size(self):
        return (self.Width, self.Height)

    @classmethod
    def blank(cls, Width, Height, Color=(0, 0, 0, 0)):
        Result = cls(Width, Height)
        Value = _pixel_bytes(Color)
        if tuple(Value) != (0, 0, 0, 0):
            Result.fill(Value)
        return Result

    @classmethod
    def generate(cls, Width, Height, Function):
        Result = cls(Width, Height)
        Data = Result._Data
        Index = 0
        for Y in range(Result.Height):
            for X in range(Result.Width):
                Data[Index:Index + 4] = _pixel_bytes(Function(X, Y))
                Index += 4
        return Result

    def __repr__(self):
        return "Pixels(%d x %d)" % (self.Width, self.Height)

    def __eq__(self, Other):
        if not isinstance(Other, Pixels):
            return NotImplemented
        return self.Width == Other.Width and self.Height == Other.Height and self.Data == Other.Data

    __hash__ = None

    def __getitem__(self, Key):
        X, Y = Key
        return self.get(X, Y)

    def __setitem__(self, Key, Value):
        X, Y = Key
        self.set(X, Y, Value)

    def contains(self, X, Y):
        return 0 <= X < self.Width and 0 <= Y < self.Height

    def get(self, X, Y):
        X = int(X)
        Y = int(Y)
        if X < 0 or Y < 0 or X >= self.Width or Y >= self.Height:
            return Color(0, 0, 0, 0)
        Data = self._Data if self._Data is not None else self._load()
        Index = (Y * self.Width + X) * 4
        return tuple.__new__(Color, Data[Index:Index + 4])

    def set(self, X, Y, Color):
        X = int(X)
        Y = int(Y)
        if X < 0 or Y < 0 or X >= self.Width or Y >= self.Height:
            return self
        Data = self._Data if self._Data is not None else self._load()
        Index = (Y * self.Width + X) * 4
        Data[Index:Index + 4] = _pixel_bytes(Color)
        return self

    def plot(self, X, Y, Color, Opacity=1.0, Blend="Normal"):
        X = int(X)
        Y = int(Y)
        if X < 0 or Y < 0 or X >= self.Width or Y >= self.Height:
            return self
        Function = _BlendFunctions[blend_mode(Blend)]
        Data = self.Data
        Index = (Y * self.Width + X) * 4
        Data[Index:Index + 4] = _composite_pixel(Data[Index:Index + 4], _pixel_bytes(Color), _unit(Opacity), Function)
        return self

    def fill(self, Color):
        Value = bytes(_pixel_bytes(Color))
        self.Data[:] = Value * (self.Width * self.Height)
        return self

    def _fill_box(self, X, Y, Width, Height, Value):
        Left = max(0, X)
        Top = max(0, Y)
        Right = min(self.Width, X + Width)
        Bottom = min(self.Height, Y + Height)
        if Right <= Left or Bottom <= Top:
            return
        Data = self.Data
        Span = Value * (Right - Left)
        Stride = self.Width * 4
        for Row in range(Top, Bottom):
            Start = Row * Stride + Left * 4
            Data[Start:Start + len(Span)] = Span

    def rect(self, X, Y, Width, Height, Color, Fill=False, Thickness=1):
        Value = bytes(_pixel_bytes(Color))
        X = int(round(X))
        Y = int(round(Y))
        Width = int(round(Width))
        Height = int(round(Height))
        Thickness = max(1, int(round(Thickness)))
        if Width <= 0 or Height <= 0:
            return self
        if Fill or Thickness * 2 >= min(Width, Height):
            self._fill_box(X, Y, Width, Height, Value)
            return self
        self._fill_box(X, Y, Width, Thickness, Value)
        self._fill_box(X, Y + Height - Thickness, Width, Thickness, Value)
        self._fill_box(X, Y + Thickness, Thickness, Height - 2 * Thickness, Value)
        self._fill_box(X + Width - Thickness, Y + Thickness, Thickness, Height - 2 * Thickness, Value)
        return self

    def _stamp(self, X, Y, Thickness, Value):
        for OffsetY, Start, End in _brush(Thickness):
            self._fill_box(X + Start, Y + OffsetY, End - Start, 1, Value)

    def line(self, X0, Y0, X1, Y1, Color, Thickness=1):
        Value = _pixel_bytes(Color)
        Packed = bytes(Value)
        X = int(round(X0))
        Y = int(round(Y0))
        EndX = int(round(X1))
        EndY = int(round(Y1))
        Thickness = max(1, int(round(Thickness)))
        DeltaX = abs(EndX - X)
        DeltaY = -abs(EndY - Y)
        StepX = 1 if X < EndX else -1
        StepY = 1 if Y < EndY else -1
        Error = DeltaX + DeltaY
        while True:
            if Thickness == 1:
                self.set(X, Y, Value)
            else:
                self._stamp(X, Y, Thickness, Packed)
            if X == EndX and Y == EndY:
                break
            Doubled = 2 * Error
            if Doubled >= DeltaY:
                Error += DeltaY
                X += StepX
            if Doubled <= DeltaX:
                Error += DeltaX
                Y += StepY
        return self

    def circle(self, CenterX, CenterY, Radius, Color, Fill=False, Thickness=1):
        Value = _pixel_bytes(Color)
        Packed = bytes(Value)
        Radius = float(Radius)
        Thickness = max(1, int(round(Thickness)))
        if Radius < 0:
            return self
        if Fill:
            self._disc_rows(float(CenterX), float(CenterY), Radius, -1.0, Packed)
            return self
        if Thickness == 1:
            CenterX = int(round(CenterX))
            CenterY = int(round(CenterY))
            X = int(round(Radius))
            Y = 0
            Decision = 1 - X
            while X >= Y:
                for OffsetX, OffsetY in ((X, Y), (Y, X), (-Y, X), (-X, Y), (-X, -Y), (-Y, -X), (Y, -X), (X, -Y)):
                    self.set(CenterX + OffsetX, CenterY + OffsetY, Value)
                Y += 1
                if Decision < 0:
                    Decision += 2 * Y + 1
                else:
                    X -= 1
                    Decision += 2 * (Y - X) + 1
            return self
        Half = Thickness / 2.0
        self._disc_rows(float(CenterX), float(CenterY), Radius + Half, max(0.0, Radius - Half), Packed)
        return self

    def _disc_rows(self, CenterX, CenterY, Outer, Inner, Value):
        Top = int(math.floor(CenterY - Outer))
        Bottom = int(math.ceil(CenterY + Outer))
        for Row in range(Top, Bottom + 1):
            OffsetY = Row + 0.5 - CenterY
            if OffsetY * OffsetY > Outer * Outer:
                continue
            OuterHalf = math.sqrt(Outer * Outer - OffsetY * OffsetY)
            Left = int(math.ceil(CenterX - OuterHalf - 0.5))
            Right = int(math.floor(CenterX + OuterHalf - 0.5))
            if Inner > 0.0 and OffsetY * OffsetY < Inner * Inner:
                InnerHalf = math.sqrt(Inner * Inner - OffsetY * OffsetY)
                InnerLeft = int(math.ceil(CenterX - InnerHalf - 0.5))
                InnerRight = int(math.floor(CenterX + InnerHalf - 0.5))
                self._fill_box(Left, Row, InnerLeft - Left, 1, Value)
                self._fill_box(InnerRight + 1, Row, Right - InnerRight, 1, Value)
            else:
                self._fill_box(Left, Row, Right - Left + 1, 1, Value)

    def copy(self):
        if self._Data is None:
            return Pixels._deferred(self.Width, self.Height, self._Source)
        return Pixels(self.Width, self.Height, bytearray(self._Data))

    def crop(self, X, Y, Width, Height):
        X = int(round(X))
        Y = int(round(Y))
        Result = Pixels(Width, Height)
        Left = max(0, X)
        Top = max(0, Y)
        Right = min(self.Width, X + Result.Width)
        Bottom = min(self.Height, Y + Result.Height)
        if Right <= Left or Bottom <= Top:
            return Result
        Source = self.Data
        Target = Result._Data
        Length = (Right - Left) * 4
        for Row in range(Top, Bottom):
            SourceStart = (Row * self.Width + Left) * 4
            TargetStart = ((Row - Y) * Result.Width + (Left - X)) * 4
            Target[TargetStart:TargetStart + Length] = Source[SourceStart:SourceStart + Length]
        return Result

    def resize(self, Width, Height):
        Result = Pixels(Width, Height)
        if Result.Width == 0 or Result.Height == 0 or self.Width == 0 or self.Height == 0:
            return Result
        Source = self.Data
        Columns = [min(self.Width - 1, (Column * self.Width) // Result.Width) * 4 for Column in range(Result.Width)]
        Stride = self.Width * 4
        Rows = []
        Cache = {}
        for Row in range(Result.Height):
            SourceRow = min(self.Height - 1, (Row * self.Height) // Result.Height)
            Built = Cache.get(SourceRow)
            if Built is None:
                Line = Source[SourceRow * Stride:(SourceRow + 1) * Stride]
                Built = b"".join(Line[Offset:Offset + 4] for Offset in Columns)
                Cache = {SourceRow: Built}
            Rows.append(Built)
        Result._Data = bytearray(b"".join(Rows))
        return Result

    def flip_x(self):
        Source = self.Data
        Stride = self.Width * 4
        Rows = []
        for Row in range(self.Height):
            Line = Source[Row * Stride:(Row + 1) * Stride]
            Rows.append(b"".join(Line[Offset:Offset + 4] for Offset in range(Stride - 4, -4, -4)))
        return Pixels(self.Width, self.Height, bytearray(b"".join(Rows)))

    def flip_y(self):
        Source = self.Data
        Stride = self.Width * 4
        Rows = [Source[Row * Stride:(Row + 1) * Stride] for Row in range(self.Height - 1, -1, -1)]
        return Pixels(self.Width, self.Height, bytearray(b"".join(Rows)))

    def with_opacity(self, Factor):
        Factor = max(0.0, float(Factor))
        Table = bytes(min(255, int(Value * Factor + 0.5)) for Value in range(256))
        Result = Pixels(self.Width, self.Height, bytearray(self.Data))
        Result._Data[3::4] = Result._Data[3::4].translate(Table)
        return Result

    def paste(self, Source, X=0, Y=0, Opacity=1.0, Blend="Normal"):
        Source = _as_pixels(Source, "paste source")
        Function = _BlendFunctions[blend_mode(Blend)]
        Opacity = _unit(Opacity)
        OffsetX = int(round(X))
        OffsetY = int(round(Y))
        Left = max(0, OffsetX)
        Top = max(0, OffsetY)
        Right = min(self.Width, OffsetX + Source.Width)
        Bottom = min(self.Height, OffsetY + Source.Height)
        if Right <= Left or Bottom <= Top or Opacity <= 0.0:
            return self
        if Source is self:
            Source = self.copy()
        Numpy = _numpy()
        if Numpy is not None and (Right - Left) * (Bottom - Top) >= 256:
            TargetArray = Numpy.frombuffer(self.Data, dtype=Numpy.uint8).reshape(self.Height, self.Width, 4)
            SourceArray = Numpy.frombuffer(Source.Data, dtype=Numpy.uint8).reshape(Source.Height, Source.Width, 4)
            Region = TargetArray[Top:Bottom, Left:Right]
            Patch = SourceArray[Top - OffsetY:Bottom - OffsetY, Left - OffsetX:Right - OffsetX]
            _composite_arrays(Numpy, Region, Patch, Opacity, Function)
            return self
        Target = self.Data
        SourceData = Source.Data
        Simple = Function is None and Opacity >= 1.0
        Length = (Right - Left) * 4
        for Row in range(Top, Bottom):
            TargetStart = (Row * self.Width + Left) * 4
            SourceStart = ((Row - OffsetY) * Source.Width + (Left - OffsetX)) * 4
            SourceRow = SourceData[SourceStart:SourceStart + Length]
            Alphas = SourceRow[3::4]
            if Simple and Alphas.count(255) == len(Alphas):
                Target[TargetStart:TargetStart + Length] = SourceRow
                continue
            if Alphas.count(0) == len(Alphas):
                continue
            for Offset in range(0, Length, 4):
                Alpha = SourceRow[Offset + 3]
                if Alpha == 0:
                    continue
                Index = TargetStart + Offset
                if Simple and Alpha == 255:
                    Target[Index:Index + 4] = SourceRow[Offset:Offset + 4]
                    continue
                Target[Index:Index + 4] = _composite_pixel(Target[Index:Index + 4], SourceRow[Offset:Offset + 4], Opacity, Function)
        return self

    def map(self, Function):
        Source = self.Data
        Result = bytearray(len(Source))
        for Index in range(0, len(Source), 4):
            Result[Index:Index + 4] = _pixel_bytes(Function(tuple.__new__(Color, Source[Index:Index + 4])))
        return Pixels(self.Width, self.Height, Result)

    def map_xy(self, Function):
        Source = self.Data
        Result = bytearray(len(Source))
        Index = 0
        for Y in range(self.Height):
            for X in range(self.Width):
                Result[Index:Index + 4] = _pixel_bytes(Function(X, Y, tuple.__new__(Color, Source[Index:Index + 4])))
                Index += 4
        return Pixels(self.Width, self.Height, Result)

    def alpha_mask(self, Threshold=1):
        Table = bytes(1 if Value >= Threshold else 0 for Value in range(256))
        return Mask(self.Width, self.Height, self.Data[3::4].translate(Table))

    def enclosed(self, Threshold=1):
        Width = self.Width
        Height = self.Height
        Table = bytes(1 if Value < Threshold else 0 for Value in range(256))
        Open = bytearray(self.Data[3::4].translate(Table))
        if Width == 0 or Height == 0:
            return Mask(Width, Height, Open)
        Last = (Height - 1) * Width
        Seeds = [Index for Index in range(Width) if Open[Index]]
        Seeds += [Last + Index for Index in range(Width) if Open[Last + Index]]
        Seeds += [Row * Width for Row in range(Height) if Open[Row * Width]]
        Seeds += [Row * Width + Width - 1 for Row in range(Height) if Open[Row * Width + Width - 1]]
        Outside = _scan_fill(bytearray(Open), Width, Height, Seeds)
        Inside = int.from_bytes(Open, "little") ^ int.from_bytes(Outside, "little")
        return Mask(Width, Height, Inside.to_bytes(len(Open), "little"))

    def _similar(self, Target, Tolerance):
        Data = self.Data
        Count = self.Width * self.Height
        Numpy = _numpy()
        if Numpy is not None and Count >= 256:
            Values = Numpy.frombuffer(Data, dtype=Numpy.uint8).reshape(Count, 4).astype(Numpy.int16)
            Difference = Numpy.abs(Values - Numpy.array(Target, dtype=Numpy.int16)).max(axis=1)
            return bytearray((Difference <= Tolerance).astype(Numpy.uint8).tobytes())
        Region = bytearray(Count)
        Red, Green, Blue, Alpha = Target
        for Pixel in range(Count):
            Index = Pixel * 4
            if (
                abs(Data[Index] - Red) <= Tolerance
                and abs(Data[Index + 1] - Green) <= Tolerance
                and abs(Data[Index + 2] - Blue) <= Tolerance
                and abs(Data[Index + 3] - Alpha) <= Tolerance
            ):
                Region[Pixel] = 1
        return Region

    def select(self, X, Y, Tolerance=0, Contiguous=True):
        X = int(X)
        Y = int(Y)
        if not self.contains(X, Y):
            return Mask(self.Width, self.Height)
        Region = self._similar(self.get(X, Y), max(0, int(Tolerance)))
        if not Contiguous:
            return Mask(self.Width, self.Height, Region)
        return Mask(self.Width, self.Height, _scan_fill(Region, self.Width, self.Height, [Y * self.Width + X]))

    def flood_fill(self, X, Y, Color, Tolerance=0):
        Selected = self.select(X, Y, Tolerance)
        self.fill_mask(Selected, Color)
        return Selected

    def fill_mask(self, Mask, Color):
        if Mask.Width != self.Width or Mask.Height != self.Height:
            raise ValueError("Mask is %dx%d but the image is %dx%d" % (Mask.Width, Mask.Height, self.Width, self.Height))
        Data = self.Data
        Bits = Mask.Data.translate(_BooleanTable)
        Width = self.Width
        if callable(Color):
            Position = Bits.find(1)
            while Position >= 0:
                Index = Position * 4
                Data[Index:Index + 4] = _pixel_bytes(Color(Position % Width, Position // Width))
                Position = Bits.find(1, Position + 1)
            return self
        Value = bytes(_pixel_bytes(Color))
        Position = Bits.find(1)
        Total = len(Bits)
        while Position >= 0:
            End = Bits.find(0, Position)
            if End < 0:
                End = Total
            Data[Position * 4:End * 4] = Value * (End - Position)
            Position = Bits.find(1, End)
        return self

    def to_numpy(self, Copy=True):
        Numpy = _require_numpy()
        if self.Width == 0 or self.Height == 0:
            return Numpy.zeros((self.Height, self.Width, 4), dtype=Numpy.uint8)
        View = Numpy.frombuffer(self.Data, dtype=Numpy.uint8).reshape(self.Height, self.Width, 4)
        return View.copy() if Copy else View

    @classmethod
    def from_numpy(cls, Array):
        Numpy = _require_numpy()
        Values = Numpy.asarray(Array)
        if Values.ndim == 2:
            Values = Values[:, :, None]
        if Values.ndim != 3 or Values.shape[2] not in (1, 2, 3, 4):
            raise ValueError(
                "Expected an array shaped (Height, Width, 4), (Height, Width, 3) or (Height, Width), got %r" % (Values.shape,)
            )
        if Values.dtype == bool:
            Values = Values.astype(Numpy.uint8) * 255
        elif Values.dtype.kind == "f":
            Values = Numpy.floor(Numpy.clip(Numpy.nan_to_num(Values), 0.0, 1.0) * 255.0 + 0.5).astype(Numpy.uint8)
        elif Values.dtype != Numpy.uint8:
            Values = Numpy.clip(Values, 0, 255).astype(Numpy.uint8)
        Height, Width, Channels = Values.shape
        if Channels < 4:
            Opaque = Numpy.full((Height, Width, 1), 255, dtype=Numpy.uint8)
            if Channels == 1:
                Values = Numpy.concatenate([Values, Values, Values, Opaque], axis=2)
            elif Channels == 2:
                Values = Numpy.concatenate([Values[:, :, :1], Values[:, :, :1], Values[:, :, :1], Values[:, :, 1:]], axis=2)
            else:
                Values = Numpy.concatenate([Values, Opaque], axis=2)
        return cls(Width, Height, bytearray(Numpy.ascontiguousarray(Values).tobytes()))

    def to_pil(self):
        Module = _require_pil()
        return Module.frombytes("RGBA", (self.Width, self.Height), bytes(self.Data))

    @classmethod
    def from_pil(cls, Picture):
        _require_pil()
        Converted = Picture if Picture.mode == "RGBA" else Picture.convert("RGBA")
        Width, Height = Converted.size
        return cls(Width, Height, bytearray(Converted.tobytes()))

    @classmethod
    def open(cls, Path):
        Module = _require_pil()
        with Module.open(Path) as Picture:
            Picture.load()
            return cls.from_pil(Picture)

    def save_png(self, Path):
        if self.Width == 0 or self.Height == 0:
            raise ValueError("Cannot save an empty %dx%d image as PNG" % (self.Width, self.Height))
        Data = self.Data
        Stride = self.Width * 4
        Raw = bytearray()
        for Row in range(self.Height):
            Raw.append(0)
            Raw += Data[Row * Stride:(Row + 1) * Stride]
        Header = struct.pack(">IIBBBBB", self.Width, self.Height, 8, 6, 0, 0, 0)
        Encoded = (
            b"\x89PNG\r\n\x1a\n"
            + _png_chunk(b"IHDR", Header)
            + _png_chunk(b"IDAT", zlib.compress(bytes(Raw), 6))
            + _png_chunk(b"IEND", b"")
        )
        with open(Path, "wb") as Handle:
            Handle.write(Encoded)
        return Path


class Mask:
    __slots__ = ("Width", "Height", "Data")

    def __init__(self, Width, Height, Data=None):
        self.Width = _dimension(Width, "Width")
        self.Height = _dimension(Height, "Height")
        Expected = self.Width * self.Height
        if Data is None:
            Data = bytearray(Expected)
        elif not isinstance(Data, bytearray):
            Data = bytearray(Data)
        if len(Data) != Expected:
            raise ValueError("Mask data for %dx%d must be %d bytes, got %d" % (self.Width, self.Height, Expected, len(Data)))
        self.Data = Data

    def __repr__(self):
        return "Mask(%d x %d, %d selected)" % (self.Width, self.Height, self.count())

    def __getitem__(self, Key):
        X, Y = Key
        return self.get(X, Y)

    def __setitem__(self, Key, Value):
        X, Y = Key
        self.set(X, Y, Value)

    def get(self, X, Y):
        X = int(X)
        Y = int(Y)
        if X < 0 or Y < 0 or X >= self.Width or Y >= self.Height:
            return False
        return self.Data[Y * self.Width + X] != 0

    def set(self, X, Y, Value=True):
        X = int(X)
        Y = int(Y)
        if 0 <= X < self.Width and 0 <= Y < self.Height:
            self.Data[Y * self.Width + X] = 1 if Value else 0
        return self

    def count(self):
        return len(self.Data) - self.Data.count(0)

    def any(self):
        return self.count() > 0

    def copy(self):
        return Mask(self.Width, self.Height, bytearray(self.Data))

    def points(self):
        Bits = self.Data.translate(_BooleanTable)
        Width = self.Width
        Position = Bits.find(1)
        while Position >= 0:
            yield (Position % Width, Position // Width)
            Position = Bits.find(1, Position + 1)

    def bounds(self):
        Bits = self.Data.translate(_BooleanTable)
        Width = self.Width
        Left = None
        Right = None
        Top = None
        Bottom = None
        for Row in range(self.Height):
            Start = Row * Width
            First = Bits.find(1, Start, Start + Width)
            if First < 0:
                continue
            Last = Bits.rfind(1, Start, Start + Width)
            if Top is None:
                Top = Row
            Bottom = Row
            Left = First - Start if Left is None else min(Left, First - Start)
            Right = Last - Start if Right is None else max(Right, Last - Start)
        if Top is None:
            return None
        return (Left, Top, Right - Left + 1, Bottom - Top + 1)

    def _bits(self):
        return int.from_bytes(self.Data.translate(_BooleanTable), "little")

    def _from_bits(self, Bits):
        return Mask(self.Width, self.Height, Bits.to_bytes(self.Width * self.Height, "little"))

    def _check(self, Other):
        if not isinstance(Other, Mask):
            return False
        if Other.Width != self.Width or Other.Height != self.Height:
            raise ValueError("Masks differ in size: %dx%d and %dx%d" % (self.Width, self.Height, Other.Width, Other.Height))
        return True

    def __or__(self, Other):
        if not self._check(Other):
            return NotImplemented
        return self._from_bits(self._bits() | Other._bits())

    def __and__(self, Other):
        if not self._check(Other):
            return NotImplemented
        return self._from_bits(self._bits() & Other._bits())

    def __sub__(self, Other):
        if not self._check(Other):
            return NotImplemented
        return self._from_bits(self._bits() & ~Other._bits())

    def __xor__(self, Other):
        if not self._check(Other):
            return NotImplemented
        return self._from_bits(self._bits() ^ Other._bits())

    def __invert__(self):
        return self.invert()

    def invert(self):
        Full = int.from_bytes(b"\x01" * (self.Width * self.Height), "little")
        return self._from_bits(self._bits() ^ Full)

    def grow(self, Radius=1):
        Width = self.Width
        Length = Width * self.Height
        Steps = int(Radius)
        if Length == 0 or Steps <= 0:
            return self.copy()
        Bits = self._bits()
        Full = int.from_bytes(b"\x01" * Length, "little")
        NotFirst = int.from_bytes((b"\x00" + b"\x01" * (Width - 1)) * self.Height, "little")
        NotLast = int.from_bytes((b"\x01" * (Width - 1) + b"\x00") * self.Height, "little")
        RowShift = 8 * Width
        for Step in range(Steps):
            Horizontal = Bits | ((Bits << 8) & NotFirst) | ((Bits >> 8) & NotLast)
            Bits = Horizontal | ((Horizontal << RowShift) & Full) | (Horizontal >> RowShift)
        return self._from_bits(Bits)

    def shrink(self, Radius=1):
        return self.invert().grow(Radius).invert()

    def edge(self):
        return self - self.shrink(1)

    def to_pixels(self, Color=(255, 255, 255, 255), Background=(0, 0, 0, 0)):
        Result = Pixels.blank(self.Width, self.Height, Background)
        Result.fill_mask(self, Color)
        return Result


class Layer:
    def __init__(self, Name, Pixels, Opacity=1.0, Visible=True, OffsetX=0.0, OffsetY=0.0, Blend="Normal"):
        self.Name = Name
        self.Pixels = Pixels
        self.Opacity = Opacity
        self.Visible = Visible
        self.OffsetX = OffsetX
        self.OffsetY = OffsetY
        self.Blend = Blend

    @property
    def Width(self):
        return self.Pixels.Width

    @property
    def Height(self):
        return self.Pixels.Height

    def __repr__(self):
        return "Layer(%r, %dx%d, Opacity=%g, Blend=%s)" % (self.Name, self.Width, self.Height, self.Opacity, self.Blend)


class Image:
    def __init__(self, Name, Pixels, Layers=None, FrameDelayMs=100, Index=None):
        self.Name = Name
        self.Pixels = Pixels
        self.Layers = list(Layers or [])
        self.FrameDelayMs = FrameDelayMs
        self.Index = Index

    @property
    def Width(self):
        return self.Pixels.Width

    @property
    def Height(self):
        return self.Pixels.Height

    def layer(self, Name):
        for Candidate in self.Layers:
            if Candidate.Name == Name:
                return Candidate
        Names = ", ".join(repr(Candidate.Name) for Candidate in self.Layers) or "(no layers)"
        raise NameNotFound("Image %r has no layer named %r. Layers: %s" % (self.Name, Name, Names))

    def __repr__(self):
        return "Image(%r, %dx%d, %d layers)" % (self.Name, self.Width, self.Height, len(self.Layers))


class Project:
    def __init__(self, Name, Images, RequestedMode="Image"):
        self.Name = Name
        self.Images = list(Images)
        self.RequestedMode = RequestedMode

    def image(self, Name):
        for Candidate in self.Images:
            if Candidate.Name == Name:
                return Candidate
        Names = ", ".join(repr(Candidate.Name) for Candidate in self.Images) or "(the project has no images)"
        raise NameNotFound("No project image named %r. Images: %s" % (Name, Names))

    def __repr__(self):
        return "Project(%r, %d images)" % (self.Name, len(self.Images))


class _Lookup(dict):
    def __init__(self, Kind):
        dict.__init__(self)
        self._Kind = Kind

    def __missing__(self, Key):
        Names = ", ".join(repr(Name) for Name in self) or "(none)"
        raise NameNotFound("No %s named %r. Available: %s" % (self._Kind, Key, Names))


def _manifest():
    global _ManifestData
    if _ManifestData is _Unset:
        Path = os.path.join(_InputDirectory, "manifest.json")
        if os.path.exists(Path):
            with open(Path, "r", encoding="utf-8") as Handle:
                _ManifestData = json.load(Handle)
        else:
            _ManifestData = {}
    return _ManifestData


def _pixel_file(Entry):
    return Pixels._deferred(Entry["Width"], Entry["Height"], os.path.join(_InputDirectory, Entry["File"]))


def _known_blend(Name):
    return _BlendLookup.get(_squash(Name), Name)


def project():
    global _ProjectInstance
    if _ProjectInstance is None:
        Manifest = _manifest()
        Images = []
        for Index, Entry in enumerate(Manifest.get("Images", [])):
            Layers = [
                Layer(
                    LayerEntry["Name"],
                    _pixel_file(LayerEntry["Pixels"]),
                    float(LayerEntry.get("Opacity", 1.0)),
                    bool(LayerEntry.get("Visible", True)),
                    float(LayerEntry.get("OffsetX", 0.0)),
                    float(LayerEntry.get("OffsetY", 0.0)),
                    _known_blend(LayerEntry.get("Blend", "Normal")),
                )
                for LayerEntry in Entry.get("Layers", [])
            ]
            Images.append(Image(Entry["Name"], _pixel_file(Entry["Composite"]), Layers, int(Entry.get("FrameDelayMs", 100)), Index))
        _ProjectInstance = Project(Manifest.get("ProjectName", "Untitled"), Images, Manifest.get("RequestedMode", "Image"))
    return _ProjectInstance


def image(Name):
    return project().image(Name)


def _convert_param(Kind, Value):
    if Kind == "Float":
        return float(Value)
    if Kind == "Int":
        return int(Value)
    if Kind == "Bool":
        return bool(Value)
    if Kind == "Text":
        return str(Value)
    if Kind == "Color":
        return Color.from_floats(*Value)
    if Kind == "Vec2":
        return (float(Value[0]), float(Value[1]))
    return Value


def params():
    global _ParamsInstance
    if _ParamsInstance is None:
        Result = _Lookup("parameter")
        for Entry in _manifest().get("Params", []):
            Result[Entry["Name"]] = _convert_param(Entry.get("Type"), Entry.get("Value"))
        _ParamsInstance = Result
    return _ParamsInstance


def param(Name, Default=None):
    return params().get(Name, Default)


def inputs():
    global _InputsInstance
    if _InputsInstance is None:
        Images = project().Images
        Result = _Lookup("input")
        for Entry in _manifest().get("Inputs", []):
            Index = int(Entry["Image"])
            if 0 <= Index < len(Images):
                Result[Entry["Name"]] = Images[Index]
        _InputsInstance = Result
    return _InputsInstance


def _as_pixels(Value, Label="image"):
    if isinstance(Value, Pixels):
        return Value
    if isinstance(Value, (Image, Layer)):
        return Value.Pixels
    if isinstance(Value, GeneratedImage):
        if not Value.Frames:
            raise ValueError("Generated image %r has no frames yet" % Value.Name)
        return Value.Frames[0].Pixels
    if hasattr(Value, "shape") and hasattr(Value, "dtype"):
        return Pixels.from_numpy(Value)
    if hasattr(Value, "tobytes") and hasattr(Value, "mode") and hasattr(Value, "size"):
        return Pixels.from_pil(Value)
    raise TypeError(
        "Expected Pixels, a project Image or Layer, a numpy array or a PIL image for the %s, got %s"
        % (Label, type(Value).__name__)
    )


class Frame:
    __slots__ = ("Pixels", "DelayMs")

    def __init__(self, Pixels, DelayMs=100):
        self.Pixels = Pixels
        self.DelayMs = DelayMs

    def __repr__(self):
        return "Frame(%dx%d, DelayMs=%d)" % (self.Pixels.Width, self.Pixels.Height, self.DelayMs)


class GeneratedImage:
    def __init__(self, Name):
        self.Name = _name(Name)
        self.Frames = []

    @property
    def Width(self):
        return self.Frames[0].Pixels.Width if self.Frames else 0

    @property
    def Height(self):
        return self.Frames[0].Pixels.Height if self.Frames else 0

    def __len__(self):
        return len(self.Frames)

    def __repr__(self):
        return "GeneratedImage(%r, %dx%d, %d frames)" % (self.Name, self.Width, self.Height, len(self.Frames))

    def add_frame(self, Source, DelayMs=None):
        Snapshot = _as_pixels(Source, "frame").copy()
        if Snapshot.Width < 1 or Snapshot.Height < 1:
            raise ValueError("Frames of %r must be at least 1x1, got %dx%d" % (self.Name, Snapshot.Width, Snapshot.Height))
        if self.Frames and (Snapshot.Width, Snapshot.Height) != (self.Width, self.Height):
            raise ValueError(
                "Every frame of %r must be %dx%d, got %dx%d" % (self.Name, self.Width, self.Height, Snapshot.Width, Snapshot.Height)
            )
        if DelayMs is None:
            DelayMs = self.Frames[-1].DelayMs if self.Frames else 100
        self.Frames.append(Frame(Snapshot, _whole(DelayMs, "DelayMs")))
        return self


class SceneItem:
    def __init__(self, Kind, Name, X=0.0, Y=0.0, ScaleX=1.0, ScaleY=1.0, Rotation=0.0, Opacity=1.0, Blend="Normal", Visible=True, FrameOffset=0):
        self.Kind = Kind
        self.Name = Name
        self.X = _finite(X, "X")
        self.Y = _finite(Y, "Y")
        self.ScaleX = _finite(ScaleX, "ScaleX")
        self.ScaleY = _finite(ScaleY, "ScaleY")
        self.Rotation = _finite(Rotation, "Rotation")
        self.Opacity = _unit(_finite(Opacity, "Opacity"))
        self.Blend = blend_mode(Blend)
        self.Visible = bool(Visible)
        self.FrameOffset = _whole(FrameOffset, "FrameOffset")

    def __repr__(self):
        return "SceneItem(%s %r at %g, %g)" % (self.Kind, self.Name, self.X, self.Y)

    def _to_json(self):
        return {
            "Source": {"Kind": self.Kind, "Name": self.Name},
            "X": _finite(self.X, "X"),
            "Y": _finite(self.Y, "Y"),
            "ScaleX": _finite(self.ScaleX, "ScaleX"),
            "ScaleY": _finite(self.ScaleY, "ScaleY"),
            "Rotation": _finite(self.Rotation, "Rotation"),
            "Opacity": _unit(_finite(self.Opacity, "Opacity")),
            "Blend": blend_mode(self.Blend),
            "Visible": bool(self.Visible),
            "FrameOffset": _whole(self.FrameOffset, "FrameOffset"),
        }


class Scene:
    def __init__(self, Owner, Name, Width, Height, Background=None, Fps=12, FrameCount=None):
        self._Owner = Owner
        self.Name = _name(Name)
        self.Width = _dimension(Width, "Scene width", 1)
        self.Height = _dimension(Height, "Scene height", 1)
        self.Background = None if Background is None else Color.parse(Background)
        self.Fps = _finite(Fps, "Fps")
        if self.Fps <= 0.0:
            raise ValueError("Fps must be greater than zero, got %r" % (Fps,))
        self.FrameCount = None if FrameCount is None else _whole(FrameCount, "FrameCount", 1)
        self.Items = []

    def __repr__(self):
        return "Scene(%r, %dx%d, %d items)" % (self.Name, self.Width, self.Height, len(self.Items))

    def place(self, Source, X=0, Y=0, ScaleX=1.0, ScaleY=1.0, Rotation=0.0, Opacity=1.0, Blend="Normal", Visible=True, FrameOffset=0, Scale=None):
        Kind, Name = self._Owner._resolve_source(Source)
        if Scale is not None:
            ScaleX = Scale
            ScaleY = Scale
        Item = SceneItem(Kind, Name, X, Y, ScaleX, ScaleY, Rotation, Opacity, Blend, Visible, FrameOffset)
        self.Items.append(Item)
        return Item

    def remove(self, Item):
        self.Items = [Existing for Existing in self.Items if Existing is not Item]
        return self

    def clear(self):
        self.Items = []
        return self

    def _to_json(self):
        return {
            "Name": self.Name,
            "Width": _dimension(self.Width, "Scene width", 1),
            "Height": _dimension(self.Height, "Scene height", 1),
            "Background": None if self.Background is None else list(Color.parse(self.Background).to_floats()),
            "Fps": _finite(self.Fps, "Fps"),
            "FrameCount": None if self.FrameCount is None else _whole(self.FrameCount, "FrameCount", 1),
            "Items": [Item._to_json() for Item in self.Items],
        }


def _fingerprint(Source):
    if Source._Data is None:
        return (id(Source), Source._Source)
    return (id(Source), len(Source._Data), zlib.crc32(Source._Data))


def _write_pixels(Source, Path):
    if os.path.lexists(Path):
        os.remove(Path)
    if Source._Data is None and Source._Source is not None:
        try:
            os.link(Source._Source, Path)
            return
        except (OSError, AttributeError, NotImplementedError):
            pass
        shutil.copyfile(Source._Source, Path)
        return
    with open(Path, "wb") as Handle:
        Handle.write(Source.Data)


class Output:
    def __init__(self):
        self.Images = []
        self.Mode = None
        self.Scene = None
        self._AutoNames = 0
        self._LastSaved = None

    def __repr__(self):
        return "Output(%d images, Mode=%s, Scene=%r)" % (len(self.Images), self.Mode, self.Scene)

    def _find(self, Name):
        for Candidate in self.Images:
            if Candidate.Name == Name:
                return Candidate
        return None

    def image(self, Name):
        Found = self._find(Name)
        if Found is None:
            Names = ", ".join(repr(Candidate.Name) for Candidate in self.Images) or "(none yet)"
            raise NameNotFound("No generated image named %r. Generated images: %s" % (Name, Names))
        return Found

    def add_image(self, Name, Source, DelayMs=100):
        Name = _name(Name)
        if self._find(Name) is not None:
            raise ValueError("An output image named %r already exists; use add_frame to append frames to it" % Name)
        Frames = list(Source) if isinstance(Source, (list, tuple)) else [Source]
        if not Frames:
            raise ValueError("add_image(%r) needs at least one frame" % Name)
        Delays = list(DelayMs) if isinstance(DelayMs, (list, tuple)) else [DelayMs] * len(Frames)
        if len(Delays) != len(Frames):
            raise ValueError("add_image(%r) got %d frames but %d delays" % (Name, len(Frames), len(Delays)))
        Created = GeneratedImage(Name)
        for Entry, Delay in zip(Frames, Delays):
            Created.add_frame(Entry, Delay)
        self.Images.append(Created)
        return Created

    def add_frame(self, Name, Frame, DelayMs=100):
        Existing = self._find(_name(Name))
        if Existing is None:
            Existing = GeneratedImage(Name)
            Existing.add_frame(Frame, DelayMs)
            self.Images.append(Existing)
        else:
            Existing.add_frame(Frame, DelayMs)
        return Existing

    def scene(self, Name, Width, Height, Background=None, Fps=12, FrameCount=None):
        if self.Scene is not None:
            if self.Scene.Name == Name:
                return self.Scene
            raise ValueError("Only one scene per compile is supported; scene %r already exists" % self.Scene.Name)
        self.Scene = Scene(self, Name, Width, Height, Background, Fps, FrameCount)
        return self.Scene

    def mode(self, Name):
        self.Mode = None if Name is None else output_mode(Name)
        return self

    def _resolve_source(self, Source):
        if isinstance(Source, GeneratedImage):
            if not any(Candidate is Source for Candidate in self.Images):
                raise ValueError("Generated image %r does not belong to this output" % Source.Name)
            return ("Generated", Source.Name)
        if isinstance(Source, Image):
            return ("Project", Source.Name)
        if isinstance(Source, str):
            if self._find(Source) is not None:
                return ("Generated", Source)
            Known = project().Images
            if any(Candidate.Name == Source for Candidate in Known):
                return ("Project", Source)
            Generated = ", ".join(repr(Candidate.Name) for Candidate in self.Images) or "(none)"
            Existing = ", ".join(repr(Candidate.Name) for Candidate in Known) or "(none)"
            raise NameNotFound(
                "No generated or project image named %r. Generated: %s. Project images: %s" % (Source, Generated, Existing)
            )
        Converted = _as_pixels(Source, "scene source")
        while True:
            self._AutoNames += 1
            Name = "Scene Item %d" % self._AutoNames
            if self._find(Name) is None:
                break
        self.add_image(Name, Converted)
        return ("Generated", Name)

    def save(self):
        ImageEntries = []
        Planned = []
        for ImageIndex, Generated in enumerate(self.Images):
            FrameEntries = []
            for FrameIndex, Entry in enumerate(Generated.Frames):
                FileName = "%d_%d.rgba" % (ImageIndex, FrameIndex)
                Planned.append((Entry.Pixels, FileName))
                FrameEntries.append(
                    {
                        "File": FileName,
                        "Width": Entry.Pixels.Width,
                        "Height": Entry.Pixels.Height,
                        "DelayMs": _whole(Entry.DelayMs, "DelayMs"),
                    }
                )
            ImageEntries.append({"Name": Generated.Name, "Frames": FrameEntries})
        Manifest = {
            "Version": ManifestVersion,
            "Mode": self.Mode,
            "Images": ImageEntries,
            "Scene": None if self.Scene is None else self.Scene._to_json(),
        }
        Text = json.dumps(Manifest, allow_nan=False, indent=1)
        Signature = (Text, tuple(_fingerprint(Source) for Source, FileName in Planned))
        if Signature == self._LastSaved:
            return self
        os.makedirs(_OutputDirectory, exist_ok=True)
        for Source, FileName in Planned:
            _write_pixels(Source, os.path.join(_OutputDirectory, FileName))
        Temporary = os.path.join(_OutputDirectory, "manifest.json.tmp")
        with open(Temporary, "w", encoding="utf-8") as Handle:
            Handle.write(Text)
        os.replace(Temporary, os.path.join(_OutputDirectory, "manifest.json"))
        self._LastSaved = Signature
        return self


def _save_at_exit():
    if _OutputInstance is None or _ScriptFailed:
        return
    try:
        _OutputInstance.save()
    except BaseException:
        traceback.print_exc()
        try:
            sys.stdout.flush()
            sys.stderr.flush()
        finally:
            os._exit(1)


def output():
    global _OutputInstance
    if _OutputInstance is None:
        _OutputInstance = Output()
        atexit.register(_save_at_exit)
    return _OutputInstance
