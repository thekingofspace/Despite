use std::sync::Arc;

use despite_core::animation::{self as rig, Channel, KeyValue, NodeId, NodeKind};
use despite_core::{ImageId, Raster};
use egui::{RichText, Sense, Ui, Vec2};

use super::{edit, node_icon};
use crate::app::DespiteApp;

fn visible_rows(App: &DespiteApp) -> Vec<(usize, usize)> {
    let Rig = &App.Project.Rig;
    let mut Out = Vec::new();
    let mut Hidden: Vec<NodeId> = Vec::new();
    for Index in Rig.preorder() {
        let Found = &Rig.Nodes[Index];
        if Found.Parent.is_some_and(|Parent| Hidden.contains(&Parent) || App.Anim.Collapsed.contains(&Parent)) {
            Hidden.push(Found.Id);
            continue;
        }
        Out.push((Index, Rig.depth(Found.Id)));
    }
    Out
}

fn reparent(App: &mut DespiteApp, Node: NodeId, Parent: Option<NodeId>) {
    if Parent == App.Project.Rig.node(Node).and_then(|Found| Found.Parent) {
        return;
    }
    if Parent.is_some_and(|Target| Target == Node || App.Project.Rig.is_descendant(Target, Node)) {
        App.set_status("A part can't be placed inside its own child");
        return;
    }
    edit(App, "Reparent");
    rig::reparent_keep_world(&mut App.Project, Node, Parent);
    if let Some(Target) = Parent {
        App.Anim.Collapsed.remove(&Target);
    }
}

