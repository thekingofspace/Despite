import math
import random

import despite

Project = despite.project()
Inputs = despite.inputs()
Out = despite.output()
Width = int(despite.param("Width", 320))
Height = int(despite.param("Height", 180))
Random = random.Random(int(despite.param("Seed", 4)))

Sky = despite.Pixels.blank(Width, Height)
for Row in range(Height):
    Sky.rect(0, Row, Width, 1, despite.mix("#1b2a4a", "#f2a65a", Row / max(1, Height - 1)), Fill=True)
Out.add_image("Sky", Sky)

Sun = despite.Pixels.blank(72, 72)
for Ring in range(12, 0, -1):
    Disc = despite.Pixels.blank(72, 72)
    Disc.circle(36, 36, 22 + Ring, "#ffd56b", Fill=True)
    Sun.paste(Disc, Opacity=0.08)
Sun.circle(36, 36, 22, "#fff1c1", Fill=True)
Out.add_image("Sun", Sun)

Twinkle = []
for Index in range(4):
    Star = despite.Pixels.blank(7, 7)
    Brightness = 0.35 + 0.65 * (0.5 - 0.5 * math.cos(math.pi * Index / 2.0))
    Tone = despite.color("#ffffff").with_alpha(Brightness)
    Star.line(3, 0, 3, 6, Tone)
    Star.line(0, 3, 6, 3, Tone)
    Star.set(3, 3, "#ffffff")
    Twinkle.append(Star)
Stars = Out.add_image("Star", Twinkle, DelayMs=120)

Hills = despite.Pixels.blank(Width, 70)
for Column in range(Width):
    Crest = 30 + 12 * math.sin(Column / 37.0) + 6 * math.sin(Column / 11.0 + 1.3)
    Hills.rect(Column, int(Crest), 1, 70 - int(Crest), "#22303c", Fill=True)
Out.add_image("Hills", Hills)

Stage = Out.scene("Evening", Width, Height, Background="#10131f", Fps=8)
Stage.place(Sky)
Stage.place("Sun", Width * 0.68, Height * 0.12, Blend="Screen")
for Index in range(9):
    Stage.place(
        Stars,
        X=Random.uniform(0, Width - 7),
        Y=Random.uniform(0, Height * 0.45),
        Scale=Random.choice([1.0, 1.0, 2.0]),
        Opacity=Random.uniform(0.6, 1.0),
        FrameOffset=Index,
    )

Subject = Inputs.get("Subject") or (Project.Images[0] if Project.Images else None)
if Subject is not None:
    Fit = min(1.0, (Height * 0.6) / max(1, Subject.Height), (Width * 0.4) / max(1, Subject.Width))
    Stage.place(
        Subject,
        X=Width * 0.22,
        Y=Height - 20 - Subject.Height * Fit,
        Scale=Fit,
        Rotation=-6,
    )
    print("Placed project image %r at %.0f%% scale" % (Subject.Name, Fit * 100))
else:
    print("Add an image to the project to see it placed in the scene")

Stage.place(Hills, 0, Height - 70)
print("Scene %r has %d items" % (Stage.Name, len(Stage.Items)))
