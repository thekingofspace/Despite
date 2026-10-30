use despite_core::animation::{self as rig, Channel, Constraint, DriverMode, EffectKind, KeyValue, NodeId, NodeKind, SpriteFrames};
use despite_core::{BlendMode, Color};
use egui::{RichText, Ui};

use super::timeline::{channel_label, key_button};
use super::{edit, edit_coalesced, node_picker, set_channel};
use crate::app::DespiteApp;

fn state_value(App: &DespiteApp, Node: NodeId, Which: Channel) -> f32 {
    let Rig = &App.Project.Rig;
    let Index = Rig.node_index(Node);
    match (Index, &App.Anim.Pose) {
        (Some(Found), Some(Pose)) if Found < Pose.States.len() => Pose.States[Found].get(Which),
        _ => Rig.node(Node).map(|Found| Found.rest_value(Which).number()).unwrap_or(0.0),
    }
}

fn number(App: &mut DespiteApp, Ui: &mut Ui, Node: NodeId, Which: Channel, Label: &str, Range: std::ops::RangeInclusive<f32>, Speed: f32, Suffix: &str) {
    Ui.horizontal(|Ui| {
        key_button(App, Ui, Node, Which);
        Ui.label(Label);
        let mut Value = state_value(App, Node, Which);
        let Response = Ui.add(egui::DragValue::new(&mut Value).range(Range).speed(Speed).suffix(Suffix).max_decimals(3));
        if Response.changed() {
            edit_coalesced(App, &format!("RigValue{:?}{}", Which, Node.0), &format!("Change {}", Which.name()));
            set_channel(App, Node, Which, KeyValue::Number(Value));
        }
    });
}

fn slider(App: &mut DespiteApp, Ui: &mut Ui, Node: NodeId, Which: Channel, Label: &str, Range: std::ops::RangeInclusive<f32>) {
    Ui.horizontal(|Ui| {
        key_button(App, Ui, Node, Which);
        let mut Value = state_value(App, Node, Which);
        if Ui.add(egui::Slider::new(&mut Value, Range).text(Label)).changed() {
            edit_coalesced(App, &format!("RigValue{:?}{}", Which, Node.0), &format!("Change {Label}"));
            set_channel(App, Node, Which, KeyValue::Number(Value));
        }
    });
}

fn color(App: &mut DespiteApp, Ui: &mut Ui, Node: NodeId, Label: &str) {
    Ui.horizontal(|Ui| {
        key_button(App, Ui, Node, Channel::Tint);
        Ui.label(Label);
        let Index = App.Project.Rig.node_index(Node).unwrap_or(0);
        let mut Value = App.Anim.Pose.as_ref().and_then(|Pose| Pose.States.get(Index)).map(|State| State.Tint).unwrap_or(Color::White);
        if crate::panels::color_button(Ui, &mut Value) {
            edit_coalesced(App, &format!("RigTint{}", Node.0), "Change colour");
            set_channel(App, Node, Channel::Tint, KeyValue::Color(Value));
        }
    });
}

fn modify(App: &mut DespiteApp, Node: NodeId, Key: &str, Label: &str, Change: impl FnOnce(&mut rig::Node)) {
    edit_coalesced(App, &format!("{Key}{}", Node.0), Label);
    if let Some(Found) = App.Project.Rig.node_mut(Node) {
        Change(Found);
    }
}

fn channel_combo(Ui: &mut Ui, Salt: impl std::hash::Hash + std::fmt::Debug, Chosen: &mut Channel, Options: &[Channel]) -> bool {
    let mut Changed = false;
    egui::ComboBox::from_id_salt(Salt).selected_text(Chosen.name()).width(130.0).show_ui(Ui, |Ui| {
        for Option in Options {
            Changed |= Ui.selectable_value(Chosen, *Option, Option.name()).changed();
        }
    });
    Changed
}

const DriverInputs: &[Channel] = &[
    Channel::Rotation,
    Channel::PositionX,
    Channel::PositionY,
    Channel::ScaleX,
    Channel::ScaleY,
    Channel::Opacity,
    Channel::Visible,
    Channel::Stretch,
    Channel::Value,
    Channel::Switch,
    Channel::Frame,
];

fn driver_outputs(Node: &rig::Node) -> Vec<Channel> {
    let mut Out = vec![
        Channel::Visible,
        Channel::Opacity,
        Channel::Switch,
        Channel::Frame,
        Channel::DrawOrder,
        Channel::Value,
        Channel::Rotation,
        Channel::PositionX,
        Channel::PositionY,
        Channel::ScaleX,
        Channel::ScaleY,
        Channel::Stretch,
    ];
    for Index in 0..Node.Params.len().min(64) {
        Out.push(Channel::Param(Index as u8));
    }
    Out
}

