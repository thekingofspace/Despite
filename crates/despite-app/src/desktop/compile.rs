use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use despite_core::export::{self, Frame, FrameSource, SheetOptions};
use despite_core::params::ParamValue;
use despite_core::{
    BlendMode, CompileMode, DocumentRef, ImageId, Layer, LayerTransform, Playback, Project, ProjectScript, ProjectShader, Raster, SceneId, SceneItem, ShaderTarget,
};
use despite_script::{OutputMode, ScriptHandle, ScriptImage, ScriptJob, ScriptLayer, ScriptPixels, ScriptScene, ScriptValue, SceneSource};
use despite_shader::{ShaderImage, ShaderInput, ShaderJob, ShaderParam, ShaderValue};
use egui::RichText;

use super::{CodeTab, RunningScript, ShaderWorker, VideoJob};
use crate::app::DespiteApp;

pub struct CompiledResult {
    pub Title: String,
    pub Mode: CompileMode,
    pub Images: Vec<(String, Vec<Frame>)>,
    pub Scene: Option<ScriptScene>,
    pub Preview: Vec<Frame>,
    pub Textures: Vec<egui::TextureHandle>,
    pub Current: usize,
    pub Clock: f64,
    pub Playing: bool,
    pub Zoom: f32,
}

fn to_compile_mode(Mode: OutputMode) -> CompileMode {
    match Mode {
        OutputMode::Image => CompileMode::Image,
        OutputMode::SpriteSheet => CompileMode::SpriteSheet,
        OutputMode::Gif => CompileMode::Gif,
        OutputMode::Video => CompileMode::Video,
        OutputMode::Scene => CompileMode::Scene,
    }
}

fn to_output_mode(Mode: CompileMode) -> OutputMode {
    match Mode {
        CompileMode::Image => OutputMode::Image,
        CompileMode::SpriteSheet => OutputMode::SpriteSheet,
        CompileMode::Gif => OutputMode::Gif,
        CompileMode::Video => OutputMode::Video,
        CompileMode::Scene => OutputMode::Scene,
    }
}

fn pixels_of(Source: &Raster) -> ScriptPixels {
    ScriptPixels::new(Source.width(), Source.height(), Source.data().to_vec())
}

pub fn run_script(App: &mut DespiteApp) {
    if App.Desktop.Running.is_some() {
        App.set_status("A script is already running");
        return;
    }
    let Index = App.Desktop.ActiveScript;
    let Some(Script) = App.Project.Scripts.get(Index).cloned() else {
        App.set_status("No script to run");
        return;
    };
    if !App.Desktop.Trusted {
        App.Desktop.AskTrust = true;
        return;
    }
    let Some(Interpreter) = super::interpreter(App) else {
        App.Desktop.log(true, "Python 3 was not found. Install Python 3 or set its path in Settings → Python.");
        App.Desktop.Tab = CodeTab::Console;
        App.ShowBottom = true;
        return;
    };
    let Job = script_job(App, &Script, Interpreter);
    App.Desktop.ErrorLine = None;
    App.Desktop.log(false, format!("▶ Running {} …", Script.Name));
    App.Desktop.Running = Some(RunningScript { Handle: ScriptHandle::spawn(Job), Name: Script.Name.clone(), Started: App.Now });
}

