use despite_core::animation::{self as rig, Channel, Easing, KeyValue, NodeId, NodeKind};
use egui::{Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, Vec2};

use super::{KeyRef, clip_index, edit, set_frame};
use crate::app::DespiteApp;

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Node(NodeId),
    Value(NodeId, Channel),
}

pub fn channel_label(App: &DespiteApp, Node: NodeId, Which: Channel) -> String {
    if let Channel::Param(Index) = Which
        && let Some(Found) = App.Project.Rig.node(Node)
        && let NodeKind::Effect { Effect } = Found.Kind
    {
        if let Some(Entry) = Effect.params().get(Index as usize) {
            return Entry.Name.to_string();
        }
        #[cfg(feature = "desktop")]
        if let rig::EffectKind::Shader { Shader } = Effect
            && let Some(Source) = App.Project.Shaders.iter().find(|Candidate| Candidate.Id == Shader)
            && let Some(Decl) = crate::desktop::export::numeric_params(Source).get(Index as usize)
        {
            return Decl.Name.clone();
        }
    }
    if Which == Channel::Tint
        && App.Project.Rig.node(Node).is_some_and(|Found| matches!(Found.Kind, NodeKind::Effect { .. }))
    {
        return "Effect colour".into();
    }
    if let Channel::Mix(Index) = Which
        && let Some(Rule) = App.Project.Rig.node(Node).and_then(|Found| Found.Constraints.get(Index as usize))
    {
        return format!("{} mix", Rule.name());
    }
    Which.name()
}

fn rows(App: &DespiteApp) -> Vec<Row> {
    let Rig = &App.Project.Rig;
    let Clip = &Rig.Clips[clip_index(App)];
    let mut Out = Vec::new();
    for Index in Rig.preorder() {
        let Found = &Rig.Nodes[Index];
        let Animated = Clip.Tracks.iter().any(|Track| Track.Node == Found.Id);
        let Selected = App.Anim.Selected.contains(&Found.Id);
        if !Animated && !Selected {
            continue;
        }
        Out.push(Row::Node(Found.Id));
        if Selected && App.Anim.ShowChannels {
            let mut Channels: Vec<Channel> = super::basic_channels(App, Found.Id);
            for Track in Clip.Tracks.iter().filter(|Track| Track.Node == Found.Id) {
                if !Channels.contains(&Track.Channel) {
                    Channels.push(Track.Channel);
                }
            }
            for Which in Channels {
                Out.push(Row::Value(Found.Id, Which));
            }
        }
    }
    Out
}

fn current_value(App: &DespiteApp, Node: NodeId, Which: Channel, Frame: f32) -> Option<KeyValue> {
    let Rig = &App.Project.Rig;
    let Solver = rig::Solver::new(Rig);
    let Index = Solver.index(Some(Node))?;
    let Pose = Solver.evaluate(Rig.Clips.get(clip_index(App)), Frame);
    let State = &Pose.States[Index];
    Some(if Which.is_flag() {
        KeyValue::Flag(State.Visible)
    } else if Which.is_color() {
        KeyValue::Color(State.Tint)
    } else {
        KeyValue::Number(State.get(Which))
    })
}

pub fn toggle_key(App: &mut DespiteApp, Node: NodeId, Which: Channel) {
    let Frame = App.Anim.Frame;
    let Clip = clip_index(App);
    let Exists = App.Project.Rig.Clips[Clip].has_key(Node, Which, Frame);
    if Exists {
        edit(App, "Delete key");
        App.Project.Rig.Clips[Clip].remove_key(Node, Which, Frame);
    } else if let Some(Value) = current_value(App, Node, Which, Frame) {
        edit(App, "Add key");
        App.Project.Rig.Clips[Clip].set_key(Node, Which, Frame, Value);
    }
}