fn remove_constraint(App: &mut DespiteApp, Node: NodeId, Index: usize) {
    edit(App, "Remove constraint");
    if let Some(Found) = App.Project.Rig.node_mut(Node)
        && Index < Found.Constraints.len()
    {
        Found.Constraints.remove(Index);
    }
    for Clip in &mut App.Project.Rig.Clips {
        Clip.Tracks.retain(|Track| !(Track.Node == Node && Track.Channel == Channel::Mix(Index as u8)));
        for Track in &mut Clip.Tracks {
            if Track.Node == Node
                && let Channel::Mix(Other) = Track.Channel
                && Other as usize > Index
            {
                Track.Channel = Channel::Mix(Other - 1);
            }
        }
    }
}

fn swap_constraints(App: &mut DespiteApp, Node: NodeId, First: usize, Second: usize) {
    edit(App, "Reorder constraints");
    if let Some(Found) = App.Project.Rig.node_mut(Node)
        && First < Found.Constraints.len()
        && Second < Found.Constraints.len()
    {
        Found.Constraints.swap(First, Second);
    }
    for Clip in &mut App.Project.Rig.Clips {
        for Track in &mut Clip.Tracks {
            if Track.Node != Node {
                continue;
            }
            if Track.Channel == Channel::Mix(First as u8) {
                Track.Channel = Channel::Mix(Second as u8);
            } else if Track.Channel == Channel::Mix(Second as u8) {
                Track.Channel = Channel::Mix(First as u8);
            }
        }
    }
}