pub fn script_job(App: &DespiteApp, Script: &ProjectScript, Interpreter: PathBuf) -> ScriptJob {
    let Images: Vec<ScriptImage> = App
        .Project
        .Images
        .iter()
        .map(|Found| ScriptImage {
            Name: Found.Name.clone(),
            Composite: pixels_of(&Found.composite()),
            Layers: Found
                .Layers
                .iter()
                .map(|Entry| ScriptLayer {
                    Name: Entry.Name.clone(),
                    Pixels: pixels_of(&Entry.rasterize_in_image_space(Found.Width, Found.Height, Found.smooth_sampling())),
                    Opacity: Entry.Opacity,
                    Visible: Entry.Visible,
                    OffsetX: 0.0,
                    OffsetY: 0.0,
                    Blend: Entry.Blend.name().replace(' ', ""),
                })
                .collect(),
            FrameDelayMs: Found.FrameDelayMs,
        })
        .collect();
    let mut Params = Vec::new();
    let mut Inputs = Vec::new();
    for Decl in &Script.Params {
        match &Decl.Value {
            ParamValue::Float(Value) => Params.push((Decl.Name.clone(), ScriptValue::Float(*Value as f64))),
            ParamValue::Int(Value) => Params.push((Decl.Name.clone(), ScriptValue::Int(*Value as i64))),
            ParamValue::Bool(Value) => Params.push((Decl.Name.clone(), ScriptValue::Bool(*Value))),
            ParamValue::Color(Value) => Params.push((Decl.Name.clone(), ScriptValue::Color(Value.to_array()))),
            ParamValue::Vec2(Value) => Params.push((Decl.Name.clone(), ScriptValue::Vec2(*Value))),
            ParamValue::Text(Value) => Params.push((Decl.Name.clone(), ScriptValue::Text(Value.clone()))),
            ParamValue::Image(Some(Id)) => {
                if let Some(Position) = App.Project.image_index(*Id) {
                    Inputs.push((Decl.Name.clone(), Position));
                }
            }
            ParamValue::Image(None) => {}
        }
    }
    let Timeout = if Script.TimeoutSeconds > 0 { Script.TimeoutSeconds } else { App.Settings.ScriptTimeoutSeconds };
    ScriptJob {
        Source: Script.Source.clone(),
        Interpreter,
        ProjectName: App.Project.Name.clone(),
        Images,
        Params,
        Inputs,
        RequestedMode: to_output_mode(Script.Mode),
        Timeout: Some(std::time::Duration::from_secs(Timeout as u64)),
    }
}

pub fn output_images(Output: &despite_script::ScriptOutput) -> Vec<(String, Vec<Frame>)> {
    Output
        .Images
        .iter()
        .map(|Generated| {
            let Frames = Generated
                .Frames
                .iter()
                .filter_map(|Entry| {
                    Raster::from_rgba(Entry.Pixels.Width, Entry.Pixels.Height, Entry.Pixels.Rgba.clone())
                        .ok()
                        .map(|Pixels| Frame { Name: Generated.Name.clone(), Pixels, DelayMs: Entry.DelayMs.max(10) })
                })
                .collect();
            (Generated.Name.clone(), Frames)
        })
        .collect()
}

pub fn export_frames(Owner: &Project, Output: &despite_script::ScriptOutput) -> Vec<Frame> {
    let Images = output_images(Output);
    let Mode = to_compile_mode(Output.Mode);
    if Mode == CompileMode::Scene && Output.Scene.is_some() {
        return build_preview(Owner, Mode, &Images, Output.Scene.as_ref());
    }
    Images.into_iter().flat_map(|(_, Frames)| Frames).collect()
}

pub fn poll_script(App: &mut DespiteApp, Context: &egui::Context) {
    let Some(Running) = &mut App.Desktop.Running else { return };
    let Events = Running.Handle.poll_events();
    let Outcome = Running.Handle.try_result();
    let Name = Running.Name.clone();
    for Event in Events {
        let Error = Event.is_stderr();
        App.Desktop.log(Error, Event.text().to_string());
    }
    match Outcome {
        None => Context.request_repaint_after(std::time::Duration::from_millis(50)),
        Some(Ok(Output)) => {
            App.Desktop.Running = None;
            App.Desktop.log(false, format!("✔ {Name} finished in {:.2}s ({} image(s){})", Output.Elapsed.as_secs_f32(), Output.Images.len(), if Output.Scene.is_some() { ", 1 scene" } else { "" }));
            let Images = output_images(&Output);
            if Images.is_empty() && Output.Scene.is_none() {
                App.Desktop.log(true, "The script produced no output. Use despite.output().add_image(...) or .scene(...).");
                App.Desktop.Tab = CodeTab::Console;
                return;
            }
            let Mode = to_compile_mode(Output.Mode);
            set_result(App, Name, Mode, Images, Output.Scene);
        }
        Some(Err(Problem)) => {
            App.Desktop.Running = None;
            App.Desktop.ErrorLine = Problem.script_line();
            if let Some(Text) = Problem.stderr() {
                for Line in Text.lines().rev().take(40).collect::<Vec<_>>().into_iter().rev() {
                    if !App.Desktop.Console.iter().rev().take(60).any(|Existing| Existing.Text == Line) {
                        App.Desktop.log(true, Line.to_string());
                    }
                }
            }
            App.Desktop.log(true, format!("✖ {Name}: {}", Problem.summary()));
            App.Desktop.Tab = CodeTab::Console;
            App.ShowBottom = true;
        }
    }
}

