#![allow(non_snake_case)]

use despite_script::{OutputMode, ScriptImage, ScriptJob, ScriptLayer, ScriptPixels, find_interpreter, run_blocking};

#[test]
fn readme_example_runs() {
    let Some(Interpreter) = find_interpreter() else {
        eprintln!("python not found; skipping");
        return;
    };
    let Readme = include_str!("../../../README.md");
    let Start = Readme.find("```python\n").expect("README has a python example") + "```python\n".len();
    let End = Start + Readme[Start..].find("```").expect("closed code block");
    let Source = &Readme[Start..End];
    let mut Outline = ScriptPixels::blank(16, 16);
    for Index in 0..16u32 {
        for (X, Y) in [(Index, 2), (Index, 13), (2, Index), (13, Index)] {
            if (2..=13).contains(&X) && (2..=13).contains(&Y) {
                let Offset = ((Y * 16 + X) * 4) as usize;
                Outline.Rgba[Offset..Offset + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
    let Hero = ScriptImage {
        Name: "Hero".into(),
        Composite: Outline.clone(),
        Layers: vec![ScriptLayer::new("Outline", Outline)],
        FrameDelayMs: 100,
    };
    let Job = ScriptJob {
        Source: Source.to_string(),
        Interpreter,
        ProjectName: "Readme".into(),
        Images: vec![Hero],
        Params: Vec::new(),
        Inputs: Vec::new(),
        RequestedMode: OutputMode::Image,
        Timeout: Some(std::time::Duration::from_secs(60)),
    };
    let Output = run_blocking(Job).expect("README example should run");
    let Filled = Output.image("Filled").expect("Filled image");
    let Pixels = &Filled.Frames[0].Pixels;
    let Center = ((8 * 16 + 8) * 4) as usize;
    assert_eq!(&Pixels.Rgba[Center..Center + 4], &[255, 200, 80, 255]);
    let Scene = Output.Scene.expect("scene");
    assert_eq!(Scene.Items.len(), 1);
    assert_eq!(Output.Mode, OutputMode::Scene);
}
