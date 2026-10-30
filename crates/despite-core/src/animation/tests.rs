use super::*;
use crate::color::Color;
use crate::project::{Project, ProjectKind};
use crate::raster::Raster;

fn close(Left: f32, Right: f32, Tolerance: f32) -> bool {
    (Left - Right).abs() <= Tolerance
}

fn project() -> Project {
    let mut Created = Project::new_of_kind("Rig", ProjectKind::Animation);
    Created.Rig = Rig::new(64, 64);
    Created
}

fn square(Owner: &mut Project, Name: &str, Size: u32, Pixel: [u8; 4]) -> crate::image::ImageId {
    add_image_asset(Owner, Name, Raster::filled(Size, Size, Pixel))
}

fn world_origin(Owner: &Project, Id: NodeId, Frame: f32) -> (f32, f32) {
    let Solver = Solver::new(&Owner.Rig);
    let Pose = Solver.evaluate(Owner.Rig.active_clip(), Frame);
    let Index = Solver.index(Some(Id)).expect("node exists");
    pose::origin_of(&Pose.World[Index])
}

#[test]
fn forward_kinematics_follow_parent_rotation() {
    let mut Owner = project();
    let Arm = add_bone(&mut Owner, None, 10.0, 10.0, 30.0, 10.0);
    let Hand = add_bone(&mut Owner, Some(Arm), 30.0, 10.0, 40.0, 10.0);
    let (X, Y) = world_origin(&Owner, Hand, 0.0);
    assert!(close(X, 30.0, 0.01) && close(Y, 10.0, 0.01), "{X} {Y}");
    let Clip = Owner.Rig.active_clip_mut().unwrap();
    Clip.set_key(Arm, Channel::Rotation, 0.0, KeyValue::Number(0.0));
    Clip.set_key(Arm, Channel::Rotation, 10.0, KeyValue::Number(90.0));
    let (X, Y) = world_origin(&Owner, Hand, 10.0);
    assert!(close(X, 10.0, 0.05) && close(Y, 30.0, 0.05), "{X} {Y}");
    let (X, Y) = world_origin(&Owner, Hand, 5.0);
    let Expected = 45f32.to_radians();
    assert!(close(X, 10.0 + 20.0 * Expected.cos(), 0.05) && close(Y, 10.0 + 20.0 * Expected.sin(), 0.05), "{X} {Y}");
}

#[test]
fn look_at_aims_and_respects_limits() {
    let mut Owner = project();
    let Head = new_node(&mut Owner, "Head", NodeKind::Group, None);
    let Target = new_node(&mut Owner, "Target", NodeKind::Group, None);
    Owner.Rig.node_mut(Head).unwrap().Rest.X = 10.0;
    Owner.Rig.node_mut(Head).unwrap().Rest.Y = 10.0;
    Owner.Rig.node_mut(Target).unwrap().Rest.X = 10.0;
    Owner.Rig.node_mut(Target).unwrap().Rest.Y = 40.0;
    Owner.Rig.node_mut(Head).unwrap().Constraints.push(Constraint::LookAt {
        Target: Some(Target),
        Mix: 1.0,
        Offset: 0.0,
        UseLimits: false,
        LimitMin: -30.0,
        LimitMax: 30.0,
        Slide: 0.0,
    });
    let Solver = Solver::new(&Owner.Rig);
    let Pose = Solver.evaluate(None, 0.0);
    assert!(close(pose::angle_of(&Pose.World[0]), 90.0, 0.01));
    if let Constraint::LookAt { UseLimits, .. } = &mut Owner.Rig.node_mut(Head).unwrap().Constraints[0] {
        *UseLimits = true;
    }
    let Pose = Solver::new(&Owner.Rig).evaluate(None, 0.0);
    assert!(close(pose::angle_of(&Pose.World[0]), 30.0, 0.01));
    let Solver = Solver::new(&Owner.Rig);
    let Head = Solver.index(Some(Head)).unwrap();
    assert!(Pose.States[Head].Visible);
}

