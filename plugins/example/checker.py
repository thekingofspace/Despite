import despite

Params = despite.params()
Project = despite.project()
Source = Project.Images[0]
Size = int(Params.get("CellSize", 8))
Result = despite.Pixels.blank(Source.Width, Source.Height)
for Y in range(Source.Height):
    for X in range(Source.Width):
        Light = ((X // Size) + (Y // Size)) % 2 == 0
        Result.set(X, Y, (220, 220, 220, 255) if Light else (160, 160, 160, 255))
Result.paste(Source.Pixels, 0, 0)
despite.output().add_image("Checker", Result)