fn set_result(App: &mut DespiteApp, Title: String, Mode: CompileMode, Images: Vec<(String, Vec<Frame>)>, Scene: Option<ScriptScene>) {
    let Preview = build_preview(&App.Project, Mode, &Images, Scene.as_ref());
    App.Desktop.Compiled = Some(CompiledResult { Title, Mode, Images, Scene, Preview, Textures: Vec::new(), Current: 0, Clock: App.Now, Playing: true, Zoom: 0.0 });
    App.Desktop.ShowPreview = true;
}

fn all_frames(Images: &[(String, Vec<Frame>)]) -> Vec<Frame> {
    if let Some((_, Frames)) = Images.iter().find(|(_, Frames)| Frames.len() > 1) {
        return Frames.clone();
    }
    Images.iter().filter_map(|(_, Frames)| Frames.first().cloned()).collect()
}

fn build_preview(Owner: &Project, Mode: CompileMode, Images: &[(String, Vec<Frame>)], Scene: Option<&ScriptScene>) -> Vec<Frame> {
    match Mode {
        CompileMode::Scene if Scene.is_some() => {
            let mut Temporary = Owner.clone();
            let (_, Created) = materialize(&mut Temporary, Images, Scene);
            match Created {
                Some(Id) => export::collect_frames(&Temporary, &FrameSource::Scene(Id)),
                None => all_frames(Images),
            }
        }
        CompileMode::SpriteSheet => {
            let Frames: Vec<Frame> = Images.iter().flat_map(|(_, Frames)| Frames.iter().cloned()).collect();
            if Frames.is_empty() {
                return Vec::new();
            }
            let Sheet = export::build_sprite_sheet(&Frames, &SheetOptions::default());
            vec![Frame { Name: "Sprite sheet".into(), Pixels: Sheet.Pixels, DelayMs: 100 }]
        }
        CompileMode::Image => Images.iter().filter_map(|(_, Frames)| Frames.first().cloned()).take(1).collect(),
        _ => all_frames(Images),
    }
}

pub fn materialize(Target: &mut Project, Images: &[(String, Vec<Frame>)], Scene: Option<&ScriptScene>) -> (Vec<ImageId>, Option<SceneId>) {
    let mut Created = Vec::new();
    let mut ByName: HashMap<String, (ImageId, bool)> = HashMap::new();
    for (Name, Frames) in Images {
        let Some(First) = Frames.first() else { continue };
        let Id = Target.add_image_from_raster(Name, First.Pixels.clone());
        if let Some(Found) = Target.image_mut(Id) {
            Found.FrameDelayMs = First.DelayMs;
            Found.Layers[0].Name = "Frame 1".into();
        }
        for (Index, Entry) in Frames.iter().enumerate().skip(1) {
            let LayerId = Target.alloc_layer_id();
            if let Some(Found) = Target.image_mut(Id) {
                let mut Next = Layer::from_raster(LayerId, format!("Frame {}", Index + 1), Entry.Pixels.clone());
                Next.Visible = true;
                Found.Layers.push(Next);
                Found.ActiveLayer = 0;
            }
        }
        ByName.insert(Name.clone(), (Id, Frames.len() > 1));
        Created.push(Id);
    }
    let Some(Spec) = Scene else { return (Created, None) };
    let SceneRef = Target.add_scene(&Spec.Name, Spec.Width.max(1), Spec.Height.max(1));
    let mut Items = Vec::new();
    for Item in &Spec.Items {
        let Resolved = match &Item.Source {
            SceneSource::Generated(Name) => ByName.get(Name).copied(),
            SceneSource::Project(Name) => Target.image_by_name(Name).map(|Found| (Found.Id, false)),
        };
        let Some((Source, Animated)) = Resolved else { continue };
        let Some(Found) = Target.image(Source) else { continue };
        let (Width, Height) = (Found.Width as f32, Found.Height as f32);
        let Id = Target.alloc_id();
        Items.push(SceneItem {
            Id,
            Name: Item.Source.name().to_string(),
            Source: Some(Source),
            Transform: LayerTransform {
                OffsetX: Item.X - Width * 0.5 * (1.0 - Item.ScaleX),
                OffsetY: Item.Y - Height * 0.5 * (1.0 - Item.ScaleY),
                ScaleX: Item.ScaleX.abs().max(0.001),
                ScaleY: Item.ScaleY.abs().max(0.001),
                Rotation: Item.Rotation,
                FlipX: Item.ScaleX < 0.0,
                FlipY: Item.ScaleY < 0.0,
            },
            Opacity: Item.Opacity,
            Blend: BlendMode::from_name(&Item.Blend).unwrap_or_default(),
            Visible: Item.Visible,
            Playback: if Animated { Playback::LayersAsFrames } else { Playback::Composite },
            FrameOffset: Item.FrameOffset,
            ..Default::default()
        });
    }
    let Background = Spec.Background.map(despite_core::Color::from_array);
    let Fps = Spec.Fps;
    let Explicit = Spec.FrameCount;
    if let Some(Found) = Target.scene_mut(SceneRef) {
        Found.Items = Items;
        Found.Background = Background;
        Found.Fps = if Fps > 0.0 { Fps } else { 12.0 };
    }
    let Count = Explicit.unwrap_or_else(|| Target.scene(SceneRef).map(|Found| Found.auto_frame_count(Target)).unwrap_or(1));
    if let Some(Found) = Target.scene_mut(SceneRef) {
        Found.FrameCount = Count.max(1);
    }
    (Created, Some(SceneRef))
}