fn constraint_ui(App: &mut DespiteApp, Ui: &mut Ui, Node: NodeId, Index: usize, Total: usize) {
    let Some(mut Rule) = App.Project.Rig.node(Node).and_then(|Found| Found.Constraints.get(Index).cloned()) else { return };
    let Original = Rule.clone();
    let Salt = ("RigConstraint", Node.0, Index);
    let mut Remove = false;
    let mut MoveUp = false;
    let mut MoveDown = false;
    egui::Frame::group(Ui.style()).show(Ui, |Ui| {
        Ui.horizontal(|Ui| {
            Ui.label(RichText::new(Rule.name()).strong());
            Ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |Ui| {
                Remove = crate::icons::button(Ui, "Close", false, "Remove").clicked();
                MoveDown = crate::icons::enabled_button(Ui, Index + 1 < Total, "ArrowDown", "Run later").clicked();
                MoveUp = crate::icons::enabled_button(Ui, Index > 0, "ArrowUp", "Run earlier").clicked();
            });
        });
        Ui.label(RichText::new(Rule.description()).weak().small());
        if !matches!(Rule, Constraint::Switch | Constraint::Driver { .. } | Constraint::Visibility { .. }) {
            slider(App, Ui, Node, Channel::Mix(Index as u8), "mix", 0.0..=1.0);
        }
        let Exclude = Some(Node);
        match &mut Rule {
            Constraint::LookAt { Target, Offset, UseLimits, LimitMin, LimitMax, Slide, .. } => {
                Ui.horizontal(|Ui| {
                    Ui.label("Look at");
                    if let Some(Picked) = node_picker(App, Ui, (Salt, "Target"), *Target, Exclude, "Choose…") {
                        *Target = Picked;
                    }
                });
                Ui.horizontal(|Ui| {
                    Ui.label("Angle offset");
                    Ui.add(egui::DragValue::new(Offset).speed(0.5).suffix("°"));
                });
                Ui.horizontal(|Ui| {
                    Ui.label("Slide instead");
                    Ui.add(egui::DragValue::new(Slide).range(0.0..=10_000.0).speed(0.2).suffix(" px")).on_hover_text("0 = rotate to face the target. Above 0 = move up to this far towards it (pupils)");
                });
                Ui.horizontal(|Ui| {
                    Ui.checkbox(UseLimits, "Limit rotation");
                    if *UseLimits {
                        Ui.add(egui::DragValue::new(LimitMin).speed(0.5).prefix("min ").suffix("°"));
                        Ui.add(egui::DragValue::new(LimitMax).speed(0.5).prefix("max ").suffix("°"));
                    }
                });
            }
            Constraint::TwoBoneIk { Target, BendPositive, .. } => {
                Ui.horizontal(|Ui| {
                    Ui.label("Reach for");
                    if let Some(Picked) = node_picker(App, Ui, (Salt, "Target"), *Target, Exclude, "Choose…") {
                        *Target = Picked;
                    }
                });
                let mut Flip = !*BendPositive;
                if Ui.checkbox(&mut Flip, "Bend the other way").changed() {
                    *BendPositive = !Flip;
                }
                let HasParent = App.Project.Rig.node(Node).and_then(|Found| Found.Parent).is_some();
                if !HasParent || App.Project.Rig.node(Node).is_some_and(|Found| !Found.Kind.is_bone()) {
                    Ui.colored_label(egui::Color32::from_rgb(230, 170, 80), "Put this on the lower bone (forearm / shin); its parent is the upper bone.");
                }
            }
            Constraint::StretchTo { Target, MaxStretch, PreserveVolume, .. } => {
                Ui.horizontal(|Ui| {
                    Ui.label("Stretch to");
                    if let Some(Picked) = node_picker(App, Ui, (Salt, "Target"), *Target, Exclude, "Choose…") {
                        *Target = Picked;
                    }
                });
                Ui.horizontal(|Ui| {
                    Ui.label("Max stretch");
                    Ui.add(egui::DragValue::new(MaxStretch).range(0.0..=20.0).speed(0.02));
                });
                Ui.checkbox(PreserveVolume, "Keep volume (squash sideways)");
                if App.Project.Rig.node(Node).is_some_and(|Found| !Found.Kind.is_bone()) {
                    Ui.label(RichText::new("Only bones stretch; other nodes just aim").weak().small());
                }
            }
            Constraint::Follow { Target, Position, Rotation, Scale, .. } => {
                Ui.horizontal(|Ui| {
                    Ui.label("Follow");
                    if let Some(Picked) = node_picker(App, Ui, (Salt, "Target"), *Target, Exclude, "Choose…") {
                        *Target = Picked;
                    }
                });
                Ui.horizontal(|Ui| {
                    Ui.checkbox(Position, "Position");
                    Ui.checkbox(Rotation, "Rotation");
                    Ui.checkbox(Scale, "Scale");
                });
            }
            Constraint::Driver { Source, From, To, Mode } => {
                Ui.horizontal(|Ui| {
                    Ui.label("When");
                    if let Some(Picked) = node_picker(App, Ui, (Salt, "Source"), *Source, None, "Choose…") {
                        *Source = Picked;
                    }
                    channel_combo(Ui, (Salt, "From"), From, DriverInputs);
                });
                let Outputs = App.Project.Rig.node(Node).map(driver_outputs).unwrap_or_default();
                Ui.horizontal(|Ui| {
                    Ui.label("drive this node's");
                    channel_combo(Ui, (Salt, "To"), To, &Outputs);
                });
                egui::ComboBox::from_id_salt((Salt, "Mode")).selected_text(Mode.name()).show_ui(Ui, |Ui| {
                    if Ui.selectable_label(matches!(Mode, DriverMode::Linear { .. }), "Map range").clicked() {
                        *Mode = DriverMode::Linear { InMin: 0.0, InMax: 90.0, OutMin: 0.0, OutMax: 1.0, Clamp: true };
                    }
                    if Ui.selectable_label(matches!(Mode, DriverMode::Threshold { .. }), "Threshold (on/off)").clicked() {
                        *Mode = DriverMode::Threshold { Threshold: 30.0, Above: true };
                    }
                    if Ui.selectable_label(matches!(Mode, DriverMode::Copy), "Copy").clicked() {
                        *Mode = DriverMode::Copy;
                    }
                });
                match Mode {
                    DriverMode::Linear { InMin, InMax, OutMin, OutMax, Clamp } => {
                        Ui.horizontal(|Ui| {
                            Ui.label("in");
                            Ui.add(egui::DragValue::new(InMin).speed(0.1));
                            Ui.label("…");
                            Ui.add(egui::DragValue::new(InMax).speed(0.1));
                        });
                        Ui.horizontal(|Ui| {
                            Ui.label("out");
                            Ui.add(egui::DragValue::new(OutMin).speed(0.01));
                            Ui.label("…");
                            Ui.add(egui::DragValue::new(OutMax).speed(0.01));
                            Ui.checkbox(Clamp, "clamp");
                        });
                    }
                    DriverMode::Threshold { Threshold, Above } => {
                        Ui.horizontal(|Ui| {
                            Ui.label("on when");
                            egui::ComboBox::from_id_salt((Salt, "Above")).selected_text(if *Above { "at or above" } else { "below" }).show_ui(Ui, |Ui| {
                                Ui.selectable_value(Above, true, "at or above");
                                Ui.selectable_value(Above, false, "below");
                            });
                            Ui.add(egui::DragValue::new(Threshold).speed(0.1));
                        });
                    }
                    DriverMode::Copy => {}
                }
            }
            Constraint::Switch => {
                let Children: Vec<String> = App.Project.Rig.Nodes.iter().filter(|Other| Other.Parent == Some(Node)).map(|Other| Other.Name.clone()).collect();
                let Current = state_value(App, Node, Channel::Switch).round().max(0.0) as usize;
                Ui.label(format!("Showing: {}", Children.get(Current.min(Children.len().saturating_sub(1))).cloned().unwrap_or_else(|| "(no children yet)".into())));
                number(App, Ui, Node, Channel::Switch, "Child number", 0.0..=(Children.len().saturating_sub(1)) as f32, 0.05, "");
            }
            Constraint::Jiggle { Stiffness, Damping, Gravity, .. } => {
                Ui.add(egui::Slider::new(Stiffness, 0.01..=1.0).text("stiffness"));
                Ui.add(egui::Slider::new(Damping, 0.0..=1.0).text("damping"));
                Ui.add(egui::Slider::new(Gravity, -5.0..=5.0).text("gravity"));
                Ui.label(RichText::new("Simulated from the start of the animation, so it looks the same in exports").weak().small());
            }
            Constraint::Visibility { Targets, From, Min, Max, Invert } => {
                Ui.horizontal(|Ui| {
                    Ui.label("While my");
                    channel_combo(Ui, (Salt, "From"), From, DriverInputs);
                });
                Ui.horizontal(|Ui| {
                    Ui.label("is between");
                    Ui.add(egui::DragValue::new(Min).speed(0.1));
                    Ui.label("and");
                    Ui.add(egui::DragValue::new(Max).speed(0.1));
                });
                Ui.checkbox(Invert, "Hide them instead of showing them");
                Ui.label("Parts:");
                let mut RemoveTarget = None;
                for (TargetIndex, Target) in Targets.iter().enumerate() {
                    Ui.horizontal(|Ui| {
                        Ui.label(App.Project.Rig.node(*Target).map(|Found| Found.Name.clone()).unwrap_or_else(|| "?".into()));
                        if crate::icons::button(Ui, "Close", false, "Remove").clicked() {
                            RemoveTarget = Some(TargetIndex);
                        }
                    });
                }
                if let Some(Gone) = RemoveTarget {
                    Targets.remove(Gone);
                }
                if let Some(Some(Picked)) = node_picker(App, Ui, (Salt, "AddTarget"), None, Exclude, "Add a part…")
                    && !Targets.contains(&Picked)
                {
                    Targets.push(Picked);
                }
            }
        }
    });
    if Remove {
        remove_constraint(App, Node, Index);
        return;
    }
    if MoveUp {
        swap_constraints(App, Node, Index, Index - 1);
        return;
    }
    if MoveDown {
        swap_constraints(App, Node, Index, Index + 1);
        return;
    }
    if Rule != Original {
        edit_coalesced(App, &format!("RigConstraint{}-{Index}", Node.0), "Edit constraint");
        if let Some(Found) = App.Project.Rig.node_mut(Node)
            && let Some(Slot) = Found.Constraints.get_mut(Index)
        {
            *Slot = Rule;
        }
    }
}

