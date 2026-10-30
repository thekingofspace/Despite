use despite_core::export::{self, Frame, FrameSource};
use despite_core::{DocumentRef, ProjectShader, ShaderTarget};
use despite_shader::{ShaderJob, ShaderRunner};
use egui::{RichText, Ui};

use super::compile;
use crate::app::DespiteApp;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodeSource {
    #[default]
    Document,
    Script(u64),
}

pub fn shader_applies(Target: ShaderTarget, Document: Option<DocumentRef>) -> bool {
    match (Target, Document) {
        (ShaderTarget::ActiveDocument, _) => true,
        (ShaderTarget::Image(Id), Some(DocumentRef::Image(Other))) => Id == Other,
        (ShaderTarget::Scene(Id), Some(DocumentRef::Scene(Other))) => Id == Other,
        _ => false,
    }
}

fn shaders_for(App: &DespiteApp, Document: Option<DocumentRef>) -> Vec<ProjectShader> {
    App.Project
        .Shaders
        .iter()
        .filter(|Shader| Shader.ApplyOnExport && !Shader.Source.trim().is_empty() && shader_applies(Shader.Target, Document))
        .cloned()
        .collect()
}

fn runner(App: &mut DespiteApp) -> Result<&mut ShaderRunner, String> {
    if App.Desktop.ExportRunner.is_none() {
        let Created = ShaderRunner::new_headless().map_err(|Problem| format!("Could not start the GPU for shaders: {Problem}"))?;
        App.Desktop.ExportRunner = Some(Created);
    }
    Ok(App.Desktop.ExportRunner.as_mut().expect("runner created above"))
}

fn run_job(App: &mut DespiteApp, Shader: &ProjectShader, Job: &ShaderJob) -> Result<Vec<despite_core::Raster>, String> {
    despite_shader::validate(Job).map_err(|Problem| format!("Shader \"{}\": {Problem}", Shader.Name))?;
    let Output = runner(App)?.run(Job).map_err(|Problem| format!("Shader \"{}\": {Problem}", Shader.Name))?;
    Output
        .Frames
        .into_iter()
        .map(|Image| despite_core::Raster::from_rgba(Image.Width, Image.Height, Image.Rgba).map_err(|Problem| Problem.to_string()))
        .collect()
}

fn apply_one(App: &mut DespiteApp, Shader: &ProjectShader, Frames: Vec<Frame>, Sequence: bool) -> Result<Vec<Frame>, String> {
    let (Params, Inputs) = compile::shader_params(Shader, &App.Project);
    let OutputSize = (Shader.OutputWidth > 0 && Shader.OutputHeight > 0).then_some((Shader.OutputWidth, Shader.OutputHeight));
    let MaxFrames = App.Settings.ShaderMaxFrames.max(1);
    let Template = |Main: &despite_core::Raster, FrameCount: u32, Fps: f32, OnlyFrame: Option<u32>| ShaderJob {
        Source: Shader.Source.clone(),
        Main: compile::shader_image(Main),
        OutputSize,
        Inputs: Inputs.clone(),
        Params: Params.clone(),
        FrameCount,
        Fps,
        SmoothSampling: Shader.Smooth,
        OnlyFrame,
    };
    let mut Out = Vec::new();
    if Sequence && Frames.len() > 1 {
        let Count = Frames.len() as u32;
        let Fps = 1000.0 / Frames[0].DelayMs.max(1) as f32;
        for (Index, Entry) in Frames.into_iter().enumerate() {
            let Job = Template(&Entry.Pixels, Count, Fps, Some(Index as u32));
            if let Some(Pixels) = run_job(App, Shader, &Job)?.into_iter().next() {
                Out.push(Frame { Name: Entry.Name, Pixels, DelayMs: Entry.DelayMs });
            }
        }
        return Ok(Out);
    }
    for Entry in Frames {
        let Count = Shader.FrameCount.clamp(1, MaxFrames);
        let Fps = Shader.Fps.max(0.1);
        let Job = Template(&Entry.Pixels, Count, Fps, None);
        let Rendered = run_job(App, Shader, &Job)?;
        let Total = Rendered.len();
        let Delay = if Total > 1 { (1000.0 / Fps).round() as u32 } else { Entry.DelayMs };
        for (Index, Pixels) in Rendered.into_iter().enumerate() {
            let Name = if Total > 1 { format!("{} {}", Entry.Name, Index + 1) } else { Entry.Name.clone() };
            Out.push(Frame { Name, Pixels, DelayMs: Delay });
        }
    }
    Ok(Out)
}

pub fn apply_shaders(App: &mut DespiteApp, Document: Option<DocumentRef>, Frames: Vec<Frame>, Sequence: bool) -> Result<Vec<Frame>, String> {
    let mut Current = Frames;
    for Shader in shaders_for(App, Document) {
        if Current.is_empty() {
            break;
        }
        Current = apply_one(App, &Shader, Current, Sequence)?;
    }
    Ok(Current)
}

