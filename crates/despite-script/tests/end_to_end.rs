#![allow(non_snake_case, non_upper_case_globals)]

use std::path::PathBuf;
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use despite_script::{
    ExampleScripts, HelperModuleSource, OutputMode, SceneSource, ScriptError, ScriptEvent, ScriptHandle, ScriptImage,
    ScriptJob, ScriptLayer, ScriptPixels, ScriptValue, find_interpreter, find_interpreter_info, run_blocking,
};

fn interpreter() -> Option<PathBuf> {
    static Cached: OnceLock<Option<PathBuf>> = OnceLock::new();
    Cached.get_or_init(find_interpreter).clone()
}

fn job(Source: &str) -> Option<ScriptJob> {
    let Some(Interpreter) = interpreter() else {
        eprintln!("skipping: no Python 3 interpreter was found");
        return None;
    };
    let mut Job = ScriptJob::new(Source);
    Job.Interpreter = Interpreter;
    Job.ProjectName = "Test Project".to_string();
    Job.Timeout = Some(Duration::from_secs(120));
    Some(Job)
}

fn sample_image() -> ScriptImage {
    let Rgba: Vec<u8> = (0..24u8).map(|Value| Value * 10).collect();
    let Composite = ScriptPixels::new(3, 2, Rgba);
    ScriptImage {
        Name: "Sample".to_string(),
        Composite: Composite.clone(),
        Layers: vec![
            ScriptLayer::new("Base", Composite),
            ScriptLayer {
                Name: "Outline".to_string(),
                Pixels: ScriptPixels::filled(2, 2, [1, 2, 3, 4]),
                Opacity: 0.5,
                Visible: false,
                OffsetX: 1.0,
                OffsetY: 2.0,
                Blend: "Multiply".to_string(),
            },
        ],
        FrameDelayMs: 70,
    }
}

fn ring_image(Name: &str, Size: u32) -> ScriptImage {
    let Center = Size as f32 / 2.0;
    let mut Pixels = ScriptPixels::blank(Size, Size);
    for Y in 0..Size {
        for X in 0..Size {
            let Distance = ((X as f32 + 0.5 - Center).powi(2) + (Y as f32 + 0.5 - Center).powi(2)).sqrt();
            if (Center * 0.6..=Center * 0.8).contains(&Distance) {
                let Index = ((Y * Size + X) * 4) as usize;
                Pixels.Rgba[Index..Index + 4].copy_from_slice(&[20, 20, 30, 255]);
            }
        }
    }
    let mut Image = ScriptImage::from_pixels(Name, Pixels);
    Image.Layers[0].Name = "Outline".to_string();
    Image
}

fn expect_failure(Outcome: Result<despite_script::ScriptOutput, ScriptError>) -> (Option<i32>, String, ScriptError) {
    match Outcome {
        Err(ScriptError::Failed { ExitCode, Stderr }) => {
            let Rebuilt = ScriptError::Failed { ExitCode, Stderr: Stderr.clone() };
            (ExitCode, Stderr, Rebuilt)
        }
        Err(Other) => panic!("expected a script failure, got {Other:?}"),
        Ok(Output) => panic!("expected a script failure, got output {Output:?}"),
    }
}

#[test]
fn finds_python_three() {
    let Some(Info) = find_interpreter_info() else {
        eprintln!("skipping: no Python 3 interpreter was found");
        return;
    };
    assert!(Info.Version.starts_with("3."), "{Info:?}");
    assert!(Info.Executable.exists(), "{Info:?}");
}