#[test]
fn slide_moves_pupil_toward_target() {
    let mut Owner = project();
    let Eye = new_node(&mut Owner, "Eye", NodeKind::Group, None);
    let Pupil = new_node(&mut Owner, "Pupil", NodeKind::Group, Some(Eye));
    let Target = new_node(&mut Owner, "Target", NodeKind::Group, None);
    Owner.Rig.node_mut(Eye).unwrap().Rest.X = 20.0;
    Owner.Rig.node_mut(Eye).unwrap().Rest.Y = 20.0;
    Owner.Rig.node_mut(Target).unwrap().Rest.X = 60.0;
    Owner.Rig.node_mut(Target).unwrap().Rest.Y = 20.0;
    Owner.Rig.node_mut(Pupil).unwrap().Constraints.push(Constraint::LookAt {
        Target: Some(Target),
        Mix: 1.0,
        Offset: 0.0,
        UseLimits: false,
        LimitMin: 0.0,
        LimitMax: 0.0,
        Slide: 3.0,
    });
    let (X, Y) = world_origin(&Owner, Pupil, 0.0);
    assert!(close(X, 23.0, 0.01) && close(Y, 20.0, 0.01), "{X} {Y}");
}

#[test]
fn two_bone_ik_reaches_target() {
    let mut Owner = project();
    let Upper = add_bone(&mut Owner, None, 10.0, 32.0, 30.0, 32.0);
    let Lower = add_bone(&mut Owner, Some(Upper), 30.0, 32.0, 50.0, 32.0);
    let Goal = new_node(&mut Owner, "Goal", NodeKind::Group, None);
    Owner.Rig.node_mut(Goal).unwrap().Rest.X = 30.0;
    Owner.Rig.node_mut(Goal).unwrap().Rest.Y = 50.0;
    Owner.Rig.node_mut(Lower).unwrap().Constraints.push(Constraint::TwoBoneIk { Target: Some(Goal), BendPositive: true, Mix: 1.0 });
    let Solver = Solver::new(&Owner.Rig);
    let Pose = Solver.evaluate(None, 0.0);
    let LowerIndex = Solver.index(Some(Lower)).unwrap();
    let (TipX, TipY) = pose::tip_of(&Pose.World[LowerIndex], &Owner.Rig.Nodes[LowerIndex]);
    assert!(close(TipX, 30.0, 0.1) && close(TipY, 50.0, 0.1), "{TipX} {TipY}");
    let (ElbowX, ElbowY) = pose::origin_of(&Pose.World[LowerIndex]);
    let UpperLength = ((ElbowX - 10.0).powi(2) + (ElbowY - 32.0).powi(2)).sqrt();
    assert!(close(UpperLength, 20.0, 0.05), "{UpperLength}");
}

#[test]
fn stretch_to_squashes_with_volume() {
    let mut Owner = project();
    let Spine = add_bone(&mut Owner, None, 0.0, 0.0, 10.0, 0.0);
    let Goal = new_node(&mut Owner, "Goal", NodeKind::Group, None);
    Owner.Rig.node_mut(Goal).unwrap().Rest.X = 15.0;
    Owner.Rig.node_mut(Spine).unwrap().Constraints.push(Constraint::StretchTo { Target: Some(Goal), MaxStretch: 1.0, PreserveVolume: true, Mix: 1.0 });
    let Pose = Solver::new(&Owner.Rig).evaluate(None, 0.0);
    let World = Pose.World[0];
    let ScaleX = (World.A * World.A + World.B * World.B).sqrt();
    let ScaleY = (World.C * World.C + World.D * World.D).sqrt();
    assert!(close(ScaleX, 1.5, 0.01) && close(ScaleY, 1.0 / 1.5, 0.01), "{ScaleX} {ScaleY}");
    Owner.Rig.node_mut(Goal).unwrap().Rest.X = 100.0;
    let Pose = Solver::new(&Owner.Rig).evaluate(None, 0.0);
    let ScaleX = (Pose.World[0].A.powi(2) + Pose.World[0].B.powi(2)).sqrt();
    assert!(close(ScaleX, 2.0, 0.01), "{ScaleX}");
}

#[test]
fn stretch_channel_preserves_area() {
    let mut Owner = project();
    let Body = new_node(&mut Owner, "Body", NodeKind::Group, None);
    Owner.Rig.active_clip_mut().unwrap().set_key(Body, Channel::Stretch, 0.0, KeyValue::Number(1.0));
    let Pose = Solver::new(&Owner.Rig).evaluate(Owner.Rig.active_clip(), 0.0);
    let World = Pose.World[0];
    let Area = World.A * World.D - World.B * World.C;
    assert!(close(Area, 1.0, 1e-4) && close(World.A, 2.0, 1e-4));
}

