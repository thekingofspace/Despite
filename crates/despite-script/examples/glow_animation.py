import math

import despite

Project = despite.project()
Inputs = despite.inputs()
FrameCount = max(2, int(despite.param("Frames", 12)))
GlowColor = despite.color(despite.param("GlowColor", "#ffe27a"))
Radius = max(1.0, float(despite.param("Radius", 6.0)))
DelayMs = int(despite.param("DelayMs", 80))


def sample_star(Size):
    Star = despite.Pixels.blank(Size, Size)
    Center = Size / 2.0
    Points = []
    for Index in range(10):
        Angle = math.pi * Index / 5.0 - math.pi / 2.0
        Reach = Size * (0.45 if Index % 2 == 0 else 0.2)
        Points.append((Center + Reach * math.cos(Angle), Center + Reach * math.sin(Angle)))
    for Index in range(10):
        StartX, StartY = Points[Index]
        EndX, EndY = Points[(Index + 1) % 10]
        Star.line(StartX, StartY, EndX, EndY, "#fff4c2", Thickness=2)
    Star.fill_mask(Star.enclosed(), "#ffd23f")
    return Star


if "Source" in Inputs:
    Shape = Inputs["Source"].Pixels
elif Project.Images:
    Shape = Project.Images[0].Pixels
else:
    Shape = sample_star(48)

Margin = int(math.ceil(Radius * 1.5)) + 1
Width = Shape.Width + Margin * 2
Height = Shape.Height + Margin * 2
Base = despite.Pixels.blank(Width, Height)
Base.paste(Shape, Margin, Margin)

Solid = Base.alpha_mask(64)
Rings = []
Previous = Solid
for Distance in range(1, Margin + 1):
    Grown = Previous.grow(1)
    Rings.append((Distance, Grown - Previous))
    Previous = Grown

Out = despite.output()
for Index in range(FrameCount):
    Phase = 0.5 - 0.5 * math.cos(2.0 * math.pi * Index / FrameCount)
    Reach = Radius * (0.6 + 0.9 * Phase)
    Frame = despite.Pixels.blank(Width, Height)
    for Distance, Ring in Rings:
        Strength = despite.clamp(1.0 - Distance / (Reach + 1.0)) ** 1.6
        if Strength > 0.0:
            Frame.fill_mask(Ring, GlowColor.with_alpha(GlowColor.A * Strength * (0.55 + 0.45 * Phase)))
    Frame.paste(Base)
    Out.add_frame("Glow", Frame, DelayMs)

Out.mode("Gif")
print("Rendered %d frames at %dx%d" % (FrameCount, Width, Height))