#[test]
fn passthrough_input_image() {
    let Some(mut Job) = job(
        r#"
import despite

Img = despite.inputs()["Source"]
assert despite.project().Name == "Test Project"
assert Img.Name == "Sample" and Img.Width == 3 and Img.Height == 2 and Img.FrameDelayMs == 70
assert Img.Pixels._Data is None
assert [Item.Name for Item in Img.Layers] == ["Base", "Outline"]
Outline = Img.layer("Outline")
assert Outline.Opacity == 0.5 and Outline.Visible is False and Outline.Blend == "Multiply"
assert (Outline.OffsetX, Outline.OffsetY) == (1.0, 2.0)
assert Outline.Pixels.get(1, 1) == (1, 2, 3, 4)
try:
    Img.layer("Missing")
except KeyError as Problem:
    assert "Outline" in str(Problem)
else:
    raise AssertionError("missing layer lookup should fail")
despite.output().add_image("Copy", Img.Pixels)
"#,
    ) else {
        return;
    };
    Job.Images = vec![sample_image()];
    Job.Inputs = vec![("Source".to_string(), 0)];
    let Output = run_blocking(Job).unwrap();
    assert_eq!(Output.Mode, OutputMode::Image);
    assert_eq!(Output.Images.len(), 1);
    assert_eq!(Output.Images[0].Name, "Copy");
    assert_eq!(Output.Images[0].Frames.len(), 1);
    assert_eq!(Output.Images[0].Frames[0].Pixels, sample_image().Composite);
    assert!(Output.Scene.is_none());
}

#[test]
fn generates_multiple_frames() {
    let Some(mut Job) = job(
        r#"
import despite

Out = despite.output()
for Index in range(4):
    Out.add_frame("Pulse", despite.Pixels.blank(5, 4, (Index * 60, 10, 20, 255)), DelayMs=50 + Index)
Canvas = despite.Pixels.blank(2, 2)
Out.add_frame("Snapshot", Canvas)
Canvas.fill("red")
Out.add_frame("Snapshot", Canvas)
"#,
    ) else {
        return;
    };
    Job.RequestedMode = OutputMode::Gif;
    let Output = run_blocking(Job).unwrap();
    assert_eq!(Output.Mode, OutputMode::Gif);
    let Pulse = Output.image("Pulse").unwrap();
    assert_eq!(Pulse.Frames.len(), 4);
    for (Index, Frame) in Pulse.Frames.iter().enumerate() {
        assert_eq!(Frame.DelayMs, 50 + Index as u32);
        assert_eq!((Frame.Pixels.Width, Frame.Pixels.Height), (5, 4));
        assert_eq!(Frame.Pixels.pixel(4, 3), Some([(Index * 60) as u8, 10, 20, 255]));
    }
    let Snapshot = Output.image("Snapshot").unwrap();
    assert_eq!(Snapshot.Frames[0].Pixels.pixel(0, 0), Some([0, 0, 0, 0]));
    assert_eq!(Snapshot.Frames[1].Pixels.pixel(1, 1), Some([255, 0, 0, 255]));
}

#[test]
fn scene_mixes_generated_and_project_images() {
    let Some(mut Job) = job(
        r##"
import despite

Out = despite.output()
Dot = Out.add_image("Dot", despite.Pixels.blank(2, 2, "red"))
Stage = Out.scene("Stage", 64, 32, Background="#000000", Fps=10, FrameCount=5)
Stage.place(Dot, X=3, Y=4, Scale=2, Rotation=45, Opacity=0.5, Blend="screen", FrameOffset=1)
Stage.place(despite.project().image("Sample"), X=10, Y=0)
Stage.place("Sample", X=20, Visible=False)
Stage.place(despite.Pixels.blank(1, 1, "blue"), ScaleX=-1, ScaleY=3)
"##,
    ) else {
        return;
    };
    Job.Images = vec![sample_image()];
    let Output = run_blocking(Job).unwrap();
    assert_eq!(Output.Mode, OutputMode::Scene);
    let Scene = Output.Scene.as_ref().unwrap();
    assert_eq!((Scene.Name.as_str(), Scene.Width, Scene.Height), ("Stage", 64, 32));
    assert_eq!(Scene.Background, Some([0.0, 0.0, 0.0, 1.0]));
    assert_eq!(Scene.Fps, 10.0);
    assert_eq!(Scene.FrameCount, Some(5));
    assert_eq!(Scene.Items.len(), 4);
    let First = &Scene.Items[0];
    assert_eq!(First.Source, SceneSource::Generated("Dot".to_string()));
    assert_eq!((First.X, First.Y, First.ScaleX, First.ScaleY), (3.0, 4.0, 2.0, 2.0));
    assert_eq!((First.Rotation, First.Opacity, First.Blend.as_str(), First.FrameOffset), (45.0, 0.5, "Screen", 1));
    assert_eq!(Scene.Items[1].Source, SceneSource::Project("Sample".to_string()));
    assert_eq!(Scene.Items[2].Source, SceneSource::Project("Sample".to_string()));
    assert!(!Scene.Items[2].Visible);
    let SceneSource::Generated(AutoName) = &Scene.Items[3].Source else {
        panic!("expected a generated source");
    };
    assert_eq!((Scene.Items[3].ScaleX, Scene.Items[3].ScaleY), (-1.0, 3.0));
    assert_eq!(Output.image(AutoName).unwrap().Frames[0].Pixels.pixel(0, 0), Some([0, 0, 255, 255]));
    assert!(Output.image("Dot").is_some());
}

