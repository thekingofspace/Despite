import despite

Project = despite.project()
Inputs = despite.inputs()
Top = despite.color(despite.param("Top", "#ffd36e"))
Bottom = despite.color(despite.param("Bottom", "#e8467c"))
Shading = float(despite.param("Shading", 0.3))
OutlineNames = ("outline", "outlines", "lines", "line art", "lineart", "ink")


def pick_source():
    if "Outline" in Inputs:
        Chosen = Inputs["Outline"]
    elif Project.Images:
        Chosen = Project.Images[0]
    else:
        return None, "a built-in sample shape"
    for Candidate in Chosen.Layers:
        if Candidate.Name.strip().lower() in OutlineNames:
            return Candidate.Pixels, "layer %r of %r" % (Candidate.Name, Chosen.Name)
    return Chosen.Pixels, "image %r" % Chosen.Name


Art, Description = pick_source()
if Art is None:
    Art = despite.Pixels.blank(96, 96)
    Art.circle(48, 48, 38, "#1d1b2a", Thickness=3)
    Art.line(14, 52, 82, 52, "#1d1b2a", Thickness=3)
Art = Art.copy()

Inside = Art.enclosed()
Box = Inside.bounds()
if Box is None:
    print("No closed outline found in %s. Draw a closed shape on a transparent layer and compile again." % Description)
else:
    Left, Upper, BoxWidth, BoxHeight = Box
    Span = max(1, BoxHeight - 1)
    Art.fill_mask(Inside, lambda X, Y: Top.lerp(Bottom, (Y - Upper) / Span))
    Rim = Inside - Inside.shrink(2)
    Art.fill_mask(Rim, lambda X, Y: Art.get(X, Y).lerp((0, 0, 0, 255), Shading))
    print("Filled %d pixels inside the outline from %s" % (Inside.count(), Description))

despite.output().add_image("Filled", Art)
