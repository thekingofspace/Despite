import math
import random

import despite

Width = int(despite.param("Width", 160))
Height = int(despite.param("Height", 120))
Scale = max(2.0, float(despite.param("Scale", 24.0)))
Octaves = max(1, int(despite.param("Octaves", 4)))
Seed = int(despite.param("Seed", 7))
Low = despite.color(despite.param("Low", "#14213d"))
High = despite.color(despite.param("High", "#fca311"))


def fade(T):
    return T * T * (3.0 - 2.0 * T)


def numpy_noise():
    import numpy as Numpy

    Generator = Numpy.random.default_rng(Seed)
    Field = Numpy.zeros((Height, Width))
    Amplitude = 1.0
    Frequency = 1.0 / Scale
    Total = 0.0
    for Octave in range(Octaves):
        Grid = Generator.random((int(Height * Frequency) + 2, int(Width * Frequency) + 2))
        Xs = Numpy.arange(Width) * Frequency
        Ys = Numpy.arange(Height) * Frequency
        Left = Numpy.floor(Xs).astype(int)
        Upper = Numpy.floor(Ys).astype(int)
        Tx = fade(Xs - Left)[None, :]
        Ty = fade(Ys - Upper)[:, None]
        TopRow = Grid[Upper][:, Left] * (1.0 - Tx) + Grid[Upper][:, Left + 1] * Tx
        BottomRow = Grid[Upper + 1][:, Left] * (1.0 - Tx) + Grid[Upper + 1][:, Left + 1] * Tx
        Field += Amplitude * (TopRow * (1.0 - Ty) + BottomRow * Ty)
        Total += Amplitude
        Amplitude *= 0.5
        Frequency *= 2.0
    Field /= Total
    Start = Numpy.array(Low, dtype=float)
    End = Numpy.array(High, dtype=float)
    Colors = Start + (End - Start) * Field[..., None]
    return despite.Pixels.from_numpy(Numpy.clip(Numpy.round(Colors), 0, 255).astype(Numpy.uint8))


def python_noise():
    Generator = random.Random(Seed)
    Layers = []
    Amplitude = 1.0
    Frequency = 1.0 / Scale
    Total = 0.0
    for Octave in range(Octaves):
        Columns = int(Width * Frequency) + 2
        Rows = int(Height * Frequency) + 2
        Grid = [[Generator.random() for Column in range(Columns)] for Row in range(Rows)]
        Layers.append((Grid, Frequency, Amplitude))
        Total += Amplitude
        Amplitude *= 0.5
        Frequency *= 2.0

    def sample(X, Y):
        Value = 0.0
        for Grid, Frequency, Amplitude in Layers:
            Fx = X * Frequency
            Fy = Y * Frequency
            Left = int(Fx)
            Upper = int(Fy)
            Tx = fade(Fx - Left)
            Ty = fade(Fy - Upper)
            TopRow = Grid[Upper][Left] * (1.0 - Tx) + Grid[Upper][Left + 1] * Tx
            BottomRow = Grid[Upper + 1][Left] * (1.0 - Tx) + Grid[Upper + 1][Left + 1] * Tx
            Value += Amplitude * (TopRow * (1.0 - Ty) + BottomRow * Ty)
        T = Value / Total
        return tuple(int(round(Start + (End - Start) * T)) for Start, End in zip(Low, High))

    return despite.Pixels.generate(Width, Height, sample)


if despite.has_numpy():
    Result = numpy_noise()
    print("Generated %dx%d noise with numpy" % (Width, Height))
else:
    Result = python_noise()
    print("numpy is not installed; generated %dx%d noise in pure Python" % (Width, Height))

despite.output().add_image("Noise", Result)
