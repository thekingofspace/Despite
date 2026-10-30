use despite_core::{CompileMode, DocumentRef, ShaderTarget};
use egui::{RichText, Ui};

use super::{CodeTab, compile};
use crate::app::DespiteApp;
use crate::panels::params_editor;

fn notepad(Ui: &mut Ui, Text: &mut String, Id: &str, ErrorLine: Option<u32>) -> bool {
    let mut Changed = false;
    if let Some(Line) = ErrorLine {
        Ui.colored_label(egui::Color32::from_rgb(230, 110, 90), format!("Error on line {Line}"));
    }
    egui::ScrollArea::both().id_salt(Id).auto_shrink([false, false]).show(Ui, |Ui| {
        let Edit = egui::TextEdit::multiline(Text)
            .id_salt(Id)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY)
            .desired_rows(16)
            .lock_focus(true)
            .hint_text("Write your code in any text editor, then paste it here (Ctrl+V).");
        Changed = Ui.add(Edit).changed();
    });
    Changed
}

fn file_buttons(App: &mut DespiteApp, Ui: &mut Ui, Text: &mut String, Label: &str, Extension: &str, Name: &str) -> bool {
    let mut Changed = false;
    Ui.horizontal_wrapped(|Ui| {
        if Ui.button("Copy all").clicked() {
            Ui.ctx().copy_text(Text.clone());
            App.set_status("Code copied to clipboard");
        }
        if Ui.button("Load file…").clicked()
            && let Some(Path) = crate::platform::dialogs::open_file(Label, &[Extension, "txt"]) {
                match std::fs::read_to_string(&Path) {
                    Ok(Loaded) => {
                        *Text = Loaded;
                        Changed = true;
                    }
                    Err(Problem) => App.set_status(format!("Could not read {}: {Problem}", Path.display())),
                }
            }
        if Ui.button("Save file…").clicked()
            && let Some(Path) = crate::platform::dialogs::save_file(Label, Extension, Name)
                && let Err(Problem) = std::fs::write(&Path, Text.as_bytes()) {
                    App.set_status(format!("Could not write {}: {Problem}", Path.display()));
                }
        if Ui.button("Clear").clicked() {
            Text.clear();
            Changed = true;
        }
    });
    Changed
}

fn mode_combo(Ui: &mut Ui, Id: &str, Mode: &mut CompileMode, AllowScene: bool) -> bool {
    let mut Changed = false;
    egui::ComboBox::from_id_salt(Id).selected_text(Mode.name()).show_ui(Ui, |Ui| {
        for Candidate in CompileMode::All {
            if *Candidate == CompileMode::Scene && !AllowScene {
                continue;
            }
            Changed |= Ui.selectable_value(Mode, *Candidate, Candidate.name()).changed();
        }
    });
    Changed
}

pub fn bottom_panel(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.set_min_height(Ui.available_height());
    Ui.horizontal(|Ui| {
        crate::icons::selectable(Ui, &mut App.Desktop.Tab, CodeTab::Python, "Python", "Python project code");
        crate::icons::selectable(Ui, &mut App.Desktop.Tab, CodeTab::Shaders, "Shader", "Shaders / VFX");
        let Errors = App.Desktop.Console.iter().filter(|Line| Line.Error).count();
        let ConsoleLabel = if Errors > 0 { format!("Console ({Errors} errors)") } else { "Console".into() };
        crate::icons::selectable(Ui, &mut App.Desktop.Tab, CodeTab::Console, "Console", ConsoleLabel);
        Ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |Ui| {
            if crate::icons::button(Ui, "Close", false, "Hide code panel").clicked() {
                App.ShowBottom = false;
            }
            if App.Desktop.Compiled.is_some() && Ui.button("Show result").clicked() {
                App.Desktop.ShowPreview = true;
            }
        });
    });
    Ui.separator();
    match App.Desktop.Tab {
        CodeTab::Python => python_tab(App, Ui),
        CodeTab::Shaders => shader_tab(App, Ui),
        CodeTab::Console => console_tab(App, Ui),
    }
}

fn list_column<T>(Ui: &mut Ui, Items: &[T], Name: impl Fn(&T) -> String, Active: &mut usize, Noun: &str) -> (bool, Option<usize>) {
    let mut Add = false;
    let mut Remove = None;
    Ui.vertical(|Ui| {
        Ui.set_width(170.0);
        Add = Ui.button(format!("+ New {Noun}")).clicked();
        egui::ScrollArea::vertical().id_salt("CodeList").auto_shrink([false, false]).show(Ui, |Ui| {
            for (Index, Item) in Items.iter().enumerate() {
                Ui.horizontal(|Ui| {
                    let Label = egui::Button::selectable(*Active == Index, Name(Item)).min_size(egui::vec2(Ui.available_width() - 28.0, 0.0));
                    if Ui.add(Label).clicked() {
                        *Active = Index;
                    }
                    if crate::icons::button(Ui, "Delete", false, &format!("Delete {Noun}")).clicked() {
                        Remove = Some(Index);
                    }
                });
            }
        });
    });
    (Add, Remove)
}