#[test]
fn requested_mode_and_script_override() {
    let Some(mut Plain) = job("import despite\ndespite.output().add_image('A', despite.Pixels.blank(1, 1))\n") else {
        return;
    };
    Plain.RequestedMode = OutputMode::Video;
    assert_eq!(run_blocking(Plain).unwrap().Mode, OutputMode::Video);

    let mut Override =
        job("import despite\ndespite.output().add_image('A', despite.Pixels.blank(1, 1))\ndespite.output().mode('sprite sheet')\n")
            .unwrap();
    Override.RequestedMode = OutputMode::Video;
    assert_eq!(run_blocking(Override).unwrap().Mode, OutputMode::SpriteSheet);

    let SceneScript = "import despite\nOut = despite.output()\nOut.scene('S', 8, 8).place(Out.add_image('A', despite.Pixels.blank(1, 1)))\n";
    assert_eq!(run_blocking(job(SceneScript).unwrap()).unwrap().Mode, OutputMode::Scene);
    let Forced = format!("{SceneScript}Out.mode('Gif')\n");
    assert_eq!(run_blocking(job(&Forced).unwrap()).unwrap().Mode, OutputMode::Gif);

    let mut Fallback = job("import despite\ndespite.output().add_image('A', [despite.Pixels.blank(1, 1)] * 3)\n").unwrap();
    Fallback.RequestedMode = OutputMode::Scene;
    assert_eq!(run_blocking(Fallback).unwrap().Mode, OutputMode::Gif);

    let Missing = job("import despite\ndespite.output().mode('Scene')\n").unwrap();
    assert!(matches!(run_blocking(Missing), Err(ScriptError::InvalidOutput(_))));
}

#[test]
fn exception_reports_traceback() {
    let Some(Job) = job("import despite\nprint('before')\n\ndef explode():\n    return 1 / 0\n\nexplode()\n") else {
        return;
    };
    let (ExitCode, Stderr, Error) = expect_failure(run_blocking(Job));
    assert_eq!(ExitCode, Some(1));
    assert!(Stderr.contains("ZeroDivisionError"), "{Stderr}");
    assert!(Stderr.contains("File \"main.py\", line 5"), "{Stderr}");
    assert!(Stderr.contains("File \"main.py\", line 7"), "{Stderr}");
    assert!(!Stderr.contains("despite-script-"), "{Stderr}");
    assert_eq!(Error.script_line(), Some(5));
    assert!(Error.to_string().contains("main.py line 5"), "{Error}");
    assert!(Error.summary().contains("ZeroDivisionError"), "{}", Error.summary());

    let (_, Stderr, Error) = expect_failure(run_blocking(job("Value = (\n").unwrap()));
    assert!(Stderr.contains("SyntaxError"), "{Stderr}");
    assert_eq!(Error.script_line(), Some(1));

    let (ExitCode, _, _) = expect_failure(run_blocking(job("import sys\nsys.exit(3)\n").unwrap()));
    assert_eq!(ExitCode, Some(3));
}