#[test]
fn driver_and_visibility_logic() {
    let mut Owner = project();
    let Head = new_node(&mut Owner, "Head", NodeKind::Group, None);
    let Front = new_node(&mut Owner, "Front face", NodeKind::Group, Some(Head));
    let Side = new_node(&mut Owner, "Side face", NodeKind::Group, Some(Head));
    Owner.Rig.node_mut(Side).unwrap().Constraints.push(Constraint::Driver {
        Source: Some(Head),
        From: Channel::Rotation,
        To: Channel::Visible,
        Mode: DriverMode::Threshold { Threshold: 30.0, Above: true },
    });
    Owner.Rig.node_mut(Head).unwrap().Constraints.push(Constraint::Visibility { Targets: vec![Front], From: Channel::Rotation, Min: -30.0, Max: 30.0, Invert: false });
    let Clip = Owner.Rig.active_clip_mut().unwrap();
    Clip.set_key(Head, Channel::Rotation, 0.0, KeyValue::Number(0.0));
    Clip.set_key(Head, Channel::Rotation, 10.0, KeyValue::Number(60.0));
    let Solver = Solver::new(&Owner.Rig);
    let (FrontIndex, SideIndex) = (Solver.index(Some(Front)).unwrap(), Solver.index(Some(Side)).unwrap());
    let Start = Solver.evaluate(Owner.Rig.active_clip(), 0.0);
    assert!(Start.Shown[FrontIndex] && !Start.Shown[SideIndex]);
    let End = Solver.evaluate(Owner.Rig.active_clip(), 10.0);
    assert!(!End.Shown[FrontIndex] && End.Shown[SideIndex]);
}

#[test]
fn controller_drives_switch() {
    let mut Owner = project();
    let Mouth = new_node(&mut Owner, "Mouth", NodeKind::Group, None);
    let Shapes: Vec<NodeId> = ["A", "E", "O"].iter().map(|Name| new_node(&mut Owner, Name, NodeKind::Group, Some(Mouth))).collect();
    let Talk = new_node(&mut Owner, "Talk", NodeKind::Controller { Min: 0.0, Max: 2.0 }, None);
    let Found = Owner.Rig.node_mut(Mouth).unwrap();
    Found.Constraints.push(Constraint::Switch);
    Found.Constraints.push(Constraint::Driver { Source: Some(Talk), From: Channel::Value, To: Channel::Switch, Mode: DriverMode::Copy });
    Owner.Rig.active_clip_mut().unwrap().set_key(Talk, Channel::Value, 0.0, KeyValue::Number(2.0));
    let Solver = Solver::new(&Owner.Rig);
    let Pose = Solver.evaluate(Owner.Rig.active_clip(), 0.0);
    let Visible: Vec<bool> = Shapes.iter().map(|Id| Pose.Shown[Solver.index(Some(*Id)).unwrap()]).collect();
    assert_eq!(Visible, vec![false, false, true]);
}

#[test]
fn jiggle_lags_then_settles() {
    let mut Owner = project();
    let Root = new_node(&mut Owner, "Root", NodeKind::Group, None);
    let Tail = new_node(&mut Owner, "Tail", NodeKind::Group, Some(Root));
    Owner.Rig.node_mut(Tail).unwrap().Constraints.push(Constraint::Jiggle { Stiffness: 0.2, Damping: 0.3, Gravity: 0.0, Mix: 1.0 });
    let Clip = Owner.Rig.active_clip_mut().unwrap();
    Clip.Loop = false;
    Clip.Length = 60;
    Clip.set_key(Root, Channel::PositionX, 0.0, KeyValue::Number(0.0));
    Clip.set_key(Root, Channel::PositionX, 1.0, KeyValue::Number(20.0));
    let (Early, _) = world_origin(&Owner, Tail, 2.0);
    assert!(Early < 19.0, "{Early}");
    let (Late, _) = world_origin(&Owner, Tail, 59.0);
    assert!(close(Late, 20.0, 0.5), "{Late}");
}

fn red_sprite_project() -> (Project, NodeId) {
    let mut Owner = project();
    let Image = square(&mut Owner, "Red", 10, [255, 0, 0, 255]);
    let Sprite = add_sprite(&mut Owner, Image, None, 32.0, 32.0).unwrap();
    (Owner, Sprite)
}

fn render_now(Owner: &Project) -> Raster {
    let Sprites = SpriteCache::build(Owner);
    render_frame(Owner, Some(0), 0.0, &Sprites, None)
}