pub fn hierarchy_panel(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.horizontal_wrapped(|Ui| {
        Ui.label(RichText::new("Rig").strong());
        let Add = crate::icons::image(Ui, "Add", Ui.visuals().text_color());
        match Add {
            Some(Icon) => {
                Ui.menu_image_button(Icon, |Ui| super::add_node_menu(App, Ui)).response.on_hover_text("Add a group, bone, controller or effect");
            }
            None => {
                Ui.menu_button("+", |Ui| super::add_node_menu(App, Ui));
            }
        }
        let HasNode = App.Anim.primary().is_some();
        if crate::icons::enabled_button(Ui, HasNode, "Duplicate", "Duplicate (Ctrl+D)").clicked() {
            super::duplicate_selected(App);
        }
        if crate::icons::enabled_button(Ui, HasNode, "Delete", "Delete with its children").clicked() {
            super::delete_selected_nodes(App);
        }
        if crate::icons::enabled_button(Ui, HasNode, "ArrowUp", "Draw earlier among siblings").clicked()
            && let Some(Node) = App.Anim.primary()
        {
            edit(App, "Reorder");
            App.Project.Rig.move_sibling(Node, true);
        }
        if crate::icons::enabled_button(Ui, HasNode, "ArrowDown", "Draw later among siblings").clicked()
            && let Some(Node) = App.Anim.primary()
        {
            edit(App, "Reorder");
            App.Project.Rig.move_sibling(Node, false);
        }
    });
    let RowHeight = if App.Compact { 34.0 } else { 24.0 };
    let mut DropOnto: Option<(NodeId, Option<NodeId>)> = None;
    let mut Select: Option<(NodeId, bool)> = None;
    let mut Toggle: Option<(NodeId, bool)> = None;
    let mut Lock: Option<NodeId> = None;
    let mut Collapse: Option<NodeId> = None;
    let mut StartRename: Option<NodeId> = None;
    for (Index, Depth) in visible_rows(App) {
        let Found = App.Project.Rig.Nodes[Index].clone();
        let Selected = App.Anim.Selected.contains(&Found.Id);
        let HasChildren = App.Project.Rig.Nodes.iter().any(|Other| Other.Parent == Some(Found.Id));
        let Shown = App.Anim.Pose.as_ref().and_then(|Pose| Pose.States.get(Index)).map(|State| State.Visible).unwrap_or(Found.Visible);
        let Row = Ui.horizontal(|Ui| {
            Ui.set_min_height(RowHeight);
            Ui.add_space(Depth.min(10) as f32 * 12.0);
            if HasChildren {
                let Closed = App.Anim.Collapsed.contains(&Found.Id);
                let (Spot, Toggle) = Ui.allocate_exact_size(Vec2::new(16.0, RowHeight), Sense::click());
                let Center = Spot.center();
                let Points = if Closed {
                    vec![Center + Vec2::new(-3.0, -5.0), Center + Vec2::new(4.0, 0.0), Center + Vec2::new(-3.0, 5.0)]
                } else {
                    vec![Center + Vec2::new(-5.0, -3.0), Center + Vec2::new(5.0, -3.0), Center + Vec2::new(0.0, 4.0)]
                };
                let Paint = if Toggle.hovered() { Ui.visuals().strong_text_color() } else { Ui.visuals().text_color() };
                Ui.painter().add(egui::Shape::convex_polygon(Points, Paint, egui::Stroke::NONE));
                if Toggle.on_hover_text(if Closed { "Show children" } else { "Hide children" }).clicked() {
                    Collapse = Some(Found.Id);
                }
            } else {
                Ui.add_space(18.0);
            }
            crate::icons::show(Ui, node_icon(&Found.Kind));
            let mut Visible = Shown;
            if crate::icons::row_toggle(Ui, &mut Visible, "Visible", "Hidden", "Show or hide (keyed if this value is animated)").changed() {
                Toggle = Some((Found.Id, Visible));
            }
            let mut Locked = Found.Locked;
            if crate::icons::row_toggle(Ui, &mut Locked, "Locked", "Unlocked", "Lock: can't be picked on the stage").changed() {
                Lock = Some(Found.Id);
            }
            let Renaming = App.Anim.Rename.as_ref().is_some_and(|(Id, _)| *Id == Found.Id);
            if Renaming {
                let mut Commit = false;
                if let Some((_, Text)) = &mut App.Anim.Rename {
                    let Field = Ui.add(egui::TextEdit::singleline(Text).desired_width(120.0));
                    Field.request_focus();
                    Commit = Field.lost_focus();
                }
                if Commit && let Some((Id, Text)) = App.Anim.Rename.take()
                    && !Text.trim().is_empty()
                {
                    edit(App, "Rename");
                    if let Some(Target) = App.Project.Rig.node_mut(Id) {
                        Target.Name = Text.trim().to_string();
                    }
                }
            } else {
                let Label = Ui.add(egui::Button::selectable(Selected, RichText::new(&Found.Name)).truncate().sense(Sense::click_and_drag()));
                if Label.clicked() {
                    Select = Some((Found.Id, Ui.input(|Input| Input.modifiers.shift || Input.modifiers.command)));
                }
                if Label.double_clicked() {
                    StartRename = Some(Found.Id);
                }
                Label.dnd_set_drag_payload(Found.Id);
            }
        });
        let Response = Row.response.interact(Sense::hover());
        if let Some(Payload) = Response.dnd_release_payload::<NodeId>() {
            DropOnto = Some((*Payload, Some(Found.Id)));
        }
        if Response.dnd_hover_payload::<NodeId>().is_some() {
            Ui.painter().rect_stroke(Response.rect, 3.0, egui::Stroke::new(1.5, Ui.visuals().selection.stroke.color), egui::StrokeKind::Inside);
        }
    }
    let (Zone, ZoneResponse) = Ui.allocate_exact_size(Vec2::new((Ui.available_width() - 12.0).max(120.0), 26.0), Sense::hover());
    if ZoneResponse.dnd_hover_payload::<NodeId>().is_some() {
        Ui.painter().rect_stroke(Zone, 3.0, egui::Stroke::new(1.5, Ui.visuals().selection.stroke.color), egui::StrokeKind::Inside);
        Ui.painter().text(Zone.center(), egui::Align2::CENTER_CENTER, "Drop here to move to the top level", egui::FontId::proportional(12.0), Ui.visuals().text_color());
    } else if !App.Project.Rig.Nodes.is_empty() {
        Ui.painter().text(Zone.center(), egui::Align2::CENTER_CENTER, "Drag a row onto another to make it a child", egui::FontId::proportional(11.0), Ui.visuals().weak_text_color());
    }
    if let Some(Payload) = ZoneResponse.dnd_release_payload::<NodeId>() {
        DropOnto = Some((*Payload, None));
    }
    if let Some((Node, Parent)) = DropOnto {
        reparent(App, Node, Parent);
    }
    if let Some((Node, Add)) = Select {
        App.Anim.select(Some(Node), Add);
    }
    if let Some(Node) = StartRename {
        let Name = App.Project.Rig.node(Node).map(|Found| Found.Name.clone()).unwrap_or_default();
        App.Anim.Rename = Some((Node, Name));
    }
    if let Some(Node) = Collapse
        && !App.Anim.Collapsed.remove(&Node)
    {
        App.Anim.Collapsed.insert(Node);
    }
    if let Some((Node, Visible)) = Toggle {
        edit(App, "Visibility");
        let Frame = App.Anim.Frame;
        rig::set_value(&mut App.Project, Node, Channel::Visible, KeyValue::Flag(Visible), Frame, false);
    }
    if let Some(Node) = Lock {
        edit(App, "Lock");
        if let Some(Found) = App.Project.Rig.node_mut(Node) {
            Found.Locked = !Found.Locked;
        }
    }
}