#[test]
fn failing_save_at_exit_fails_the_run() {
    let Some(Job) = job(
        "import despite\nOut = despite.output()\nItem = Out.scene('S', 4, 4).place(Out.add_image('A', despite.Pixels.blank(1, 1)))\nItem.X = float('nan')\n",
    ) else {
        return;
    };
    let (ExitCode, Stderr, _) = expect_failure(run_blocking(Job));
    assert_eq!(ExitCode, Some(1));
    assert!(Stderr.contains("finite"), "{Stderr}");
}

#[test]
fn timeout_stops_the_script() {
    let Some(mut Job) = job("import time\nprint('sleeping')\ntime.sleep(30)\n") else {
        return;
    };
    Job.Timeout = Some(Duration::from_millis(700));
    let Started = Instant::now();
    let Outcome = run_blocking(Job);
    assert!(matches!(Outcome, Err(ScriptError::Timeout)), "{Outcome:?}");
    assert!(Started.elapsed() < Duration::from_secs(10));
}

#[test]
fn cancel_stops_the_script() {
    let Some(Job) = job("import time\nprint('ready')\ntime.sleep(30)\n") else {
        return;
    };
    let mut Handle = ScriptHandle::spawn(Job);
    let Started = Instant::now();
    let mut Ready = false;
    while !Ready && Started.elapsed() < Duration::from_secs(30) {
        Ready = Handle.poll_events().iter().any(|Event| *Event == ScriptEvent::Stdout("ready".to_string()));
        thread::sleep(Duration::from_millis(10));
    }
    assert!(Ready, "the script never printed 'ready'");
    assert!(!Handle.is_finished());
    Handle.cancel();
    let CancelledAt = Instant::now();
    let Outcome = loop {
        if let Some(Outcome) = Handle.try_result() {
            break Outcome;
        }
        assert!(CancelledAt.elapsed() < Duration::from_secs(10), "cancel did not stop the script");
        thread::sleep(Duration::from_millis(10));
    };
    assert!(matches!(Outcome, Err(ScriptError::Cancelled)), "{Outcome:?}");
    assert!(Handle.is_finished());
    assert!(Handle.try_result().is_none());
}

#[test]
fn invalid_output_is_rejected() {
    let Some(Job) = job(
        r#"
import json
import os

os.makedirs("output", exist_ok=True)
with open(os.path.join("output", "bad.rgba"), "wb") as Handle:
    Handle.write(b"\x00" * 10)
Manifest = {"Version": 1, "Mode": None, "Images": [{"Name": "Bad", "Frames": [{"File": "bad.rgba", "Width": 2, "Height": 2, "DelayMs": 100}]}], "Scene": None}
with open(os.path.join("output", "manifest.json"), "w") as Handle:
    json.dump(Manifest, Handle)
"#,
    ) else {
        return;
    };
    match run_blocking(Job) {
        Err(ScriptError::InvalidOutput(Message)) => assert!(Message.contains("16 bytes"), "{Message}"),
        Other => panic!("expected InvalidOutput, got {Other:?}"),
    }
}