fn keys_in_row(App: &DespiteApp, Entry: Row) -> Vec<(f32, Easing)> {
    let Clip = &App.Project.Rig.Clips[clip_index(App)];
    match Entry {
        Row::Value(Node, Which) => Clip.track(Node, Which).map(|Track| Track.Keys.iter().map(|Key| (Key.Frame, Key.Ease)).collect()).unwrap_or_default(),
        Row::Node(Node) => Clip.node_key_frames(Node).into_iter().map(|Frame| (Frame, Easing::Linear)).collect(),
    }
}

fn refs_for(App: &DespiteApp, Entry: Row, Frame: f32) -> Vec<KeyRef> {
    let Clip = &App.Project.Rig.Clips[clip_index(App)];
    match Entry {
        Row::Value(Node, Which) => vec![KeyRef { Node, Channel: Which, Frame }],
        Row::Node(Node) => Clip
            .Tracks
            .iter()
            .filter(|Track| Track.Node == Node && Track.key_at(Frame).is_some())
            .map(|Track| KeyRef { Node, Channel: Track.Channel, Frame })
            .collect(),
    }
}

fn is_selected(App: &DespiteApp, Refs: &[KeyRef]) -> bool {
    !Refs.is_empty() && Refs.iter().all(|Entry| App.Anim.Keys.iter().any(|Chosen| Chosen.Node == Entry.Node && Chosen.Channel == Entry.Channel && (Chosen.Frame - Entry.Frame).abs() < 0.01))
}

fn move_keys(App: &mut DespiteApp, Originals: &[KeyRef], From: f32, To: f32) {
    let Clip = clip_index(App);
    let Length = App.Project.Rig.Clips[Clip].Length.max(1) as f32;
    let Active = &mut App.Project.Rig.Clips[Clip];
    let mut Moving = Vec::new();
    for Entry in Originals {
        let Current = Entry.Frame + From;
        if let Some(Track) = Active.Tracks.iter_mut().find(|Track| Track.Node == Entry.Node && Track.Channel == Entry.Channel)
            && let Some(Index) = Track.key_at(Current)
        {
            Moving.push((*Entry, Track.Keys.remove(Index)));
        }
    }
    let mut Placed = Vec::new();
    for (Entry, mut Key) in Moving {
        Key.Frame = (Entry.Frame + To).clamp(0.0, Length - 1.0);
        let Track = Active.track_mut(Entry.Node, Entry.Channel);
        Track.Keys.retain(|Existing| (Existing.Frame - Key.Frame).abs() >= 0.01);
        Track.Keys.push(Key);
        Track.sort();
        Placed.push(KeyRef { Frame: Key.Frame, ..Entry });
    }
    Active.Tracks.retain(|Track| !Track.Keys.is_empty());
    App.Anim.Keys = Placed;
    super::changed(App);
}