fn thumbnail(App: &mut DespiteApp, Ui: &Ui, Image: ImageId) -> Option<egui::TextureHandle> {
    let Found = App.Project.image(Image)?;
    let Stamp = Found.Layers.iter().fold(Found.Layers.len() as u64, |Hash, Entry| Hash.wrapping_mul(31).wrapping_add(Arc::as_ptr(&Entry.Pixels) as u64));
    if let Some((Texture, Built)) = App.Anim.Thumbs.get(&Image)
        && *Built == Stamp
    {
        return Some(Texture.clone());
    }
    let Full = Found.composite();
    let Longest = Full.width().max(Full.height()).max(1);
    let Small: Raster = if Longest > 64 {
        let Width = ((Full.width() * 64) / Longest).max(1);
        let Height = ((Full.height() * 64) / Longest).max(1);
        if Found.PixelArt { Full.scaled_nearest(Width, Height) } else { Full.scaled_smooth(Width, Height) }
    } else {
        Full
    };
    let Texture = crate::view::raster_texture(Ui.ctx(), &format!("RigAsset{}", Image.0), &Small, !Found.PixelArt);
    App.Anim.Thumbs.insert(Image, (Texture.clone(), Stamp));
    Some(Texture)
}

pub fn place_asset(App: &mut DespiteApp, Image: ImageId) {
    let Center = (App.Project.Rig.Width as f32 / 2.0, App.Project.Rig.Height as f32 / 2.0);
    let Parent = App.Anim.primary().filter(|Id| App.Project.Rig.node(*Id).is_some_and(|Found| matches!(Found.Kind, NodeKind::Group | NodeKind::Bone { .. })));
    edit(App, "Add part");
    if let Some(Created) = rig::add_sprite(&mut App.Project, Image, Parent, Center.0, Center.1) {
        App.Anim.select(Some(Created), false);
    }
}

pub fn assets_panel(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.horizontal_wrapped(|Ui| {
        Ui.label(RichText::new("Part images").strong());
        if crate::icons::text_button(Ui, "Import", "Images…").on_hover_text("Add PNG/JPEG/GIF/WebP files as parts").clicked() {
            super::import_assets_dialog(App);
        }
        Ui.menu_button("From Art project", |Ui| super::art_project_menu(App, Ui)).response.on_hover_text("Bring in an Art project's layers as separate parts, already positioned");
    });
    let Images: Vec<(ImageId, String, bool)> = App.Project.Images.iter().map(|Found| (Found.Id, Found.Name.clone(), App.Project.Rig.uses_image(Found.Id))).collect();
    if Images.is_empty() {
        return;
    }
    let Side = if App.Compact { 64.0 } else { 52.0 };
    let mut Place = None;
    let mut Remove = None;
    let Columns = ((Ui.available_width() + 4.0) / (Side + 14.0)).floor().max(1.0) as usize;
    for Row in Images.chunks(Columns) {
        Ui.horizontal(|Ui| {
        for (Image, Name, Used) in Row.iter().cloned() {
            let Texture = thumbnail(App, Ui, Image);
            Ui.vertical(|Ui| {
                Ui.set_width(Side + 8.0);
                let Response = match &Texture {
                    Some(Found) => {
                        let Size = Found.size_vec2();
                        let Scale = (Side / Size.x.max(Size.y)).min(4.0);
                        Ui.add(egui::Button::image(egui::Image::new(egui::load::SizedTexture::new(Found.id(), Size * Scale))).sense(Sense::click_and_drag()))
                    }
                    None => Ui.add(egui::Button::new(&Name).sense(Sense::click_and_drag())),
                };
                let Response = Response.on_hover_text(format!("{Name}{}", if Used { " (used in the rig)" } else { "" }));
                if Response.drag_started() {
                    App.Anim.DragAsset = Some(Image);
                }
                if Response.clicked() {
                    Place = Some(Image);
                }
                Response.context_menu(|Ui| {
                    if Ui.button("Place on stage").clicked() {
                        Place = Some(Image);
                        Ui.close();
                    }
                    if Ui.add_enabled(!Used, egui::Button::new("Delete image")).on_disabled_hover_text("Remove the parts that use it first").clicked() {
                        Remove = Some(Image);
                        Ui.close();
                    }
                });
                Ui.add(egui::Label::new(RichText::new(&Name).small()).truncate());
            });
        }
        });
    }
    if let Some(Image) = Place {
        place_asset(App, Image);
    }
    if let Some(Image) = Remove {
        edit(App, "Delete part image");
        App.Project.remove_image(Image);
        App.Anim.Thumbs.remove(&Image);
    }
}