#[test]
fn streams_printed_lines() {
    let Some(Job) = job(
        "import sys\nimport despite\nprint('alpha')\nprint('beta \u{2713}')\nprint('gamma', file=sys.stderr)\ndespite.output().add_image('A', despite.Pixels.blank(1, 1))\n",
    ) else {
        return;
    };
    let mut Handle = ScriptHandle::spawn(Job);
    let mut Events = Vec::new();
    let Started = Instant::now();
    let Outcome = loop {
        Events.extend(Handle.poll_events());
        if let Some(Outcome) = Handle.try_result() {
            break Outcome;
        }
        assert!(Started.elapsed() < Duration::from_secs(60));
        thread::sleep(Duration::from_millis(5));
    };
    Events.extend(Handle.poll_events());
    let Output = Outcome.unwrap();
    assert!(Events.contains(&ScriptEvent::Stdout("alpha".to_string())), "{Events:?}");
    assert!(Events.contains(&ScriptEvent::Stdout("beta \u{2713}".to_string())), "{Events:?}");
    assert!(Events.contains(&ScriptEvent::Stderr("gamma".to_string())), "{Events:?}");
    assert_eq!(Output.Stdout, "alpha\nbeta \u{2713}");
    assert_eq!(Output.Stderr, "gamma");
    assert_eq!(Output.Images.len(), 1);
}

#[test]
fn params_reach_the_script() {
    let Some(mut Job) = job(
        r#"
import despite

Params = despite.params()
assert Params["Speed"] == 1.5 and isinstance(Params["Speed"], float)
assert Params["Count"] == -3 and isinstance(Params["Count"], int)
assert Params["Enabled"] is True
assert Params["Label"] == "hi"
assert Params["Tint"] == (255, 0, 0, 128) and Params["Tint"].A == 128
assert Params["Offset"] == (2.0, 3.0)
assert despite.param("Missing", 7) == 7
assert despite.project().RequestedMode == "SpriteSheet"
try:
    Params["Nope"]
except KeyError as Problem:
    assert "Speed" in str(Problem)
else:
    raise AssertionError("a missing parameter should raise")
print("ok")
"#,
    ) else {
        return;
    };
    Job.RequestedMode = OutputMode::SpriteSheet;
    Job.Params = vec![
        ("Speed".to_string(), ScriptValue::Float(1.5)),
        ("Count".to_string(), ScriptValue::Int(-3)),
        ("Enabled".to_string(), ScriptValue::Bool(true)),
        ("Label".to_string(), ScriptValue::Text("hi".to_string())),
        ("Tint".to_string(), ScriptValue::Color([1.0, 0.0, 0.0, 0.5])),
        ("Offset".to_string(), ScriptValue::Vec2([2.0, 3.0])),
    ];
    let Output = run_blocking(Job).unwrap();
    assert_eq!(Output.Stdout, "ok");
    assert!(Output.is_empty());
    assert_eq!(Output.Mode, OutputMode::SpriteSheet);
}

#[test]
fn scripts_without_the_helper_produce_no_images() {
    let Some(Job) = job("print('hello')\n") else {
        return;
    };
    let Output = run_blocking(Job).unwrap();
    assert!(Output.is_empty());
    assert_eq!(Output.Stdout, "hello");
}

#[test]
fn explicit_save_then_more_frames() {
    let Some(Job) = job(
        "import despite\nOut = despite.output()\nOut.add_frame('A', despite.Pixels.blank(1, 1))\nOut.save()\nOut.save()\nOut.add_frame('A', despite.Pixels.blank(1, 1, 'white'))\n",
    ) else {
        return;
    };
    let Output = run_blocking(Job).unwrap();
    assert_eq!(Output.image("A").unwrap().Frames.len(), 2);
    assert_eq!(Output.image("A").unwrap().Frames[1].Pixels.pixel(0, 0), Some([255, 255, 255, 255]));
}

#[test]
fn bad_jobs_are_reported() {
    let Some(mut Job) = job("print('never')\n") else {
        return;
    };
    Job.Images = vec![sample_image()];
    Job.Inputs = vec![("Source".to_string(), 3)];
    assert!(matches!(run_blocking(Job.clone()), Err(ScriptError::Io(_))));
    Job.Inputs.clear();
    Job.Images[0].Composite.Rgba.pop();
    assert!(matches!(run_blocking(Job.clone()), Err(ScriptError::Io(_))));
    Job.Images.clear();
    Job.Interpreter = PathBuf::from("/definitely/not/a/python");
    assert!(matches!(run_blocking(Job), Err(ScriptError::Spawn(_))));
}