fn shader_value(Value: &ParamValue) -> Option<ShaderValue> {
    Some(match Value {
        ParamValue::Float(Number) => ShaderValue::Float(*Number),
        ParamValue::Int(Number) => ShaderValue::Int(*Number),
        ParamValue::Bool(Flag) => ShaderValue::Bool(*Flag),
        ParamValue::Color(Color) => ShaderValue::Color(Color.to_array()),
        ParamValue::Vec2(Pair) => ShaderValue::Vec2(*Pair),
        ParamValue::Text(_) | ParamValue::Image(_) => return None,
    })
}

pub fn shader_image(Source: &Raster) -> ShaderImage {
    ShaderImage::new(Source.width(), Source.height(), Source.data().to_vec())
}

pub fn shader_params(Shader: &ProjectShader, Owner: &Project) -> (Vec<ShaderParam>, Vec<ShaderInput>) {
    let mut Params = Vec::new();
    let mut Inputs = Vec::new();
    for Decl in &Shader.Params {
        if let ParamValue::Image(Chosen) = &Decl.Value {
            let Pixels = Chosen.and_then(|Id| Owner.image(Id)).map(|Found| Found.composite()).unwrap_or_else(|| Raster::new(1, 1));
            Inputs.push(ShaderInput::new(Decl.Name.clone(), shader_image(&Pixels)));
        } else if let Some(Value) = shader_value(&Decl.Value) {
            Params.push(ShaderParam::new(Decl.Name.clone(), Value));
        }
    }
    (Params, Inputs)
}

fn ensure_worker(App: &mut DespiteApp, Context: &egui::Context) {
    if App.Desktop.Worker.is_some() {
        return;
    }
    let (JobSender, JobReceiver) = mpsc::channel::<ShaderJob>();
    let (ResultSender, ResultReceiver) = mpsc::channel();
    let Adapter = Arc::new(Mutex::new(String::new()));
    let AdapterName = Adapter.clone();
    let Repaint = Context.clone();
    std::thread::Builder::new()
        .name("despite-shader".into())
        .spawn(move || {
            let mut Runner: Option<despite_shader::ShaderRunner> = None;
            for Job in JobReceiver {
                if Runner.is_none() {
                    match despite_shader::ShaderRunner::new_headless() {
                        Ok(Created) => {
                            if let Ok(mut Name) = AdapterName.lock() {
                                *Name = Created.adapter_name();
                            }
                            Runner = Some(Created);
                        }
                        Err(Problem) => {
                            let _ = ResultSender.send(Err(Problem));
                            Repaint.request_repaint();
                            continue;
                        }
                    }
                }
                let Result = Runner.as_mut().expect("runner created above").run(&Job);
                if ResultSender.send(Result).is_err() {
                    break;
                }
                Repaint.request_repaint();
            }
        })
        .expect("spawn shader thread");
    App.Desktop.Worker = Some(ShaderWorker { Jobs: JobSender, Results: ResultReceiver, Adapter });
}