fn effect_ui(App: &mut DespiteApp, Ui: &mut Ui, Node: NodeId, Effect: EffectKind) {
    Ui.label(RichText::new(match Effect {
        EffectKind::Shader { .. } => "Runs one of your shaders. Its number settings can be keyed on the timeline.",
        _ => "Keys on these settings animate the effect.",
    }).weak().small());
    Ui.label(RichText::new("With children it affects only them; without children it affects everything drawn below it. Opacity fades the effect.").weak().small());
    if let EffectKind::Shader { Shader } = Effect {
        #[cfg(feature = "desktop")]
        {
            let Names: Vec<(u64, String)> = App.Project.Shaders.iter().map(|Found| (Found.Id, Found.Name.clone())).collect();
            let Current = Names.iter().find(|(Id, _)| *Id == Shader).map(|(_, Name)| Name.clone()).unwrap_or_else(|| "Missing shader".into());
            let mut Picked = None;
            egui::ComboBox::from_id_salt(("RigShaderPick", Node.0)).selected_text(Current).show_ui(Ui, |Ui| {
                for (Id, Name) in &Names {
                    if Ui.selectable_label(*Id == Shader, Name).clicked() {
                        Picked = Some(*Id);
                    }
                }
            });
            if let Some(Id) = Picked {
                let Defaults = App.Project.Shaders.iter().find(|Found| Found.Id == Id).map(crate::desktop::export::numeric_defaults).unwrap_or_default();
                modify(App, Node, "RigShader", "Change shader", |Found| {
                    Found.Kind = NodeKind::Effect { Effect: EffectKind::Shader { Shader: Id } };
                    Found.Params = Defaults;
                });
            }
            if let Some(Problem) = App.Desktop.ShaderFailures.get(&Shader) {
                Ui.colored_label(egui::Color32::from_rgb(230, 120, 80), Problem);
            }
        }
        #[cfg(not(feature = "desktop"))]
        {
            let _ = Shader;
            Ui.colored_label(egui::Color32::from_rgb(230, 170, 80), "Shader effects run in the PC version; here they pass the picture through unchanged.");
        }
        let Count = App.Project.Rig.node(Node).map(|Found| Found.Params.len()).unwrap_or(0);
        for Index in 0..Count.min(64) {
            let Label = channel_label(App, Node, Channel::Param(Index as u8));
            number(App, Ui, Node, Channel::Param(Index as u8), &Label, f32::MIN..=f32::MAX, 0.01, "");
        }
        return;
    }
    for (Index, Entry) in Effect.params().iter().enumerate() {
        slider(App, Ui, Node, Channel::Param(Index as u8), Entry.Name, Entry.Min..=Entry.Max);
    }
    if Effect.uses_color() {
        color(App, Ui, Node, "Colour");
    }
}