#[test]
fn renders_sprite_with_opacity_and_tint() {
    let (mut Owner, Sprite) = red_sprite_project();
    let Out = render_now(&Owner);
    assert_eq!(Out.get(32, 32), [255, 0, 0, 255]);
    assert_eq!(Out.get(5, 5)[3], 0);
    let Found = Owner.Rig.node_mut(Sprite).unwrap();
    Found.Opacity = 0.5;
    Found.Tint = Color::rgb(0.0, 1.0, 1.0);
    let Out = render_now(&Owner);
    let Pixel = Out.get(32, 32);
    assert!(Pixel[0] < 5 && (120..=135).contains(&Pixel[3]), "{Pixel:?}");
}

#[test]
fn clip_children_and_masks() {
    let (mut Owner, Body) = red_sprite_project();
    let Blue = square(&mut Owner, "Blue", 30, [0, 0, 255, 255]);
    let Shade = add_sprite(&mut Owner, Blue, Some(Body), 32.0, 32.0).unwrap();
    let Out = render_now(&Owner);
    assert_eq!(Out.get(20, 32), [0, 0, 255, 255]);
    Owner.Rig.node_mut(Body).unwrap().ClipChildren = true;
    let Out = render_now(&Owner);
    assert_eq!(Out.get(20, 32)[3], 0);
    assert_eq!(Out.get(32, 32), [0, 0, 255, 255]);
    Owner.Rig.node_mut(Body).unwrap().ClipChildren = false;
    let Green = square(&mut Owner, "Hole", 4, [0, 255, 0, 255]);
    let Hole = add_sprite(&mut Owner, Green, None, 20.0, 32.0).unwrap();
    Owner.Rig.node_mut(Hole).unwrap().Visible = false;
    let Found = Owner.Rig.node_mut(Shade).unwrap();
    Found.MaskBy = Some(Hole);
    Found.InvertMask = true;
    let Out = render_now(&Owner);
    assert_eq!(Out.get(20, 32)[3], 0);
    assert_eq!(Out.get(25, 32), [0, 0, 255, 255]);
}

#[test]
fn effect_params_are_keyable() {
    let (mut Owner, Body) = red_sprite_project();
    let Blur = add_effect(&mut Owner, EffectKind::Blur, None, Some(vec![0.0]));
    let Sharp = render_now(&Owner);
    let Clip = Owner.Rig.active_clip_mut().unwrap();
    Clip.set_key(Blur, Channel::Param(0), 0.0, KeyValue::Number(0.0));
    Clip.set_key(Blur, Channel::Param(0), 10.0, KeyValue::Number(6.0));
    let Sprites = SpriteCache::build(&Owner);
    let Start = render_frame(&Owner, Some(0), 0.0, &Sprites, None);
    let Soft = render_frame(&Owner, Some(0), 10.0, &Sprites, None);
    assert_eq!(Start.get(26, 32), Sharp.get(26, 32));
    assert!(Soft.get(26, 32)[3] > 0 && Soft.get(26, 32)[3] < 255);
    let _ = Body;
}

#[test]
fn effect_group_only_touches_children() {
    let (mut Owner, Body) = red_sprite_project();
    let Shadow = add_effect(&mut Owner, EffectKind::DropShadow, None, Some(vec![5.0, 0.0, 0.0, 1.0]));
    Owner.Rig.node_mut(Body).unwrap().Parent = Some(Shadow);
    let Out = render_now(&Owner);
    assert_eq!(Out.get(39, 32), [0, 0, 0, 255]);
    assert_eq!(Out.get(32, 32), [255, 0, 0, 255]);
}

#[test]
fn import_layers_as_parts() {
    let mut Art = Project::with_image("Art", 40, 40);
    let Image = Art.Images[0].Id;
    let Second = Art.add_layer(Image, "Arm").unwrap();
    {
        let Found = Art.image_mut(Image).unwrap();
        std::sync::Arc::make_mut(&mut Found.Layers[0].Pixels).fill_rect(crate::geom::IRect::new(0, 0, 10, 10), [255, 0, 0, 255]);
        let Arm = Found.layer_mut(Second).unwrap();
        std::sync::Arc::make_mut(&mut Arm.Pixels).fill_rect(crate::geom::IRect::new(20, 30, 6, 4), [0, 255, 0, 255]);
    }
    let mut Owner = project();
    let Groups = import_project_parts(&mut Owner, &Art);
    assert_eq!(Groups.len(), 1);
    let Parts: Vec<&Node> = Owner.Rig.Nodes.iter().filter(|Entry| matches!(Entry.Kind, NodeKind::Sprite { .. })).collect();
    assert_eq!(Parts.len(), 2);
    assert!(close(Parts[1].Rest.X, 23.0, 0.01) && close(Parts[1].Rest.Y, 32.0, 0.01));
    let Out = render_now(&Owner);
    assert_eq!(Out.get(22, 31), [0, 255, 0, 255]);
    assert_eq!(Out.get(5, 5), [255, 0, 0, 255]);
}