pub fn run_shader(App: &mut DespiteApp) {
    if App.Desktop.ShaderPending.is_some() {
        return;
    }
    let Some(Shader) = App.Project.Shaders.get(App.Desktop.ActiveShader).cloned() else {
        App.set_status("No shader to run");
        return;
    };
    let Source = if crate::animator::is_animation(App) {
        let Sprites = despite_core::animation::SpriteCache::build(&App.Project);
        Some(despite_core::animation::render_frame(&App.Project, Some(crate::animator::clip_index(App)), App.Anim.Frame, &Sprites, None))
    } else {
        match Shader.Target {
            ShaderTarget::ActiveDocument => {
                let Frame = App.Project.Active.and_then(|Document| App.Views.get(&Document)).map(|View| View.Frame).unwrap_or(0);
                App.Project.Active.and_then(|Document| App.Project.render_document(Document, Frame))
            }
            ShaderTarget::Image(Id) => App.Project.render_document(DocumentRef::Image(Id), 0),
            ShaderTarget::Scene(Id) => App.Project.render_document(DocumentRef::Scene(Id), 0),
        }
    };
    let Some(Main) = Source else {
        App.set_status("The shader input image no longer exists");
        return;
    };
    let (Params, Inputs) = shader_params(&Shader, &App.Project);
    let FrameCount = Shader.FrameCount.clamp(1, App.Settings.ShaderMaxFrames.max(1));
    let Job = ShaderJob {
        Source: Shader.Source.clone(),
        Main: shader_image(&Main),
        OutputSize: (Shader.OutputWidth > 0 && Shader.OutputHeight > 0).then_some((Shader.OutputWidth, Shader.OutputHeight)),
        Inputs,
        Params,
        FrameCount,
        Fps: Shader.Fps.max(0.1),
        SmoothSampling: Shader.Smooth,
        OnlyFrame: None,
    };
    if let Err(Problem) = despite_shader::validate(&Job) {
        App.Desktop.log(true, format!("✖ {}: {Problem}", Shader.Name));
        App.Desktop.ShaderStatus = Some(Err(Problem));
        return;
    }
    App.Desktop.ShaderStatus = None;
    App.Desktop.ShaderPending = Some((Shader.Name.clone(), Shader.Mode, FrameCount, Shader.Fps));
    App.Desktop.PendingJob = Some(Job);
}

pub fn poll_shader(App: &mut DespiteApp, Context: &egui::Context) {
    if let Some(Job) = App.Desktop.PendingJob.take() {
        ensure_worker(App, Context);
        if let Some(Worker) = &App.Desktop.Worker
            && Worker.Jobs.send(Job).is_err() {
                App.Desktop.Worker = None;
                App.Desktop.ShaderPending = None;
                App.Desktop.log(true, "The shader worker stopped; try again");
            }
    }
    let Some(Worker) = &App.Desktop.Worker else { return };
    let Ok(Outcome) = Worker.Results.try_recv() else { return };
    let Some((Name, Mode, _, Fps)) = App.Desktop.ShaderPending.take() else { return };
    match Outcome {
        Ok(Output) => {
            let Delay = (1000.0 / Fps.max(0.1)).round() as u32;
            let Frames: Vec<Frame> = Output
                .Frames
                .into_iter()
                .enumerate()
                .filter_map(|(Index, Image)| Raster::from_rgba(Image.Width, Image.Height, Image.Rgba).ok().map(|Pixels| Frame { Name: format!("{Name} {Index}"), Pixels, DelayMs: Delay }))
                .collect();
            App.Desktop.log(false, format!("✔ {Name}: rendered {} frame(s)", Frames.len()));
            App.Desktop.ShaderStatus = Some(Ok(format!("Rendered {} frame(s)", Frames.len())));
            set_result(App, Name.clone(), Mode, vec![(Name, Frames)], None);
        }
        Err(Problem) => {
            App.Desktop.log(true, format!("✖ {Name}: {Problem}"));
            App.Desktop.ShaderStatus = Some(Err(Problem));
        }
    }
}