fn overview(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.heading("Rig");
    let Rig = &App.Project.Rig;
    Ui.label(format!("Stage {}×{}{}", Rig.Width, Rig.Height, if Rig.PixelArt { ", pixel art" } else { "" }));
    if Ui.button("Stage settings…").clicked() {
        App.Anim.ShowStageSettings = true;
    }
}

pub fn properties_panel(App: &mut DespiteApp, Ui: &mut Ui) {
    let Some(Node) = App.Anim.primary() else {
        overview(App, Ui);
        return;
    };
    let Some(Found) = App.Project.Rig.node(Node).cloned() else { return };
    Ui.horizontal(|Ui| {
        crate::icons::show(Ui, super::node_icon(&Found.Kind));
        let mut Name = Found.Name.clone();
        if Ui.add(egui::TextEdit::singleline(&mut Name).desired_width(170.0)).changed() {
            modify(App, Node, "RigName", "Rename", |Target| Target.Name = Name);
        }
        Ui.label(RichText::new(Found.Kind.name()).weak());
    });
    if App.Anim.Selected.len() > 1 {
        Ui.label(RichText::new(format!("{} selected; editing the last one", App.Anim.Selected.len())).weak().small());
    }
    Ui.horizontal(|Ui| {
        Ui.label("Parent");
        let mut Excluded = App.Project.Rig.descendants(Node);
        Excluded.push(Node);
        let Rig = &App.Project.Rig;
        let Label = Found.Parent.and_then(|Id| Rig.node(Id)).map(|Parent| Parent.Name.clone()).unwrap_or_else(|| "(top level)".into());
        let mut Picked = None;
        egui::ComboBox::from_id_salt(("RigParent", Node.0)).selected_text(Label).width(150.0).show_ui(Ui, |Ui| {
            if Ui.selectable_label(Found.Parent.is_none(), "(top level)").clicked() {
                Picked = Some(None);
            }
            for Index in Rig.preorder() {
                let Candidate = &Rig.Nodes[Index];
                if Excluded.contains(&Candidate.Id) {
                    continue;
                }
                if Ui.selectable_label(Found.Parent == Some(Candidate.Id), format!("{}{}", "  ".repeat(Rig.depth(Candidate.Id).min(8)), Candidate.Name)).clicked() {
                    Picked = Some(Some(Candidate.Id));
                }
            }
        });
        if let Some(Parent) = Picked {
            edit(App, "Reparent");
            rig::reparent_keep_world(&mut App.Project, Node, Parent);
        }
    });
    let IsEffect = matches!(Found.Kind, NodeKind::Effect { .. });
    let IsController = matches!(Found.Kind, NodeKind::Controller { .. });
    if !IsEffect && !IsController {
        egui::CollapsingHeader::new(RichText::new("Transform").strong()).id_salt("RigTransform").default_open(true).show(Ui, |Ui| {
            number(App, Ui, Node, Channel::PositionX, "X", f32::MIN..=f32::MAX, 0.5, "");
            number(App, Ui, Node, Channel::PositionY, "Y", f32::MIN..=f32::MAX, 0.5, "");
            number(App, Ui, Node, Channel::Rotation, "Rotation", f32::MIN..=f32::MAX, 0.5, "°");
            number(App, Ui, Node, Channel::ScaleX, "Scale X", -100.0..=100.0, 0.01, "");
            number(App, Ui, Node, Channel::ScaleY, "Scale Y", -100.0..=100.0, 0.01, "");
            Ui.horizontal(|Ui| {
                if Ui.small_button("Flip X").clicked() {
                    let Value = -state_value(App, Node, Channel::ScaleX);
                    edit(App, "Flip");
                    set_channel(App, Node, Channel::ScaleX, KeyValue::Number(Value));
                }
                if Ui.small_button("Flip Y").clicked() {
                    let Value = -state_value(App, Node, Channel::ScaleY);
                    edit(App, "Flip");
                    set_channel(App, Node, Channel::ScaleY, KeyValue::Number(Value));
                }
                if Ui.small_button("Reset").on_hover_text("Rotation 0, scale 1").clicked() {
                    edit(App, "Reset transform");
                    set_channel(App, Node, Channel::Rotation, KeyValue::Number(0.0));
                    set_channel(App, Node, Channel::ScaleX, KeyValue::Number(1.0));
                    set_channel(App, Node, Channel::ScaleY, KeyValue::Number(1.0));
                }
            });
            Ui.horizontal_wrapped(|Ui| {
                let (mut Rotation, mut Scale) = (Found.InheritRotation, Found.InheritScale);
                if Ui.checkbox(&mut Rotation, "Follow parent rotation").changed() {
                    modify(App, Node, "RigInherit", "Inherit rotation", |Target| Target.InheritRotation = Rotation);
                }
                if Ui.checkbox(&mut Scale, "Follow parent scale").changed() {
                    modify(App, Node, "RigInherit", "Inherit scale", |Target| Target.InheritScale = Scale);
                }
            });
        });
        egui::CollapsingHeader::new(RichText::new("Squash & stretch").strong()).id_salt("RigStretch").default_open(Found.Kind.is_bone()).show(Ui, |Ui| {
            number(App, Ui, Node, Channel::Stretch, "Stretch", -0.95..=20.0, 0.01, "");
            let mut Volume = Found.PreserveVolume;
            if Ui.checkbox(&mut Volume, "Keep volume (gets thinner as it stretches)").changed() {
                modify(App, Node, "RigVolume", "Keep volume", |Target| Target.PreserveVolume = Volume);
            }
            Ui.label(RichText::new("Stretches along the node's X axis; for bones that is along the bone").weak().small());
        });
    }
    egui::CollapsingHeader::new(RichText::new(if IsEffect { "Effect" } else { "Appearance" }).strong()).id_salt("RigLook").default_open(true).show(Ui, |Ui| {
        if let NodeKind::Effect { Effect } = Found.Kind {
            effect_ui(App, Ui, Node, Effect);
            Ui.separator();
        }
        if !IsController {
            slider(App, Ui, Node, Channel::Opacity, if IsEffect { "amount" } else { "opacity" }, 0.0..=1.0);
        }
        Ui.horizontal(|Ui| {
            key_button(App, Ui, Node, Channel::Visible);
            let mut Visible = state_value(App, Node, Channel::Visible) >= 0.5;
            if Ui.checkbox(&mut Visible, "Visible").changed() {
                edit(App, "Visibility");
                set_channel(App, Node, Channel::Visible, KeyValue::Flag(Visible));
            }
        });
        if !IsController {
            number(App, Ui, Node, Channel::DrawOrder, "Draw order", -10_000.0..=10_000.0, 0.05, "");
            Ui.label(RichText::new("Siblings with higher numbers draw on top; below 0 draws behind the parent").weak().small());
        }
        if matches!(Found.Kind, NodeKind::Sprite { .. } | NodeKind::Group) {
            color(App, Ui, Node, "Tint");
        }
        if matches!(Found.Kind, NodeKind::Sprite { .. } | NodeKind::Effect { .. }) {
            Ui.horizontal(|Ui| {
                Ui.label("Blend");
                let mut Blend: BlendMode = Found.Blend;
                if crate::panels::blend_combo(Ui, &format!("RigBlend{}", Node.0), &mut Blend) {
                    modify(App, Node, "RigBlend", "Blend mode", |Target| Target.Blend = Blend);
                }
            });
        }
    });
    match &Found.Kind {
        NodeKind::Sprite { Source, PivotX, PivotY, Frames } => {
            egui::CollapsingHeader::new(RichText::new("Sprite").strong()).id_salt("RigSprite").default_open(true).show(Ui, |Ui| {
                let Images: Vec<(despite_core::ImageId, String, u32, u32, usize)> = App.Project.Images.iter().map(|Image| (Image.Id, Image.Name.clone(), Image.Width, Image.Height, Image.Layers.len())).collect();
                let Current = Source.and_then(|Id| Images.iter().find(|Entry| Entry.0 == Id));
                let mut Picked = None;
                egui::ComboBox::from_id_salt(("RigSpriteImage", Node.0)).selected_text(Current.map(|Entry| Entry.1.clone()).unwrap_or_else(|| "(none)".into())).width(170.0).show_ui(Ui, |Ui| {
                    for Entry in &Images {
                        if Ui.selectable_label(Some(Entry.0) == *Source, &Entry.1).clicked() {
                            Picked = Some(Entry.0);
                        }
                    }
                });
                if let Some(Image) = Picked {
                    modify(App, Node, "RigSpriteImage", "Change image", |Target| {
                        if let NodeKind::Sprite { Source, .. } = &mut Target.Kind {
                            *Source = Some(Image);
                        }
                    });
                }
                Ui.horizontal(|Ui| {
                    let (mut X, mut Y) = (*PivotX, *PivotY);
                    Ui.label("Pivot");
                    let Changed = Ui.add(egui::DragValue::new(&mut X).speed(0.5).prefix("x ")).changed() | Ui.add(egui::DragValue::new(&mut Y).speed(0.5).prefix("y ")).changed();
                    if Ui.small_button("Centre").clicked()
                        && let Some(Entry) = Current
                    {
                        X = Entry.2 as f32 / 2.0;
                        Y = Entry.3 as f32 / 2.0;
                        modify(App, Node, "RigPivot", "Pivot", |Target| {
                            if let NodeKind::Sprite { PivotX, PivotY, .. } = &mut Target.Kind {
                                *PivotX = X;
                                *PivotY = Y;
                            }
                        });
                    } else if Changed {
                        modify(App, Node, "RigPivot", "Pivot", |Target| {
                            if let NodeKind::Sprite { PivotX, PivotY, .. } = &mut Target.Kind {
                                *PivotX = X;
                                *PivotY = Y;
                            }
                        });
                    }
                });
                Ui.label(RichText::new("Tip: the Pivot tool moves the pivot without moving the picture").weak().small());
                let mut Mode = *Frames;
                let Layers = Current.map(|Entry| Entry.4).unwrap_or(1);
                egui::ComboBox::from_id_salt(("RigSpriteFrames", Node.0))
                    .selected_text(if Mode == SpriteFrames::Layers { "Each layer is a frame" } else { "Whole image" })
                    .show_ui(Ui, |Ui| {
                        Ui.selectable_value(&mut Mode, SpriteFrames::Composite, "Whole image");
                        Ui.selectable_value(&mut Mode, SpriteFrames::Layers, "Each layer is a frame").on_hover_text("Swap drawings by keying the frame number: mouths, hands, blinks");
                    });
                if Mode != *Frames {
                    modify(App, Node, "RigSpriteFrames", "Sprite frames", |Target| {
                        if let NodeKind::Sprite { Frames, .. } = &mut Target.Kind {
                            *Frames = Mode;
                        }
                    });
                }
                if *Frames == SpriteFrames::Layers {
                    number(App, Ui, Node, Channel::Frame, "Frame", 0.0..=(Layers.saturating_sub(1)) as f32, 0.05, &format!(" of {Layers}"));
                }
                let mut Clip = Found.ClipChildren;
                if Ui.checkbox(&mut Clip, "Clip children to this shape").on_hover_text("Children only show where this sprite has pixels: shading, patterns, eyes inside sockets").changed() {
                    modify(App, Node, "RigClip", "Clipping", |Target| Target.ClipChildren = Clip);
                }
            });
        }
        NodeKind::Bone { Length, Paint } => {
            egui::CollapsingHeader::new(RichText::new("Bone").strong()).id_salt("RigBone").default_open(true).show(Ui, |Ui| {
                Ui.horizontal(|Ui| {
                    let mut Value = *Length;
                    Ui.label("Length");
                    if Ui.add(egui::DragValue::new(&mut Value).range(1.0..=100_000.0).speed(0.5)).changed() {
                        modify(App, Node, "RigBoneLength", "Bone length", |Target| {
                            if let NodeKind::Bone { Length, .. } = &mut Target.Kind {
                                *Length = Value;
                            }
                        });
                    }
                    let mut Tint = *Paint;
                    if crate::panels::color_button(Ui, &mut Tint) {
                        modify(App, Node, "RigBonePaint", "Bone colour", |Target| {
                            if let NodeKind::Bone { Paint, .. } = &mut Target.Kind {
                                *Paint = Tint;
                            }
                        });
                    }
                });
                if Ui.button("Add child bone at the tip").clicked() {
                    edit(App, "Add bone");
                    let Pose = App.Anim.Pose.clone();
                    if let (Some(Pose), Some(Index)) = (Pose, App.Project.Rig.node_index(Node)) {
                        let World = Pose.World[Index];
                        let (StartX, StartY) = World.apply(*Length, 0.0);
                        let (EndX, EndY) = World.apply(*Length * 1.8, 0.0);
                        let Created = rig::add_bone(&mut App.Project, Some(Node), StartX, StartY, EndX, EndY);
                        App.Anim.select(Some(Created), false);
                    }
                }
            });
        }
        NodeKind::Controller { Min, Max } => {
            egui::CollapsingHeader::new(RichText::new("Controller").strong()).id_salt("RigController").default_open(true).show(Ui, |Ui| {
                slider(App, Ui, Node, Channel::Value, "value", Min.min(*Max)..=Max.max(*Min).max(Min.min(*Max) + 1e-3));
                Ui.horizontal(|Ui| {
                    let (mut Low, mut High) = (*Min, *Max);
                    let Changed = Ui.add(egui::DragValue::new(&mut Low).speed(0.1).prefix("min ")).changed() | Ui.add(egui::DragValue::new(&mut High).speed(0.1).prefix("max ")).changed();
                    if Changed {
                        modify(App, Node, "RigControllerRange", "Controller range", |Target| {
                            if let NodeKind::Controller { Min, Max } = &mut Target.Kind {
                                *Min = Low;
                                *Max = High;
                            }
                        });
                    }
                });
                Ui.label(RichText::new("Use it as the source of Driver or Show/hide rules on other nodes, then key its value").weak().small());
            });
        }
        NodeKind::Group | NodeKind::Effect { .. } => {}
    }
    egui::CollapsingHeader::new(RichText::new("Masking").strong()).id_salt("RigMask").default_open(Found.MaskBy.is_some()).show(Ui, |Ui| {
        Ui.horizontal(|Ui| {
            Ui.label("Mask by");
            if let Some(Picked) = node_picker(App, Ui, ("RigMaskBy", Node.0), Found.MaskBy, Some(Node), "(none)") {
                modify(App, Node, "RigMaskBy", "Mask", |Target| Target.MaskBy = Picked);
            }
        });
        let mut Invert = Found.InvertMask;
        if Ui.checkbox(&mut Invert, "Invert (cut the mask shape out)").changed() {
            modify(App, Node, "RigInvertMask", "Invert mask", |Target| Target.InvertMask = Invert);
        }
        Ui.label(RichText::new("The mask's shape is used even when it is hidden, so hide it to use it purely as a mask").weak().small());
    });
    egui::CollapsingHeader::new(RichText::new(format!("Constraints & logic ({})", Found.Constraints.len())).strong()).id_salt("RigConstraints").default_open(true).show(Ui, |Ui| {
        let Total = Found.Constraints.len();
        for Index in 0..Total {
            constraint_ui(App, Ui, Node, Index, Total);
        }
        egui::ComboBox::from_id_salt(("RigAddConstraint", Node.0)).selected_text("Add constraint…").width(200.0).show_ui(Ui, |Ui| {
            for Rule in Constraint::catalogue() {
                if Ui.selectable_label(false, Rule.name()).on_hover_text(Rule.description()).clicked() {
                    edit(App, "Add constraint");
                    if let Some(Target) = App.Project.Rig.node_mut(Node) {
                        Target.Constraints.push(Rule);
                    }
                }
            }
        });
    });
}