#[test]
fn parts_placed_on_rotated_bones_stay_upright() {
    let mut Owner = project();
    let Spine = add_bone(&mut Owner, None, 32.0, 60.0, 32.0, 20.0);
    let Image = square(&mut Owner, "Box", 6, [0, 0, 255, 255]);
    let Part = add_sprite(&mut Owner, Image, Some(Spine), 40.0, 30.0).unwrap();
    let Solver = Solver::new(&Owner.Rig);
    let Pose = Solver.evaluate(None, 0.0);
    let Index = Solver.index(Some(Part)).unwrap();
    assert!(close(pose::angle_of(&Pose.World[Index]), 0.0, 0.01));
    let (X, Y) = pose::origin_of(&Pose.World[Index]);
    assert!(close(X, 40.0, 0.01) && close(Y, 30.0, 0.01));
}

#[test]
fn baking_ik_keeps_the_pose_without_constraints() {
    let mut Owner = project();
    let Upper = add_bone(&mut Owner, None, 10.0, 32.0, 30.0, 32.0);
    let Lower = add_bone(&mut Owner, Some(Upper), 30.0, 32.0, 50.0, 32.0);
    let Goal = new_node(&mut Owner, "Goal", NodeKind::Group, None);
    Owner.Rig.node_mut(Lower).unwrap().Constraints.push(Constraint::TwoBoneIk { Target: Some(Goal), BendPositive: true, Mix: 1.0 });
    let Clip = Owner.Rig.active_clip_mut().unwrap();
    Clip.Length = 6;
    Clip.set_key(Goal, Channel::PositionX, 0.0, KeyValue::Number(40.0));
    Clip.set_key(Goal, Channel::PositionY, 0.0, KeyValue::Number(50.0));
    Clip.set_key(Goal, Channel::PositionX, 5.0, KeyValue::Number(20.0));
    Clip.set_key(Goal, Channel::PositionY, 5.0, KeyValue::Number(45.0));
    let Tip = |Owner: &Project, Frame: f32| {
        let Solver = Solver::new(&Owner.Rig);
        let Pose = Solver.evaluate(Owner.Rig.active_clip(), Frame);
        let Index = Solver.index(Some(Lower)).unwrap();
        pose::tip_of(&Pose.World[Index], &Owner.Rig.Nodes[Index])
    };
    let Before: Vec<(f32, f32)> = (0..6).map(|Frame| Tip(&Owner, Frame as f32)).collect();
    let Baked = bake_constraints(&mut Owner, 0, &[Lower]);
    assert!(Baked.contains(&Upper) && Baked.contains(&Lower));
    assert!(Owner.Rig.node(Lower).unwrap().Constraints.is_empty());
    for Frame in 0..6 {
        let After = Tip(&Owner, Frame as f32);
        assert!(close(After.0, Before[Frame].0, 0.05) && close(After.1, Before[Frame].1, 0.05), "frame {Frame}: {After:?} vs {:?}", Before[Frame]);
    }
}

#[test]
fn insert_and_remove_frames_shift_keys() {
    let mut Owner = project();
    let Body = new_node(&mut Owner, "Body", NodeKind::Group, None);
    let Clip = Owner.Rig.active_clip_mut().unwrap();
    Clip.Length = 10;
    Clip.set_key(Body, Channel::PositionX, 2.0, KeyValue::Number(1.0));
    Clip.set_key(Body, Channel::PositionX, 6.0, KeyValue::Number(2.0));
    Clip.insert_frames(4.0, 3);
    assert_eq!(Clip.Length, 13);
    assert!(Clip.has_key(Body, Channel::PositionX, 2.0) && Clip.has_key(Body, Channel::PositionX, 9.0));
    Clip.remove_frames(8.0, 2);
    assert_eq!(Clip.Length, 11);
    assert!(Clip.has_key(Body, Channel::PositionX, 2.0) && !Clip.has_key(Body, Channel::PositionX, 9.0) && !Clip.has_key(Body, Channel::PositionX, 7.0));
}