fn python_tab(App: &mut DespiteApp, Ui: &mut Ui) {
    let Scripts = App.Project.Scripts.clone();
    let mut Active = App.Desktop.ActiveScript.min(Scripts.len().saturating_sub(1));
    let mut PendingAdd = false;
    let mut PendingRemove: Option<usize> = None;
    Ui.horizontal_top(|Ui| {
        let (Add, Remove) = list_column(Ui, &Scripts, |Script| Script.Name.clone(), &mut Active, "script");
        App.Desktop.ActiveScript = Active;
        PendingAdd = Add;
        PendingRemove = Remove;
        Ui.separator();
        let Some(mut Script) = App.Project.Scripts.get(Active).cloned() else {
            Ui.vertical(|Ui| {
                Ui.label("Project code lets Python generate images: draw an outline, then compile to see the final image, sprite sheet, GIF, video or scene.");
                if Ui.button("Create a script").clicked() {
                    super::add_script(App, "Script", despite_script::example_script("starter").unwrap_or(""), CompileMode::Image, Vec::new());
                }
            });
            return;
        };
        let Original = Script.clone();
        let SideWidth = 280.0;
        let EditorWidth = (Ui.available_width() - SideWidth - 16.0).max(200.0);
        Ui.vertical(|Ui| {
            Ui.set_width(EditorWidth);
            Ui.horizontal(|Ui| {
                Ui.label("Name");
                Ui.text_edit_singleline(&mut Script.Name);
            });
            let Name = Script.Name.clone();
            file_buttons(App, Ui, &mut Script.Source, "Python", "py", &Name);
            notepad(Ui, &mut Script.Source, "PythonNotepad", App.Desktop.ErrorLine);
        });
        Ui.separator();
        Ui.vertical(|Ui| {
            Ui.set_width(SideWidth);
            egui::ScrollArea::vertical().id_salt("PythonSide").show(Ui, |Ui| {
                Ui.horizontal(|Ui| {
                    Ui.label("Compile to");
                    mode_combo(Ui, "ScriptMode", &mut Script.Mode, true);
                });
                Ui.add(egui::Slider::new(&mut Script.TimeoutSeconds, 1..=3600).text("timeout s"));
                let Running = App.Desktop.Running.is_some();
                Ui.horizontal(|Ui| {
                    if Running {
                        if crate::icons::text_button(Ui, "Close", "Stop").clicked()
                            && let Some(Active) = &App.Desktop.Running {
                                Active.Handle.cancel();
                            }
                        Ui.spinner();
                        let Elapsed = App.Now - App.Desktop.Running.as_ref().map(|Active| Active.Started).unwrap_or(App.Now);
                        Ui.label(format!("{Elapsed:.1}s"));
                    } else if crate::icons::text_button(Ui, "Play", RichText::new("Compile (F5)").strong()).clicked() {
                        App.Desktop.RunRequested = true;
                    }
                });
                match super::interpreter(App) {
                    Some(Path) => Ui.label(RichText::new(format!("Python: {}", Path.display())).weak().small()),
                    None => Ui.colored_label(egui::Color32::from_rgb(230, 110, 90), "Python 3 not found. Install it or set the path in Settings."),
                };
                Ui.separator();
                Ui.label(RichText::new("Parameters").strong());
                Ui.label(RichText::new("Read them in Python with despite.params()[\"Name\"]; Image parameters arrive in despite.inputs().").weak().small());
                params_editor(Ui, &mut Script.Params, &App.Project, true, "ScriptParams");
            });
        });
        if Script != Original {
            App.record_coalesced(&format!("script{}", Script.Id), "Edit script");
            if let Some(Target) = App.Project.Scripts.get_mut(Active) {
                *Target = Script;
            }
        }
    });
    if PendingAdd {
        super::add_script(App, "Script", despite_script::example_script("starter").unwrap_or(""), CompileMode::Image, Vec::new());
    }
    if let Some(Index) = PendingRemove.filter(|Index| *Index < App.Project.Scripts.len()) {
        App.record("Delete script");
        App.Project.Scripts.remove(Index);
        if App.Desktop.ActiveScript >= Index {
            App.Desktop.ActiveScript = App.Desktop.ActiveScript.saturating_sub(1);
        }
    }
    if std::mem::take(&mut App.Desktop.RunRequested) {
        compile::run_script(App);
    }
}