fn header(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.horizontal_wrapped(|Ui| {
        let Clip = clip_index(App);
        let Names: Vec<String> = App.Project.Rig.Clips.iter().map(|Found| Found.Name.clone()).collect();
        egui::ComboBox::from_id_salt("RigClipPicker").selected_text(Names[Clip].clone()).width(130.0).show_ui(Ui, |Ui| {
            for (Index, Name) in Names.iter().enumerate() {
                if Ui.selectable_label(Index == Clip, Name).clicked() {
                    App.Project.Rig.ActiveClip = Index;
                    App.Anim.Keys.clear();
                    App.Generation += 1;
                }
            }
            Ui.separator();
            super::clip_menu(App, Ui);
        });
        if crate::icons::button(Ui, "FirstFrame", false, "First frame (Home)").clicked() {
            set_frame(App, 0.0);
        }
        if crate::icons::button(Ui, "PrevKey", false, "Previous key (Shift+Left)").clicked() {
            super::jump_key(App, false);
        }
        let Playing = App.Anim.Playing;
        if crate::icons::button(Ui, if Playing { "Pause" } else { "Play" }, Playing, "Play / pause (Space)").clicked() {
            super::toggle_play(App);
        }
        if crate::icons::button(Ui, "NextKey", false, "Next key (Shift+Right)").clicked() {
            super::jump_key(App, true);
        }
        if crate::icons::button(Ui, "LastFrame", false, "Last frame").clicked() {
            let Last = App.Project.Rig.Clips[Clip].Length.max(1) as f32 - 1.0;
            set_frame(App, Last);
        }
        let mut Shown = App.Anim.Frame as u32 + 1;
        let Length = App.Project.Rig.Clips[Clip].Length.max(1);
        if Ui.add(egui::DragValue::new(&mut Shown).range(1..=Length).prefix("frame ")).changed() {
            set_frame(App, Shown as f32 - 1.0);
        }
        Ui.label(RichText::new(format!("/ {Length}")).weak());
        let (mut Fps, mut NewLength, mut Looping) = {
            let Found = &App.Project.Rig.Clips[Clip];
            (Found.Fps, Found.Length, Found.Loop)
        };
        let mut Edited = Ui.add(egui::DragValue::new(&mut Fps).range(1.0..=120.0).speed(0.2).suffix(" fps")).changed();
        Edited |= Ui.add(egui::DragValue::new(&mut NewLength).range(1..=100_000).prefix("length ")).on_hover_text("Number of frames").changed();
        Edited |= crate::icons::toggle(Ui, &mut Looping, "Loop", "Loop", "Loop playback and export").changed();
        if Edited {
            super::edit_coalesced(App, "RigClipSettings", "Animation settings");
            let Found = &mut App.Project.Rig.Clips[Clip];
            Found.Fps = Fps;
            Found.Length = NewLength.max(1);
            Found.Loop = Looping;
        }
        Ui.separator();
        if crate::icons::text_button(Ui, "Key", "Key").on_hover_text("Key the selected parts at this frame (K)").clicked() {
            super::key_selected(App);
        }
        if crate::icons::button(Ui, "InsertFrame", false, "Insert a frame at the playhead (pushes later keys right)").clicked() {
            super::insert_frames(App, 1);
        }
        if crate::icons::button(Ui, "RemoveFrame", false, "Remove the frame at the playhead (pulls later keys left)").clicked() {
            super::remove_frames(App, 1);
        }
        if crate::icons::enabled_button(Ui, App.Anim.Keys.len() > 1, "Reverse", "Reverse the selected keys in time").clicked() {
            super::reverse_keys(App);
        }
        if crate::icons::enabled_button(Ui, App.Anim.primary().is_some(), "Bake", "Bake constraints of the selection into keys").clicked() {
            super::bake_selected(App);
        }
        let mut Speed = App.Anim.Speed;
        egui::ComboBox::from_id_salt("RigSpeed").selected_text(format!("{Speed}×")).width(56.0).show_ui(Ui, |Ui| {
            for Option in [0.1, 0.25, 0.5, 1.0, 1.5, 2.0, 4.0] {
                Ui.selectable_value(&mut Speed, Option, format!("{Option}×"));
            }
        });
        App.Anim.Speed = Speed;
        if crate::icons::button(Ui, "ZoomOut", false, "Zoom timeline out").clicked() {
            App.Anim.PixelsPerFrame = (App.Anim.PixelsPerFrame / 1.25).max(3.0);
        }
        if crate::icons::button(Ui, "ZoomIn", false, "Zoom timeline in").clicked() {
            App.Anim.PixelsPerFrame = (App.Anim.PixelsPerFrame * 1.25).min(60.0);
        }
        if !App.Anim.Keys.is_empty() {
            Ui.separator();
            Ui.label(RichText::new(format!("{} key(s)", App.Anim.Keys.len())).strong());
            let Current = {
                let Active = &App.Project.Rig.Clips[Clip];
                App.Anim.Keys.first().and_then(|Entry| {
                    let Track = Active.track(Entry.Node, Entry.Channel)?;
                    Track.key_at(Entry.Frame).map(|Index| Track.Keys[Index].Ease)
                })
            };
            egui::ComboBox::from_id_salt("RigKeyEase").selected_text(Current.map(Easing::name).unwrap_or("Easing")).width(130.0).show_ui(Ui, |Ui| {
                for Ease in Easing::All {
                    if Ui.selectable_label(Current == Some(*Ease), Ease.name()).clicked() {
                        super::set_easing(App, *Ease);
                    }
                }
            });
            if let Some(Easing::Bezier { X1, Y1, X2, Y2 }) = Current {
                let (mut A, mut B, mut C, mut D) = (X1, Y1, X2, Y2);
                let mut Bend = false;
                Bend |= Ui.add(egui::DragValue::new(&mut A).range(0.0..=1.0).speed(0.01).prefix("x1 ")).changed();
                Bend |= Ui.add(egui::DragValue::new(&mut B).range(-1.0..=2.0).speed(0.01).prefix("y1 ")).changed();
                Bend |= Ui.add(egui::DragValue::new(&mut C).range(0.0..=1.0).speed(0.01).prefix("x2 ")).changed();
                Bend |= Ui.add(egui::DragValue::new(&mut D).range(-1.0..=2.0).speed(0.01).prefix("y2 ")).changed();
                if Bend {
                    super::set_easing(App, Easing::Bezier { X1: A, Y1: B, X2: C, Y2: D });
                }
            }
            if crate::icons::button(Ui, "Duplicate", false, "Copy keys (Ctrl+C)").clicked() {
                super::copy_keys(App);
            }
            if crate::icons::button(Ui, "Delete", false, "Delete keys (Del)").clicked() {
                super::delete_selected_keys(App);
            }
        }
        if !App.Anim.KeyClipboard.is_empty() && Ui.small_button("Paste keys").on_hover_text("Paste at the playhead (Ctrl+V)").clicked() {
            super::paste_keys(App);
        }
    });
}