pub fn start_video_export(App: &mut DespiteApp, Frames: Vec<Frame>, Target: PathBuf, Options: despite_video::VideoOptions) {
    if App.Desktop.Video.is_some() {
        App.set_status("A video export is already running");
        return;
    }
    let Progress = Arc::new(Mutex::new(0.0f32));
    let Cancel = super::cancel_flag();
    let (ProgressWriter, CancelReader, Path) = (Progress.clone(), Cancel.clone(), Target.clone());
    let Handle = std::thread::Builder::new()
        .name("despite-video".into())
        .spawn(move || {
            despite_video::encode_video(&Frames, &Path, &Options, Some(CancelReader), |Value| {
                if let Ok(mut Slot) = ProgressWriter.lock() {
                    *Slot = Value;
                }
            })
            .map_err(|Problem| Problem.to_string())
        })
        .ok();
    App.Desktop.Video = Some(VideoJob { Target, Progress, Cancel, Handle });
}

pub fn poll_video(App: &mut DespiteApp, Context: &egui::Context) {
    let Finished = App.Desktop.Video.as_ref().is_some_and(|Job| Job.Handle.as_ref().is_none_or(|Handle| Handle.is_finished()));
    if !Finished {
        if App.Desktop.Video.is_some() {
            Context.request_repaint_after(std::time::Duration::from_millis(100));
        }
        return;
    }
    let Some(mut Job) = App.Desktop.Video.take() else { return };
    let Outcome = Job.Handle.take().map(|Handle| Handle.join().unwrap_or_else(|_| Err("video thread crashed".into()))).unwrap_or(Err("could not start video thread".into()));
    match Outcome {
        Ok(()) => App.set_status(format!("Video saved to {}", Job.Target.display())),
        Err(Problem) => {
            App.Desktop.log(true, format!("Video export failed: {Problem}"));
            App.Dialog = Some(crate::dialogs::Dialog::Message("Video export failed".into(), Problem));
        }
    }
}

fn export_result(App: &mut DespiteApp, Format: &str) {
    let Some(Result) = &App.Desktop.Compiled else { return };
    let Name = despite_core::export::sanitize_file_name(&Result.Title);
    let Frames = if Result.Preview.is_empty() { all_frames(&Result.Images) } else { Result.Preview.clone() };
    let Outcome: Result<String, String> = match Format {
        "png" => match crate::platform::dialogs::save_file("PNG image", "png", &Name) {
            Some(Path) => {
                let Pixels = Frames.get(Result.Current.min(Frames.len().saturating_sub(1))).map(|Entry| Entry.Pixels.clone()).unwrap_or_default();
                Pixels.save_png(&Path).map(|()| format!("Saved {}", Path.display())).map_err(|Problem| Problem.to_string())
            }
            None => return,
        },
        "sheet" => match crate::platform::dialogs::save_file("Sprite sheet", "png", &Name) {
            Some(Path) => {
                let Sheet = export::build_sprite_sheet(&Frames, &App.Settings.Sheet);
                let ImageName = Path.file_name().and_then(|Value| Value.to_str()).unwrap_or("sheet.png").to_string();
                Sheet
                    .Pixels
                    .save_png(&Path)
                    .map_err(|Problem| Problem.to_string())
                    .and_then(|()| std::fs::write(Path.with_extension("json"), export::sprite_sheet_json(&Sheet, &ImageName)).map_err(|Problem| Problem.to_string()))
                    .map(|()| format!("Saved {}", Path.display()))
            }
            None => return,
        },
        "gif" => match crate::platform::dialogs::save_file("GIF", "gif", &Name) {
            Some(Path) => export::encode_gif(&Frames, &App.Settings.Gif)
                .map_err(|Problem| Problem.to_string())
                .and_then(|Bytes| std::fs::write(&Path, Bytes).map_err(|Problem| Problem.to_string()))
                .map(|()| format!("Saved {}", Path.display())),
            None => return,
        },
        "sequence" => match crate::platform::dialogs::pick_folder() {
            Some(Folder) => export::write_png_sequence(&Frames, &Folder, &Name).map(|Written| format!("Saved {} frames", Written.len())).map_err(|Problem| Problem.to_string()),
            None => return,
        },
        _ => {
            let Options = despite_video::VideoOptions {
                Fps: despite_video::fps_from_frames(&Frames),
                Quality: App.Settings.VideoQuality,
                Ffmpeg: App.Settings.FfmpegPath.clone(),
                ..Default::default()
            };
            match crate::platform::dialogs::save_file("MP4 video", Options.Format.extension(), &Name) {
                Some(Path) => {
                    start_video_export(App, Frames, Path.clone(), Options);
                    Ok(format!("Encoding {}…", Path.display()))
                }
                None => return,
            }
        }
    };
    match Outcome {
        Ok(Message) => App.set_status(Message),
        Err(Problem) => App.set_status(format!("Export failed: {Problem}")),
    }
}