pub fn apply_to_all_images(App: &mut DespiteApp, Frames: Vec<Frame>) -> Result<Vec<Frame>, String> {
    let Ids: Vec<_> = App.Project.Images.iter().map(|Found| Found.Id).collect();
    let mut Out = Vec::new();
    for (Entry, Id) in Frames.into_iter().zip(Ids) {
        Out.extend(apply_shaders(App, Some(DocumentRef::Image(Id)), vec![Entry], false)?);
    }
    Ok(Out)
}

pub fn script_frames(App: &mut DespiteApp, Id: u64) -> Result<Vec<Frame>, String> {
    let Script = App.Project.Scripts.iter().find(|Found| Found.Id == Id).cloned().ok_or("The chosen script no longer exists")?;
    if !App.Desktop.Trusted {
        return Err("Trust this project's code first (button in the export box)".into());
    }
    let Interpreter = super::interpreter(App).ok_or("Python 3 was not found. Install it or set its path in Settings → Python.")?;
    let Job = compile::script_job(App, &Script, Interpreter);
    App.Desktop.log(false, format!("▶ Running {} for export …", Script.Name));
    match despite_script::run_blocking(Job) {
        Ok(Output) => {
            let Frames = compile::export_frames(&App.Project, &Output);
            App.Desktop.log(false, format!("✔ {} produced {} frame(s) for export", Script.Name, Frames.len()));
            if Frames.is_empty() {
                return Err(format!("\"{}\" produced no images", Script.Name));
            }
            let Frames = apply_shaders(App, None, Frames, true)?;
            Ok(Frames)
        }
        Err(Problem) => {
            App.Desktop.ErrorLine = Problem.script_line();
            App.Desktop.log(true, format!("✖ {}: {}", Script.Name, Problem.summary()));
            Err(format!("Script \"{}\" failed: {}", Script.Name, Problem.summary()))
        }
    }
}

fn rig_frames(App: &mut DespiteApp, Source: &FrameSource) -> Vec<Frame> {
    let Owner = &App.Project;
    let mut Hook = RigShaderHook { Runner: &mut App.Desktop.ExportRunner, Owner, Failures: &mut App.Desktop.ShaderFailures };
    match Source {
        FrameSource::Animation { Clip } => despite_core::animation::clip_frames(Owner, *Clip, Some(&mut Hook)),
        FrameSource::AnimationFrame { Clip, Frame: Index } => {
            let Sprites = despite_core::animation::SpriteCache::build(Owner);
            let Pixels = despite_core::animation::render_frame(Owner, Some(*Clip), *Index as f32, &Sprites, Some(&mut Hook));
            let Delay = Owner.Rig.Clips.get(*Clip).map(|Found| (1000.0 / Found.Fps.max(0.1)).round() as u32).unwrap_or(100);
            vec![Frame { Name: format!("Frame {}", Index + 1), Pixels, DelayMs: Delay }]
        }
        _ => Vec::new(),
    }
}

pub fn document_frames(App: &mut DespiteApp, Source: &FrameSource) -> Result<Vec<Frame>, String> {
    if matches!(Source, FrameSource::Animation { .. } | FrameSource::AnimationFrame { .. }) {
        let Frames = rig_frames(App, Source);
        return apply_shaders(App, None, Frames, true);
    }
    let Frames = export::collect_frames(&App.Project, Source);
    match Source {
        FrameSource::AllImages => apply_to_all_images(App, Frames),
        FrameSource::Image(Id) => apply_shaders(App, Some(DocumentRef::Image(*Id)), Frames, true),
        FrameSource::Scene(Id) | FrameSource::SceneRange { Scene: Id, .. } => apply_shaders(App, Some(DocumentRef::Scene(*Id)), Frames, true),
        FrameSource::Layers { Image, .. } | FrameSource::SelectedLayers { Image, .. } => apply_shaders(App, Some(DocumentRef::Image(*Image)), Frames, false),
        _ => apply_shaders(App, App.Project.Active, Frames, true),
    }
}