pub fn timeline_panel(App: &mut DespiteApp, Ui: &mut Ui) {
    header(App, Ui);
    Ui.separator();
    let Clip = clip_index(App);
    let Length = App.Project.Rig.Clips[Clip].Length.max(1) as f32;
    let Full = Ui.available_rect_before_wrap();
    let NameWidth = if App.Compact { 110.0 } else { 170.0 };
    let TrackLeft = Full.left() + NameWidth;
    let TrackWidth = (Full.width() - NameWidth - 6.0).max(40.0);
    let Ppf = App.Anim.PixelsPerFrame;
    let Visible = TrackWidth / Ppf;
    let MaxStart = (Length - Visible).max(0.0);
    if App.Anim.Playing && (App.Anim.Frame < App.Anim.TimelineStart || App.Anim.Frame > App.Anim.TimelineStart + Visible - 1.0) {
        App.Anim.TimelineStart = (App.Anim.Frame - 1.0).clamp(0.0, MaxStart);
    }
    App.Anim.TimelineStart = App.Anim.TimelineStart.clamp(0.0, MaxStart);
    let Start = App.Anim.TimelineStart;
    let XOf = |Frame: f32| TrackLeft + (Frame - Start) * Ppf + Ppf * 0.5;
    let FrameAt = |X: f32| ((X - TrackLeft) / Ppf + Start - 0.5).round().clamp(0.0, Length - 1.0);
    let (RulerRect, Ruler) = Ui.allocate_exact_size(Vec2::new(Full.width(), 22.0), Sense::click_and_drag());
    let Painter = Ui.painter().clone();
    let Track = Rect::from_min_max(Pos2::new(TrackLeft, RulerRect.top()), Pos2::new(TrackLeft + TrackWidth, RulerRect.bottom()));
    Painter.rect_filled(Track, 2.0, Ui.visuals().extreme_bg_color);
    let Step = [1, 2, 5, 10, 20, 50, 100, 200, 500].into_iter().find(|Step| *Step as f32 * Ppf >= 34.0).unwrap_or(1000);
    let FirstVisible = Start.floor() as i32;
    let LastVisible = (Start + Visible).ceil() as i32;
    let ClipPainter = Painter.with_clip_rect(Track.expand2(Vec2::new(0.0, 1000.0)).intersect(Full));
    for Frame in FirstVisible.max(0)..LastVisible.min(Length as i32) {
        let X = XOf(Frame as f32);
        let Major = Frame % Step == 0;
        ClipPainter.line_segment([Pos2::new(X, Track.bottom() - if Major { 8.0 } else { 3.0 }), Pos2::new(X, Track.bottom())], Stroke::new(1.0, Ui.visuals().weak_text_color()));
        if Major {
            ClipPainter.text(Pos2::new(X + 2.0, Track.top() + 1.0), egui::Align2::LEFT_TOP, format!("{}", Frame + 1), egui::FontId::monospace(10.0), Ui.visuals().text_color());
        }
    }
    Painter.text(Pos2::new(Full.left() + 4.0, RulerRect.center().y), egui::Align2::LEFT_CENTER, App.Project.Rig.Clips[Clip].Name.clone(), egui::FontId::proportional(12.0), Ui.visuals().strong_text_color());
    if let Some(Pointer) = Ruler.interact_pointer_pos()
        && (Ruler.clicked() || Ruler.dragged())
        && Pointer.x >= TrackLeft
    {
        set_frame(App, FrameAt(Pointer.x));
        App.Anim.Playing = false;
    }
    let Scroll = Ui.input(|Input| Input.smooth_scroll_delta);
    if Ui.rect_contains_pointer(Rect::from_min_max(Pos2::new(TrackLeft, Full.top()), Full.max)) {
        if Ui.input(|Input| Input.modifiers.command) && Scroll.y != 0.0 {
            App.Anim.PixelsPerFrame = (App.Anim.PixelsPerFrame * (Scroll.y * 0.003).exp()).clamp(3.0, 60.0);
        } else if Scroll.x != 0.0 {
            App.Anim.TimelineStart = (App.Anim.TimelineStart - Scroll.x / Ppf).clamp(0.0, MaxStart);
        }
    }
    let PlayheadX = XOf(App.Anim.Frame);
    let Accent = Color32::from_rgb(235, 90, 90);
    let Rows = rows(App);
    if Rows.is_empty() {
        Ui.add_space(6.0);
        Ui.label(RichText::new("Select a part or bone to see its keys here. Move or rotate things with Auto-key on, or press Key, to animate.").weak());
    }
    let RowHeight = if App.Compact { 26.0 } else { 20.0 };
    let mut Clicked: Option<(Row, Option<f32>, bool)> = None;
    let mut Double: Option<(Row, f32)> = None;
    let mut DragStart: Option<(Row, f32, f32)> = None;
    let mut Dragging: Option<f32> = None;
    let mut Stopped = false;
    let mut Toggle: Option<(NodeId, Channel)> = None;
    egui::ScrollArea::vertical().id_salt("RigTimelineRows").auto_shrink([false, false]).show(Ui, |Ui| {
        for (RowIndex, Entry) in Rows.iter().enumerate() {
            let (RowRect, RowResponse) = Ui.allocate_exact_size(Vec2::new(Full.width(), RowHeight), Sense::click());
            if !Ui.is_rect_visible(RowRect) {
                continue;
            }
            let Painter = Ui.painter();
            let Lane = Rect::from_min_max(Pos2::new(TrackLeft, RowRect.top()), Pos2::new(TrackLeft + TrackWidth, RowRect.bottom()));
            let Shade = if RowIndex % 2 == 0 { Ui.visuals().faint_bg_color } else { Color32::TRANSPARENT };
            Painter.rect_filled(RowRect, 0.0, Shade);
            let NameRect = Rect::from_min_max(RowRect.min, Pos2::new(TrackLeft - 4.0, RowRect.bottom()));
            match Entry {
                Row::Node(Node) => {
                    let Found = App.Project.Rig.node(*Node);
                    let Name = Found.map(|Item| Item.Name.clone()).unwrap_or_default();
                    let Selected = App.Anim.Selected.contains(Node);
                    let Depth = App.Project.Rig.depth(*Node).min(6) as f32 * 8.0;
                    if Selected {
                        Painter.rect_filled(NameRect, 2.0, Ui.visuals().selection.bg_fill.gamma_multiply(0.6));
                    }
                    if let Some(Item) = Found {
                        crate::icons::paint(Painter, Ui.ctx(), super::node_icon(&Item.Kind), Rect::from_min_size(NameRect.min + Vec2::new(2.0 + Depth, (RowHeight - 14.0) / 2.0), Vec2::splat(14.0)), Ui.visuals().text_color());
                    }
                    Painter.with_clip_rect(NameRect).text(
                        Pos2::new(NameRect.left() + 20.0 + Depth, NameRect.center().y),
                        egui::Align2::LEFT_CENTER,
                        Name,
                        egui::FontId::proportional(13.0),
                        if Selected { Ui.visuals().strong_text_color() } else { Ui.visuals().text_color() },
                    );
                }
                Row::Value(Node, Which) => {
                    let Label = channel_label(App, *Node, *Which);
                    let Has = App.Project.Rig.Clips[Clip].has_key(*Node, *Which, App.Anim.Frame);
                    let Button = Rect::from_min_size(Pos2::new(NameRect.right() - 18.0, NameRect.center().y - 8.0), Vec2::splat(16.0));
                    let ButtonResponse = Ui.interact(Button, Ui.scope_id().with(("RigKeyToggle", Node.0, format!("{Which:?}"))), Sense::click());
                    crate::icons::paint(Painter, Ui.ctx(), if Has { "Key" } else { "KeyEmpty" }, Button, if Has { Accent } else { Ui.visuals().weak_text_color() });
                    if ButtonResponse.on_hover_text(if Has { "Remove the key at this frame" } else { "Add a key at this frame" }).clicked() {
                        Toggle = Some((*Node, *Which));
                    }
                    Painter.with_clip_rect(NameRect.with_max_x(Button.left() - 2.0)).text(
                        Pos2::new(NameRect.left() + 26.0, NameRect.center().y),
                        egui::Align2::LEFT_CENTER,
                        Label,
                        egui::FontId::proportional(12.0),
                        Ui.visuals().weak_text_color(),
                    );
                }
            }
            let LanePainter = Painter.with_clip_rect(Lane);
            let Keys = keys_in_row(App, *Entry);
            for Pair in Keys.windows(2) {
                let (Left, Right) = (Pair[0], Pair[1]);
                let Y = Lane.center().y;
                let Stroke = Stroke::new(if matches!(Entry, Row::Node(_)) { 2.0 } else { 1.0 }, Ui.visuals().weak_text_color().gamma_multiply(0.6));
                if Left.1 == Easing::Constant {
                    LanePainter.extend(Shape::dashed_line(&[Pos2::new(XOf(Left.0), Y), Pos2::new(XOf(Right.0), Y)], Stroke, 3.0, 3.0));
                } else {
                    LanePainter.line_segment([Pos2::new(XOf(Left.0), Y), Pos2::new(XOf(Right.0), Y)], Stroke);
                }
            }
            for (Frame, Ease) in Keys {
                let Center = Pos2::new(XOf(Frame), Lane.center().y);
                if Center.x < Lane.left() - 8.0 || Center.x > Lane.right() + 8.0 {
                    continue;
                }
                let Size = if matches!(Entry, Row::Node(_)) { 6.0 } else { 5.0 };
                let Hit = Rect::from_center_size(Center, Vec2::new(Ppf.max(12.0), RowHeight));
                let Response = Ui.interact(Hit, Ui.scope_id().with(("RigKey", RowIndex, (Frame * 100.0) as i64)), Sense::click_and_drag());
                let Refs = refs_for(App, *Entry, Frame);
                let Chosen = is_selected(App, &Refs);
                let Fill = if Chosen {
                    Color32::from_rgb(255, 210, 80)
                } else if Ease == Easing::Constant {
                    Color32::from_rgb(170, 170, 190)
                } else if matches!(Entry, Row::Node(_)) {
                    Color32::from_rgb(220, 220, 230)
                } else {
                    Color32::from_rgb(150, 200, 255)
                };
                let Diamond = vec![Center + Vec2::new(0.0, -Size), Center + Vec2::new(Size, 0.0), Center + Vec2::new(0.0, Size), Center + Vec2::new(-Size, 0.0)];
                LanePainter.add(Shape::convex_polygon(Diamond, Fill, Stroke::new(1.0, Color32::from_black_alpha(200))));
                if Response.double_clicked() {
                    Double = Some((*Entry, Frame));
                } else if Response.clicked() {
                    Clicked = Some((*Entry, Some(Frame), Ui.input(|Input| Input.modifiers.shift)));
                }
                if Response.drag_started()
                    && let Some(Pointer) = Response.interact_pointer_pos()
                {
                    DragStart = Some((*Entry, Frame, FrameAt(Pointer.x)));
                }
                if Response.dragged()
                    && let Some(Pointer) = Response.interact_pointer_pos()
                {
                    Dragging = Some(FrameAt(Pointer.x));
                }
                if Response.drag_stopped() {
                    Stopped = true;
                }
                Response.context_menu(|Ui| {
                    Ui.label(RichText::new(format!("Key at frame {}", Frame as u32 + 1)).strong());
                    Ui.menu_button("Easing", |Ui| {
                        for Option in Easing::All {
                            if Ui.button(Option.name()).clicked() {
                                if !is_selected(App, &Refs) {
                                    App.Anim.Keys = Refs.clone();
                                }
                                super::set_easing(App, *Option);
                                Ui.close();
                            }
                        }
                    });
                    if Ui.button("Copy").clicked() {
                        if !is_selected(App, &Refs) {
                            App.Anim.Keys = Refs.clone();
                        }
                        super::copy_keys(App);
                        Ui.close();
                    }
                    if Ui.button("Delete").clicked() {
                        if !is_selected(App, &Refs) {
                            App.Anim.Keys = Refs.clone();
                        }
                        super::delete_selected_keys(App);
                        Ui.close();
                    }
                });
            }
            if PlayheadX >= Lane.left() && PlayheadX <= Lane.right() {
                Painter.line_segment([Pos2::new(PlayheadX, RowRect.top()), Pos2::new(PlayheadX, RowRect.bottom())], Stroke::new(1.5, Accent));
            }
            if RowResponse.double_clicked()
                && let Some(Pointer) = RowResponse.interact_pointer_pos()
                && Pointer.x >= TrackLeft
            {
                Double = Some((*Entry, FrameAt(Pointer.x)));
            } else if RowResponse.clicked()
                && let Some(Pointer) = RowResponse.interact_pointer_pos()
            {
                if Pointer.x >= TrackLeft {
                    Clicked = Some((*Entry, None, false));
                    set_frame(App, FrameAt(Pointer.x));
                    App.Anim.Playing = false;
                } else if let Row::Node(Node) = Entry {
                    let Add = Ui.input(|Input| Input.modifiers.shift || Input.modifiers.command);
                    App.Anim.select(Some(*Node), Add);
                }
            }
        }
        Ui.add_space(8.0);
    });
    if PlayheadX >= TrackLeft && PlayheadX <= TrackLeft + TrackWidth {
        let Top = Pos2::new(PlayheadX, RulerRect.top());
        Painter.add(Shape::convex_polygon(vec![Top, Top + Vec2::new(5.0, 0.0), Top + Vec2::new(0.0, 7.0), Top + Vec2::new(-5.0, 0.0)], Accent, Stroke::NONE));
        Painter.line_segment([Top, Pos2::new(PlayheadX, RulerRect.bottom())], Stroke::new(1.5, Accent));
    }
    if let Some((Node, Which)) = Toggle {
        toggle_key(App, Node, Which);
    }
    if let Some((Entry, Frame, Additive)) = Clicked {
        match Frame {
            Some(At) => {
                let Refs = refs_for(App, Entry, At);
                if Additive {
                    if is_selected(App, &Refs) {
                        App.Anim.Keys.retain(|Chosen| !Refs.iter().any(|Entry| Entry.Node == Chosen.Node && Entry.Channel == Chosen.Channel && (Entry.Frame - Chosen.Frame).abs() < 0.01));
                    } else {
                        App.Anim.Keys.extend(Refs);
                    }
                } else {
                    App.Anim.Keys = Refs;
                }
                set_frame(App, At);
                App.Anim.Playing = false;
            }
            None => App.Anim.Keys.clear(),
        }
    }
    if let Some((Entry, At)) = Double {
        set_frame(App, At);
        match Entry {
            Row::Value(Node, Which) => {
                if !App.Project.Rig.Clips[Clip].has_key(Node, Which, At) {
                    toggle_key(App, Node, Which);
                }
            }
            Row::Node(Node) => {
                App.Anim.select(Some(Node), false);
                super::key_selected(App);
            }
        }
    }
    if let Some((Entry, At, Pointer)) = DragStart {
        let Refs = refs_for(App, Entry, At);
        if !is_selected(App, &Refs) {
            App.Anim.Keys = Refs;
        }
        App.record("Move keys");
        App.Anim.KeyDrag = Some((Pointer, App.Anim.Keys.clone()));
        App.Anim.KeyMoved = 0.0;
    }
    if let (Some(Pointer), Some((Origin, Originals))) = (Dragging, App.Anim.KeyDrag.clone()) {
        let Offset = Pointer - Origin;
        if (Offset - App.Anim.KeyMoved).abs() > 0.01 {
            let Previous = App.Anim.KeyMoved;
            move_keys(App, &Originals, Previous, Offset);
            App.Anim.KeyMoved = Offset;
            set_frame(App, Pointer);
        }
    }
    if Stopped {
        App.Anim.KeyDrag = None;
        App.Anim.KeyMoved = 0.0;
    }
}

pub fn key_button(App: &mut DespiteApp, Ui: &mut Ui, Node: NodeId, Which: Channel) {
    let Clip = &App.Project.Rig.Clips[clip_index(App)];
    let Has = Clip.has_key(Node, Which, App.Anim.Frame);
    let Animated = Clip.track(Node, Which).is_some_and(|Track| !Track.Keys.is_empty());
    let (Icon, Tint, Tip) = if Has {
        ("Key", Color32::from_rgb(235, 90, 90), "Keyed here: click to remove this key")
    } else if Animated {
        ("KeyEmpty", Color32::from_rgb(235, 160, 90), "Animated: click to add a key here")
    } else {
        ("KeyEmpty", Ui.visuals().weak_text_color(), "Click to add a key at this frame")
    };
    let Size = if App.Compact { 22.0 } else { 16.0 };
    let (Rect, Response) = Ui.allocate_exact_size(Vec2::splat(Size), Sense::click());
    crate::icons::paint(Ui.painter(), Ui.ctx(), Icon, Rect, Tint);
    if Response.on_hover_text(Tip).clicked() {
        toggle_key(App, Node, Which);
    }
}