fn shader_tab(App: &mut DespiteApp, Ui: &mut Ui) {
    let Shaders = App.Project.Shaders.clone();
    let mut Active = App.Desktop.ActiveShader.min(Shaders.len().saturating_sub(1));
    let mut RunNow = false;
    let mut PendingAdd = false;
    let mut PendingRemove: Option<usize> = None;
    Ui.horizontal_top(|Ui| {
        let (Add, Remove) = list_column(Ui, &Shaders, |Shader| Shader.Name.clone(), &mut Active, "shader");
        App.Desktop.ActiveShader = Active;
        PendingAdd = Add;
        PendingRemove = Remove;
        Ui.separator();
        let Some(mut Shader) = App.Project.Shaders.get(Active).cloned() else {
            Ui.vertical(|Ui| {
                Ui.label("Shaders run WGSL code on the GPU over your image. Use other project images as extra inputs (Image parameters) and render many frames for VFX.");
                if Ui.button("Create a shader").clicked() {
                    super::add_shader(App, "Shader", despite_shader::DefaultEffectSource, Vec::new(), 1, 12.0);
                }
            });
            return;
        };
        let Original = Shader.clone();
        let SideWidth = 300.0;
        let EditorWidth = (Ui.available_width() - SideWidth - 16.0).max(200.0);
        let ErrorLine = match &App.Desktop.ShaderStatus {
            Some(Err(Problem)) => Problem.Line,
            _ => None,
        };
        Ui.vertical(|Ui| {
            Ui.set_width(EditorWidth);
            Ui.horizontal(|Ui| {
                Ui.label("Name");
                Ui.text_edit_singleline(&mut Shader.Name);
                Ui.checkbox(&mut App.Desktop.ShowPrelude, "Show built-in bindings");
            });
            let Name = Shader.Name.clone();
            file_buttons(App, Ui, &mut Shader.Source, "WGSL shader", "wgsl", &Name);
            if App.Desktop.ShowPrelude {
                let (Params, Inputs) = compile::shader_params(&Shader, &App.Project);
                let Prelude = despite_shader::build_prelude(&Params, &Inputs.iter().map(|Input| Input.Name.clone()).collect::<Vec<_>>());
                egui::ScrollArea::vertical().id_salt("Prelude").max_height(160.0).show(Ui, |Ui| {
                    Ui.label(RichText::new(Prelude).monospace().small());
                });
                Ui.separator();
            }
            notepad(Ui, &mut Shader.Source, "ShaderNotepad", ErrorLine);
        });
        Ui.separator();
        Ui.vertical(|Ui| {
            Ui.set_width(SideWidth);
            egui::ScrollArea::vertical().id_salt("ShaderSide").show(Ui, |Ui| {
                let Targets: Vec<(ShaderTarget, String)> = std::iter::once((ShaderTarget::ActiveDocument, "Current image / scene".to_string()))
                    .chain(App.Project.Images.iter().map(|Found| (ShaderTarget::Image(Found.Id), format!("Image: {}", Found.Name))))
                    .chain(App.Project.Scenes.iter().map(|Found| (ShaderTarget::Scene(Found.Id), format!("Scene: {}", Found.Name))))
                    .collect();
                let Current = Targets.iter().find(|(Target, _)| *Target == Shader.Target).map(|(_, Name)| Name.clone()).unwrap_or_default();
                Ui.horizontal(|Ui| {
                    Ui.label("Input");
                    egui::ComboBox::from_id_salt("ShaderTarget").selected_text(Current).show_ui(Ui, |Ui| {
                        for (Target, Name) in &Targets {
                            Ui.selectable_value(&mut Shader.Target, *Target, Name);
                        }
                    });
                });
                Ui.horizontal(|Ui| {
                    Ui.label("Compile to");
                    mode_combo(Ui, "ShaderMode", &mut Shader.Mode, false);
                });
                Ui.horizontal(|Ui| {
                    Ui.add(egui::DragValue::new(&mut Shader.FrameCount).range(1..=App.Settings.ShaderMaxFrames).prefix("frames "));
                    Ui.add(egui::DragValue::new(&mut Shader.Fps).range(0.5..=120.0).prefix("fps "));
                });
                Ui.horizontal(|Ui| {
                    Ui.label("Output size");
                    Ui.add(egui::DragValue::new(&mut Shader.OutputWidth).range(0..=16384));
                    Ui.add(egui::DragValue::new(&mut Shader.OutputHeight).range(0..=16384));
                });
                Ui.label(RichText::new("0 × 0 = same as input").weak().small());
                Ui.checkbox(&mut Shader.Smooth, "Smooth sampling (off for pixel art)");
                Ui.horizontal(|Ui| {
                    let Busy = App.Desktop.ShaderPending.is_some();
                    if Busy {
                        Ui.spinner();
                        Ui.label("Rendering…");
                    } else if crate::icons::text_button(Ui, "Play", RichText::new("Run (F6)").strong()).clicked() {
                        RunNow = true;
                    }
                    if Ui.button("Check").on_hover_text("Check the code for errors without running it").clicked() {
                        let (Params, Inputs) = compile::shader_params(&Shader, &App.Project);
                        let Names: Vec<String> = Inputs.iter().map(|Input| Input.Name.clone()).collect();
                        App.Desktop.ShaderStatus = Some(despite_shader::validate_source(&Shader.Source, &Params, &Names).map(|()| "No errors".to_string()));
                    }
                });
                match &App.Desktop.ShaderStatus {
                    Some(Ok(Message)) => {
                        Ui.label(RichText::new(Message).color(egui::Color32::from_rgb(120, 200, 120)));
                    }
                    Some(Err(Problem)) => {
                        Ui.colored_label(egui::Color32::from_rgb(230, 110, 90), Problem.to_string());
                    }
                    None => {}
                }
                if let Some(Worker) = &App.Desktop.Worker
                    && let Ok(Name) = Worker.Adapter.lock()
                        && !Name.is_empty() {
                            Ui.label(RichText::new(format!("GPU: {Name}")).weak().small());
                        }
                Ui.separator();
                Ui.label(RichText::new("Parameters").strong());
                Ui.label(RichText::new("Read them in WGSL as Params.Name. Image parameters become textures you can sample by name.").weak().small());
                params_editor(Ui, &mut Shader.Params, &App.Project, true, "ShaderParams");
            });
        });
        if Shader != Original {
            App.record_coalesced(&format!("shader{}", Shader.Id), "Edit shader");
            if let Some(Target) = App.Project.Shaders.get_mut(Active) {
                *Target = Shader.clone();
            }
            if App.Settings.ShaderLiveValidate && Shader.Source != App.Desktop.LastValidated {
                App.Desktop.LastValidated = Shader.Source.clone();
                let (Params, Inputs) = compile::shader_params(&Shader, &App.Project);
                let Names: Vec<String> = Inputs.iter().map(|Input| Input.Name.clone()).collect();
                App.Desktop.ShaderStatus = Some(despite_shader::validate_source(&Shader.Source, &Params, &Names).map(|()| "No errors".to_string()));
            }
        }
    });
    if PendingAdd {
        super::add_shader(App, "Shader", despite_shader::DefaultEffectSource, Vec::new(), 1, 12.0);
    }
    if let Some(Index) = PendingRemove.filter(|Index| *Index < App.Project.Shaders.len()) {
        App.record("Delete shader");
        App.Project.Shaders.remove(Index);
        if App.Desktop.ActiveShader >= Index {
            App.Desktop.ActiveShader = App.Desktop.ActiveShader.saturating_sub(1);
        }
        App.Desktop.ShaderStatus = None;
    }
    if RunNow {
        compile::run_shader(App);
    }
}

fn console_tab(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.horizontal(|Ui| {
        if Ui.button("Clear").clicked() {
            App.Desktop.Console.clear();
            App.Desktop.ErrorLine = None;
        }
        if Ui.button("Copy").clicked() {
            let Text: Vec<String> = App.Desktop.Console.iter().map(|Line| Line.Text.clone()).collect();
            Ui.ctx().copy_text(Text.join("\n"));
        }
    });
    egui::ScrollArea::vertical().id_salt("Console").stick_to_bottom(true).auto_shrink([false, false]).show(Ui, |Ui| {
        for Line in &App.Desktop.Console {
            let Text = RichText::new(&Line.Text).monospace();
            Ui.label(if Line.Error { Text.color(egui::Color32::from_rgb(235, 120, 100)) } else { Text });
        }
    });
}

pub fn shortcuts(App: &mut DespiteApp, Context: &egui::Context) {
    if Context.input_mut(|Input| Input.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
        compile::run_script(App);
    }
    if Context.input_mut(|Input| Input.consume_key(egui::Modifiers::NONE, egui::Key::F6)) {
        compile::run_shader(App);
    }
    let _ = DocumentRef::Image;
}
