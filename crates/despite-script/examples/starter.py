import despite

Project = despite.project()
Out = despite.output()

print("Project %r has %d image(s)" % (Project.Name, len(Project.Images)))
for Entry in Project.Images:
    print("  %s: %dx%d, layers: %s" % (Entry.Name, Entry.Width, Entry.Height, ", ".join(Item.Name for Item in Entry.Layers)))

Canvas = despite.Pixels.blank(64, 64)
Canvas.circle(32, 32, 24, "#ffcc33", Fill=True)
Canvas.circle(32, 32, 24, "#2b2b2b", Thickness=2)
Canvas.line(22, 26, 22, 30, "#2b2b2b", Thickness=2)
Canvas.line(42, 26, 42, 30, "#2b2b2b", Thickness=2)
Canvas.line(22, 40, 32, 45, "#2b2b2b", Thickness=2)
Canvas.line(32, 45, 42, 40, "#2b2b2b", Thickness=2)

Out.add_image("Result", Canvas)