#[test]
fn reparent_keeps_world_position() {
    let mut Owner = project();
    let Arm = add_bone(&mut Owner, None, 10.0, 10.0, 10.0, 40.0);
    let Thing = new_node(&mut Owner, "Thing", NodeKind::Group, None);
    Owner.Rig.node_mut(Thing).unwrap().Rest.X = 25.0;
    Owner.Rig.node_mut(Thing).unwrap().Rest.Y = 18.0;
    assert!(reparent_keep_world(&mut Owner, Thing, Some(Arm)));
    let (X, Y) = world_origin(&Owner, Thing, 0.0);
    assert!(close(X, 25.0, 0.01) && close(Y, 18.0, 0.01), "{X} {Y}");
    assert!(!reparent_keep_world(&mut Owner, Arm, Some(Thing)));
}

#[test]
fn autokey_and_rest_editing() {
    let mut Owner = project();
    let Body = new_node(&mut Owner, "Body", NodeKind::Group, None);
    set_value(&mut Owner, Body, Channel::PositionX, KeyValue::Number(5.0), 0.0, false);
    assert_eq!(Owner.Rig.node(Body).unwrap().Rest.X, 5.0);
    set_value(&mut Owner, Body, Channel::PositionX, KeyValue::Number(9.0), 12.0, true);
    assert!(Owner.Rig.active_clip().unwrap().has_key(Body, Channel::PositionX, 12.0));
    set_value(&mut Owner, Body, Channel::PositionX, KeyValue::Number(1.0), 3.0, false);
    assert!(Owner.Rig.active_clip().unwrap().has_key(Body, Channel::PositionX, 3.0));
    key_pose(&mut Owner, Body, 6.0, &[Channel::Rotation, Channel::Visible]);
    assert!(Owner.Rig.active_clip().unwrap().has_key(Body, Channel::Visible, 6.0));
}

#[test]
fn rig_round_trips_through_project_file() {
    let (mut Owner, Body) = red_sprite_project();
    let Bone = add_bone(&mut Owner, None, 0.0, 0.0, 10.0, 0.0);
    Owner.Rig.node_mut(Body).unwrap().Constraints.push(Constraint::LookAt { Target: Some(Bone), Mix: 0.5, Offset: 0.0, UseLimits: false, LimitMin: 0.0, LimitMax: 0.0, Slide: 0.0 });
    Owner.Rig.active_clip_mut().unwrap().set_key(Body, Channel::Rotation, 4.0, KeyValue::Number(33.0));
    add_effect(&mut Owner, EffectKind::Glow, None, None);
    let Path = std::env::temp_dir().join(format!("despite-rig-test-{}.despite", std::process::id()));
    crate::io::save_project(&Path, &Owner, None, &crate::io::SaveOptions::default()).unwrap();
    let Loaded = crate::io::load_project(&Path, &crate::io::LoadOptions::default()).unwrap().Project;
    assert_eq!(Loaded.Kind, ProjectKind::Animation);
    assert_eq!(Loaded.Rig, Owner.Rig);
    assert!(Loaded.NextId > Owner.Rig.max_id());
    assert_eq!(render_now(&Loaded).data(), render_now(&Owner).data());
    let _ = std::fs::remove_file(&Path);
}

#[test]
fn clip_frames_match_length() {
    let (mut Owner, Body) = red_sprite_project();
    let Clip = Owner.Rig.active_clip_mut().unwrap();
    Clip.Length = 12;
    Clip.Fps = 12.0;
    Clip.set_key(Body, Channel::PositionX, 0.0, KeyValue::Number(10.0));
    Clip.set_key(Body, Channel::PositionX, 11.0, KeyValue::Number(54.0));
    let Frames = clip_frames(&Owner, 0, None);
    assert_eq!(Frames.len(), 12);
    assert_eq!(Frames[0].DelayMs, 83);
    assert_eq!(Frames[0].Pixels.get(10, 32), [255, 0, 0, 255]);
    assert_eq!(Frames[11].Pixels.get(54, 32), [255, 0, 0, 255]);
    assert_eq!(Frames[11].Pixels.get(10, 32)[3], 0);
}