pub fn windows(App: &mut DespiteApp, Context: &egui::Context) {
    super::code::shortcuts(App, Context);
    preview_window(App, Context);
    super::plugins_window(App, Context);
    if App.Desktop.AskTrust {
        let mut Open = true;
        crate::dialogs::modal_window("Run project code?", Context, &mut Open, App.Compact).show(Context, |Ui| {
            Ui.label("This project was opened from a file and contains Python code.");
            Ui.label("Python code can read, change or delete your files and use the network. Only run code from people you trust.");
            Ui.horizontal(|Ui| {
                if Ui.button("Trust and run").clicked() {
                    App.Desktop.Trusted = true;
                    App.Desktop.AskTrust = false;
                    run_script(App);
                }
                if Ui.button("Cancel").clicked() {
                    App.Desktop.AskTrust = false;
                }
            });
        });
        if !Open {
            App.Desktop.AskTrust = false;
        }
    }
    if let Some(Job) = &App.Desktop.Video {
        let Progress = Job.Progress.lock().map(|Value| *Value).unwrap_or(0.0);
        let Target = Job.Target.display().to_string();
        let Cancel = Job.Cancel.clone();
        let mut Open = true;
        egui::Window::new("Exporting video").open(&mut Open).collapsible(false).show(Context, |Ui| {
            Ui.label(Target);
            Ui.add(egui::ProgressBar::new(Progress).show_percentage());
            if Ui.button("Cancel").clicked() {
                Cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
        if !Open {
            Cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

fn preview_window(App: &mut DespiteApp, Context: &egui::Context) {
    if !App.Desktop.ShowPreview {
        return;
    }
    let Now = App.Now;
    let PreviewFps = App.Settings.PreviewFps.max(1.0) as f64;
    let Some(Result) = &mut App.Desktop.Compiled else {
        App.Desktop.ShowPreview = false;
        return;
    };
    if Result.Textures.len() != Result.Preview.len() {
        Result.Textures = Result
            .Preview
            .iter()
            .enumerate()
            .map(|(Index, Entry)| crate::view::raster_texture(Context, &format!("compiled-{Index}"), &Entry.Pixels, false))
            .collect();
    }
    let Count = Result.Textures.len();
    if Result.Playing && Count > 1 {
        let Delay = Result.Preview.get(Result.Current).map(|Entry| Entry.DelayMs).unwrap_or(100).max(1) as f64 / 1000.0;
        let Delay = Delay.max(1.0 / PreviewFps);
        if Now - Result.Clock >= Delay {
            Result.Clock = Now;
            Result.Current = (Result.Current + 1) % Count;
        }
        Context.request_repaint_after(std::time::Duration::from_secs_f64(Delay));
    }
    let mut Open = true;
    let mut Action: Option<&'static str> = None;
    egui::Window::new(format!("Compiled: {}", Result.Title)).open(&mut Open).default_size([560.0, 520.0]).resizable(true).show(Context, |Ui| {
        Ui.horizontal(|Ui| {
            Ui.label(RichText::new(Result.Mode.name()).strong());
            Ui.label(format!("{} frame(s)", Count));
            if let Some(First) = Result.Preview.first() {
                Ui.label(format!("{}×{}", First.Pixels.width(), First.Pixels.height()));
            }
            if Count > 1 {
                if crate::icons::button(Ui, if Result.Playing { "Pause" } else { "Play" }, false, "Play / pause").clicked() {
                    Result.Playing = !Result.Playing;
                }
                let mut Current = Result.Current;
                if Ui.add(egui::Slider::new(&mut Current, 0..=Count - 1)).changed() {
                    Result.Current = Current;
                    Result.Playing = false;
                }
            }
            Ui.add(egui::Slider::new(&mut Result.Zoom, 0.0..=16.0).text("zoom (0 = fit)"));
        });
        let Available = Ui.available_size() - egui::vec2(0.0, 70.0);
        if let Some(Texture) = Result.Textures.get(Result.Current) {
            let Size = Texture.size_vec2();
            let Scale = if Result.Zoom > 0.0 { Result.Zoom } else { (Available.x / Size.x).min(Available.y / Size.y).max(0.05) };
            let Shown = Size * Scale;
            egui::ScrollArea::both().max_height(Available.y.max(100.0)).show(Ui, |Ui| {
                let (Rect, _) = Ui.allocate_exact_size(Shown, egui::Sense::hover());
                let Cell = 8.0;
                for Row in 0..(Rect.height() / Cell).ceil() as i32 {
                    for Column in 0..(Rect.width() / Cell).ceil() as i32 {
                        let Tile = egui::Rect::from_min_size(Rect.min + egui::vec2(Column as f32 * Cell, Row as f32 * Cell), egui::vec2(Cell, Cell)).intersect(Rect);
                        let Shade = if (Row + Column) % 2 == 0 { 200 } else { 160 };
                        Ui.painter().rect_filled(Tile, 0.0, egui::Color32::from_gray(Shade));
                    }
                }
                Ui.painter().image(Texture.id(), Rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            });
        }
        Ui.separator();
        Ui.horizontal_wrapped(|Ui| {
            Ui.label("Add to project:");
            if Ui.button("As new image(s)").clicked() {
                Action = Some("images");
            }
            if Result.Scene.is_some() && Ui.button("Images + scene").clicked() {
                Action = Some("scene");
            }
            if Ui.button("As layers in current image").clicked() {
                Action = Some("layers");
            }
            if Ui.button("Replace active layer").on_hover_text("Uses the shown frame").clicked() {
                Action = Some("replace");
            }
        });
        Ui.horizontal_wrapped(|Ui| {
            Ui.label("Export:");
            for (Label, Key) in [("PNG", "png"), ("Sprite sheet", "sheet"), ("GIF", "gif"), ("PNG sequence", "sequence"), ("Video", "video")] {
                if Ui.button(Label).clicked() {
                    Action = Some(Key);
                }
            }
        });
    });
    if !Open {
        App.Desktop.ShowPreview = false;
    }
    let Some(Chosen) = Action else { return };
    match Chosen {
        "images" | "scene" => {
            let Some(Result) = App.Desktop.Compiled.take() else { return };
            App.record("Add compiled result");
            let Scene = if Chosen == "scene" { Result.Scene.as_ref() } else { None };
            let (Created, SceneRef) = materialize(&mut App.Project, &Result.Images, Scene);
            if let Some(Id) = SceneRef {
                App.activate(DocumentRef::Scene(Id));
            } else if let Some(Id) = Created.first() {
                App.activate(DocumentRef::Image(*Id));
            }
            App.Desktop.Compiled = Some(Result);
            App.touch();
            App.set_status(format!("Added {} image(s)", Created.len()));
        }
        "layers" => {
            let Some(Id) = App.active_image_id() else {
                App.set_status("Open an image first");
                return;
            };
            let Layers: Vec<(String, Raster)> = App
                .Desktop
                .Compiled
                .as_ref()
                .map(|Result| Result.Images.iter().flat_map(|(Name, Frames)| Frames.iter().map(move |Entry| (Name.clone(), Entry.Pixels.clone()))).collect())
                .unwrap_or_default();
            App.record("Add compiled layers");
            for (Name, Pixels) in Layers {
                App.Project.add_layer_from_raster(Id, &Name, Pixels);
            }
            App.touch();
        }
        "replace" => {
            let Pixels = App.Desktop.Compiled.as_ref().and_then(|Result| Result.Preview.get(Result.Current).or(Result.Preview.first()).map(|Entry| Entry.Pixels.clone()));
            let Some(Pixels) = Pixels else { return };
            if App.active_layer_locked() {
                App.set_status("The active layer is locked");
                return;
            }
            App.with_active_layer("Replace layer with result", |Target| {
                let Matches = Pixels.size() == (Target.Width, Target.Height);
                if let Some(Entry) = Target.active_layer_mut() {
                    Entry.Pixels = Arc::new(Pixels);
                    if Matches {
                        Entry.Transform = LayerTransform::default();
                    }
                }
            });
        }
        Format => export_result(App, Format),
    }
}