#[test]
fn helper_self_test() {
    let Some(mut Job) = job(
        r##"
import random

import despite
from despite import Color, Mask, Pixels


def check(Condition, Message):
    if not Condition:
        raise AssertionError(Message)


Lazy = despite.project().Images[0].Pixels
check(Lazy._Data is None, "project pixels should load lazily")
check(Lazy.get(0, 0) == (0, 0, 0, 0) and Lazy.get(12, 4) == (20, 20, 30, 255), "lazy load")
check(Lazy._Data is not None, "pixels should be loaded after access")

Base = Pixels.blank(1, 1, (128, 128, 128, 255))
Base.paste(Pixels(1, 1, bytes([128, 255, 0, 255])), Blend="Multiply")
check(Base.get(0, 0) == (64, 128, 0, 255), Base.get(0, 0))
Clear = Pixels.blank(1, 1)
Clear.paste(Pixels(1, 1, bytes([255, 10, 20, 100])), Blend="Multiply")
check(Clear.get(0, 0) == (255, 10, 20, 100), Clear.get(0, 0))
Dark = Pixels.blank(1, 1, (0, 0, 0, 255))
Dark.paste(Pixels(1, 1, bytes([10, 20, 30, 255])), Blend="Screen")
check(Dark.get(0, 0) == (10, 20, 30, 255), Dark.get(0, 0))
Blue = Pixels.blank(1, 1, (0, 0, 255, 255))
Blue.paste(Pixels(1, 1, bytes([255, 0, 0, 128])))
Mixed = Blue.get(0, 0)
check(120 < Mixed.R < 135 and 120 < Mixed.B < 135 and Mixed.A == 255, Mixed)

Random = random.Random(5)


def noise(Width, Height):
    return Pixels(Width, Height, bytes(Random.randrange(256) for Index in range(Width * Height * 4)))


if despite.has_numpy():
    for Mode in despite.BlendModes:
        Back = noise(20, 20)
        Front = noise(17, 19)
        Fast = Back.copy().paste(Front, 2, 1, 0.7, Mode)
        Saved = despite._NumpyModule
        despite._NumpyModule = None
        try:
            Slow = Back.copy().paste(Front, 2, 1, 0.7, Mode)
        finally:
            despite._NumpyModule = Saved
        check(Fast == Slow, "numpy and pure Python disagree for " + Mode)

Ring = Pixels.blank(20, 20)
Ring.circle(10, 10, 6, "black")
Inside = Ring.enclosed()
check(Inside.get(10, 10) and not Inside.get(0, 0) and not Inside.get(10, 4), "enclosed")
Ring.fill_mask(Inside, "red")
check(Ring.get(10, 10) == (255, 0, 0, 255), "fill_mask")
Opaque = Ring.alpha_mask().count()
Filled = Ring.flood_fill(0, 0, "#0000ff")
check(Filled.count() == 400 - Opaque, (Filled.count(), Opaque))
check(Ring.get(0, 0) == (0, 0, 255, 255), "flood_fill")

Dot = Mask(5, 5).set(2, 2)
check(Dot.grow(1).count() == 9 and Dot.grow(2).count() == 25, "grow")
check((Dot.grow(1) - Dot).count() == 8, "subtract")
check((~Dot).count() == 24, "invert")
check(Dot.grow(1).bounds() == (1, 1, 3, 3), "bounds")
check(list(Dot.points()) == [(2, 2)], "points")

check(despite.color("#ff8000") == (255, 128, 0, 255), "hex")
check(despite.color((1.0, 0.5, 0.0)) == (255, 128, 0, 255), "unit floats")
check(despite.color(255, 0, 0, 0.5) == (255, 0, 0, 128), "css alpha")
check(despite.color("red").to_hex() == "#ff0000", "named")
check(despite.hsv(120, 1, 1) == (0, 255, 0, 255), "hsv")
check(Color(255, 0, 0).lerp((0, 0, 255), 0.5) == (128, 0, 128, 255), "lerp")
check(despite.blend_mode("color dodge") == "ColorDodge", "blend names")

Canvas = Pixels.blank(8, 8)
Canvas.rect(1, 1, 6, 6, "white")
check(Canvas.alpha_mask().count() == 20, "rect outline")
Canvas.rect(0, 0, 8, 8, "white", Fill=True)
check(Canvas.alpha_mask().count() == 64, "rect fill")
check(Pixels.blank(8, 8).line(0, 0, 7, 7, "white").alpha_mask().count() == 8, "line")
check(Ring.flip_x().flip_x() == Ring and Ring.flip_y().flip_y() == Ring, "flips")
check(Ring.resize(40, 40).resize(20, 20) == Ring, "nearest resize")
check(Ring.crop(5, 5, 3, 3).get(0, 0) == Ring.get(5, 5), "crop")
check(Ring.map(lambda Value: Value).Data == Ring.Data, "map")

if despite.has_numpy():
    check(Pixels.from_numpy(Ring.to_numpy()) == Ring, "numpy round trip")
if despite.has_pil():
    check(Pixels.from_pil(Ring.to_pil()) == Ring, "PIL round trip")
    Ring.save_png("ring.png")
    check(Pixels.open("ring.png") == Ring, "png round trip")

Out = despite.output()
Out.add_image("A", Pixels.blank(2, 2))
for Attempt in (
    lambda: Out.add_image("A", Pixels.blank(2, 2)),
    lambda: Out.add_frame("A", Pixels.blank(3, 3)),
    lambda: Out.mode("Cinema"),
    lambda: Out.scene("S", 4, 4).place("Nowhere"),
    lambda: Pixels(16385, 1),
):
    try:
        Attempt()
    except (ValueError, KeyError):
        pass
    else:
        raise AssertionError("expected an error")
print("self test passed")
"##,
    ) else {
        return;
    };
    Job.Images = vec![ring_image("Ring", 24)];
    let Output = run_blocking(Job).unwrap();
    assert_eq!(Output.Stdout, "self test passed");
}

