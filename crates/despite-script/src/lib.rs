#![allow(non_snake_case, non_upper_case_globals)]

mod error;
mod handle;
mod interpreter;
mod manifest;
mod runner;
mod types;

pub use error::{ScriptError, traceback_lines};
pub use handle::{ScriptHandle, run_blocking};
pub use interpreter::{InterpreterInfo, find_interpreter, find_interpreter_info, probe_interpreter};
pub use manifest::ManifestVersion;
pub use types::{
    BlendModeNames, MaxDimension, MaxOutputBytes, OutputMode, SceneSource, ScriptEvent, ScriptFrame, ScriptImage,
    ScriptJob, ScriptLayer, ScriptOutput, ScriptOutputImage, ScriptPixels, ScriptScene, ScriptSceneItem, ScriptValue,
    canonical_blend_name,
};

pub const HelperModuleSource: &str = include_str!("../python/despite.py");

pub const ExampleScripts: &[(&str, &str)] = &[
    ("starter", include_str!("../examples/starter.py")),
    ("fill_outline", include_str!("../examples/fill_outline.py")),
    ("glow_animation", include_str!("../examples/glow_animation.py")),
    ("scene_compose", include_str!("../examples/scene_compose.py")),
    ("numpy_noise", include_str!("../examples/numpy_noise.py")),
];

pub fn example_script(Name: &str) -> Option<&'static str> {
    ExampleScripts.iter().find(|(Candidate, _)| *Candidate == Name).map(|(_, Source)| *Source)
}