pub fn export_section(App: &mut DespiteApp, Ui: &mut Ui, Source: &mut CodeSource, AllImages: bool) {
    Ui.separator();
    Ui.label(RichText::new("Project code (PC)").strong());
    let SourceName = |App: &DespiteApp, Choice: CodeSource| match Choice {
        CodeSource::Document => "The drawing (layers / scene)".to_string(),
        CodeSource::Script(Id) => App.Project.Scripts.iter().find(|Found| Found.Id == Id).map(|Found| format!("Python: {}", Found.Name)).unwrap_or_else(|| "Missing script".into()),
    };
    if let CodeSource::Script(Id) = *Source
        && !App.Project.Scripts.iter().any(|Found| Found.Id == Id)
    {
        *Source = CodeSource::Document;
    }
    Ui.horizontal(|Ui| {
        Ui.label("Frames from");
        egui::ComboBox::from_id_salt("ExportCodeSource").selected_text(SourceName(App, *Source)).width(240.0).show_ui(Ui, |Ui| {
            Ui.selectable_value(Source, CodeSource::Document, SourceName(App, CodeSource::Document));
            for Script in &App.Project.Scripts {
                Ui.selectable_value(Source, CodeSource::Script(Script.Id), format!("Python: {}", Script.Name)).on_hover_text("Runs the script and exports what it outputs");
            }
        });
    });
    if matches!(Source, CodeSource::Script(_)) && !App.Desktop.Trusted {
        Ui.horizontal_wrapped(|Ui| {
            Ui.colored_label(egui::Color32::from_rgb(230, 180, 80), "This project's Python code isn't trusted yet.");
            if Ui.button("Trust and allow").on_hover_text("Python code can read or change your files. Only trust code you know.").clicked() {
                App.Desktop.Trusted = true;
            }
        });
    }
    if App.Project.Shaders.is_empty() {
        Ui.label(RichText::new("No shaders yet. Add one in the code panel's Shader tab to apply effects to exports.").weak().small());
        return;
    }
    Ui.label("Shaders applied to every exported frame, top to bottom:");
    let Document = match Source {
        CodeSource::Script(_) => None,
        CodeSource::Document => App.Project.Active,
    };
    let mut Changed = false;
    for Shader in &mut App.Project.Shaders {
        let Applies = AllImages || shader_applies(Shader.Target, Document);
        Ui.horizontal_wrapped(|Ui| {
            Changed |= Ui.checkbox(&mut Shader.ApplyOnExport, &Shader.Name).changed();
            let Note = if !Applies {
                "targets another image, skipped here".to_string()
            } else if Shader.FrameCount > 1 {
                format!("animated, {} frames at {} fps for still images", Shader.FrameCount, Shader.Fps)
            } else {
                "single frame".to_string()
            };
            Ui.label(RichText::new(Note).weak().small());
        });
    }
    if Changed {
        App.Modified = true;
    }
}

pub fn numeric_params(Shader: &ProjectShader) -> Vec<&despite_core::ParamDecl> {
    Shader
        .Params
        .iter()
        .filter(|Decl| matches!(Decl.Value, despite_core::ParamValue::Float(_) | despite_core::ParamValue::Int(_) | despite_core::ParamValue::Bool(_)))
        .collect()
}

pub fn numeric_defaults(Shader: &ProjectShader) -> Vec<f32> {
    numeric_params(Shader)
        .into_iter()
        .map(|Decl| match Decl.Value {
            despite_core::ParamValue::Float(Value) => Value,
            despite_core::ParamValue::Int(Value) => Value as f32,
            despite_core::ParamValue::Bool(Value) => f32::from(Value),
            _ => 0.0,
        })
        .collect()
}

pub struct RigShaderHook<'a> {
    pub Runner: &'a mut Option<ShaderRunner>,
    pub Owner: &'a despite_core::Project,
    pub Failures: &'a mut std::collections::HashMap<u64, String>,
}

impl despite_core::animation::EffectHook for RigShaderHook<'_> {
    fn shader(&mut self, ShaderId: u64, _Node: despite_core::animation::NodeId, Values: &[f32], Input: &despite_core::Raster, Clock: despite_core::animation::Timing) -> Option<despite_core::Raster> {
        let Shader = self.Owner.Shaders.iter().find(|Found| Found.Id == ShaderId)?;
        if Shader.Source.trim().is_empty() {
            return None;
        }
        let (mut Params, Inputs) = compile::shader_params(Shader, self.Owner);
        for (Index, Decl) in numeric_params(Shader).into_iter().enumerate() {
            let Some(Value) = Values.get(Index).copied() else { continue };
            let Replacement = match Decl.Value {
                despite_core::ParamValue::Float(_) => despite_shader::ShaderValue::Float(Value),
                despite_core::ParamValue::Int(_) => despite_shader::ShaderValue::Int(Value.round() as i32),
                despite_core::ParamValue::Bool(_) => despite_shader::ShaderValue::Bool(Value >= 0.5),
                _ => continue,
            };
            if let Some(Existing) = Params.iter_mut().find(|Entry| Entry.Name == Decl.Name) {
                Existing.Value = Replacement;
            }
        }
        let Length = Clock.Length.max(1);
        let Job = ShaderJob {
            Source: Shader.Source.clone(),
            Main: compile::shader_image(Input),
            OutputSize: Some(Input.size()),
            Inputs,
            Params,
            FrameCount: Length,
            Fps: Clock.Fps.max(0.1),
            SmoothSampling: Shader.Smooth,
            OnlyFrame: Some((Clock.Frame.floor().max(0.0) as u32).min(Length - 1)),
        };
        if let Err(Problem) = despite_shader::validate(&Job) {
            self.Failures.insert(ShaderId, Problem.to_string());
            return None;
        }
        if self.Runner.is_none() {
            match ShaderRunner::new_headless() {
                Ok(Created) => *self.Runner = Some(Created),
                Err(Problem) => {
                    self.Failures.insert(ShaderId, Problem.to_string());
                    return None;
                }
            }
        }
        let Output = match self.Runner.as_mut()?.run(&Job) {
            Ok(Output) => Output,
            Err(Problem) => {
                self.Failures.insert(ShaderId, Problem.to_string());
                return None;
            }
        };
        self.Failures.remove(&ShaderId);
        let Frame = Output.Frames.into_iter().next()?;
        despite_core::Raster::from_rgba(Frame.Width, Frame.Height, Frame.Rgba).ok()
    }
}