#[test]
fn helper_and_examples_have_no_comments() {
    let Some(mut Job) = job(
        r#"
import ast
import io
import tokenize

import despite

for Name, Source in despite.params().items():
    for Token in tokenize.generate_tokens(io.StringIO(Source).readline):
        if Token.type == tokenize.COMMENT:
            raise AssertionError("%s line %d has a comment" % (Name, Token.start[0]))
    Tree = ast.parse(Source)
    for Node in ast.walk(Tree):
        if isinstance(Node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)) and ast.get_docstring(Node):
            raise AssertionError("%s has a docstring" % Name)
print("clean")
"#,
    ) else {
        return;
    };
    Job.Params = vec![("despite.py".to_string(), ScriptValue::Text(HelperModuleSource.to_string()))];
    for (Name, Source) in ExampleScripts {
        Job.Params.push((format!("{Name}.py"), ScriptValue::Text(Source.to_string())));
    }
    assert_eq!(run_blocking(Job).unwrap().Stdout, "clean");
}

#[test]
fn examples_run() {
    if interpreter().is_none() {
        eprintln!("skipping: no Python 3 interpreter was found");
        return;
    }
    let mut Variants: Vec<(String, String)> =
        ExampleScripts.iter().map(|(Name, Source)| (Name.to_string(), Source.to_string())).collect();
    for (Name, Source) in ExampleScripts {
        Variants.push((format!("{Name} (pure Python)"), format!("import sys\nsys.modules['numpy'] = None\n{Source}")));
    }
    for (Name, Source) in Variants {
        for WithProject in [false, true] {
            let mut Job = job(&Source).unwrap();
            if WithProject {
                Job.Images = vec![ring_image("Ring", 24)];
                Job.Inputs = vec![("Outline".to_string(), 0), ("Source".to_string(), 0), ("Subject".to_string(), 0)];
            }
            let Output = match run_blocking(Job) {
                Ok(Output) => Output,
                Err(Error) => panic!("example {Name} (project: {WithProject}) failed: {Error}\n{}", Error.stderr().unwrap_or("")),
            };
            assert!(!Output.Images.is_empty(), "example {Name} produced no images");
            if Name.starts_with("glow_animation") {
                assert_eq!(Output.Mode, OutputMode::Gif);
                assert_eq!(Output.Images[0].Frames.len(), 12);
            }
            if Name.starts_with("scene_compose") {
                assert_eq!(Output.Mode, OutputMode::Scene);
                let Scene = Output.Scene.as_ref().unwrap();
                let HasProject = Scene.Items.iter().any(|Item| matches!(Item.Source, SceneSource::Project(_)));
                assert_eq!(HasProject, WithProject, "{Name}");
            }
            if Name.starts_with("fill_outline") {
                let Filled = &Output.Images[0].Frames[0].Pixels;
                let Center = Filled.Width / 2;
                assert_ne!(Filled.pixel(Center, Center + 2), Some([0, 0, 0, 0]), "{Name} did not fill the interior");
            }
        }
    }
}

#[test]
fn saving_linked_passthrough_keeps_inputs_intact() {
    let Some(mut Job) = job(
        r#"
import despite

Img = despite.inputs()["Source"]
Out = despite.output()
Copy = Out.add_image("Copy", Img.Pixels)
Out.save()
Copy.Frames[0].Pixels.fill("red")
Out.save()
assert Img.Pixels.get(0, 0) == (0, 10, 20, 30), Img.Pixels.get(0, 0)
Copy.Frames[0].Pixels.set(0, 0, "blue")
"#,
    ) else {
        return;
    };
    Job.Images = vec![sample_image()];
    Job.Inputs = vec![("Source".to_string(), 0)];
    let Output = run_blocking(Job).unwrap();
    let Frame = &Output.image("Copy").unwrap().Frames[0].Pixels;
    assert_eq!(Frame.pixel(0, 0), Some([0, 0, 255, 255]));
    assert_eq!(Frame.pixel(1, 0), Some([255, 0, 0, 255]));
}

#[cfg(target_os = "linux")]
#[test]
fn cancel_also_stops_child_processes() {
    let Some(Job) = job(
        "import subprocess\nimport sys\nimport time\nChild = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])\nprint(Child.pid)\ntime.sleep(60)\n",
    ) else {
        return;
    };
    let mut Handle = ScriptHandle::spawn(Job);
    let Started = Instant::now();
    let mut ChildPid = None;
    while ChildPid.is_none() && Started.elapsed() < Duration::from_secs(30) {
        for Event in Handle.poll_events() {
            if let ScriptEvent::Stdout(Text) = Event {
                ChildPid = Text.trim().parse::<u32>().ok();
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    let ChildPid = ChildPid.expect("the script never printed its child's pid");
    Handle.cancel();
    let CancelledAt = Instant::now();
    let Outcome = loop {
        if let Some(Outcome) = Handle.try_result() {
            break Outcome;
        }
        assert!(CancelledAt.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(10));
    };
    assert!(matches!(Outcome, Err(ScriptError::Cancelled)), "{Outcome:?}");
    let StatPath = format!("/proc/{ChildPid}/stat");
    let mut Gone = false;
    while !Gone && CancelledAt.elapsed() < Duration::from_secs(10) {
        Gone = match std::fs::read_to_string(&StatPath) {
            Ok(Stat) => Stat.rsplit(')').next().map(str::trim_start).is_some_and(|Rest| Rest.starts_with('Z')),
            Err(_) => true,
        };
        thread::sleep(Duration::from_millis(20));
    }
    assert!(Gone, "the script's child process {ChildPid} survived cancellation");
}
