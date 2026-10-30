use super::{Channel, Clip, Constraint, DriverMode, Easing, EffectKind, KeyValue, NodeId, NodeKind, Rig, Transform2};
use crate::color::Color;
use crate::project::{Project, ProjectKind};
use crate::raster::Raster;

fn ellipse(Width: u32, Height: u32, Fill: [u8; 4], Edge: [u8; 4], Thickness: f32) -> Raster {
    let mut Out = Raster::new(Width, Height);
    let (Cx, Cy) = (Width as f32 / 2.0, Height as f32 / 2.0);
    let (Rx, Ry) = (Cx - 0.5, Cy - 0.5);
    let Inner = 1.0 - Thickness / Rx.min(Ry).max(1.0);
    for Y in 0..Height {
        for X in 0..Width {
            let (Dx, Dy) = ((X as f32 + 0.5 - Cx) / Rx, (Y as f32 + 0.5 - Cy) / Ry);
            let Distance = (Dx * Dx + Dy * Dy).sqrt();
            if Distance <= 1.0 {
                Out.put(X, Y, if Distance >= Inner { Edge } else { Fill });
            }
        }
    }
    Out
}

fn capsule(Width: u32, Height: u32, Fill: [u8; 4], Edge: [u8; 4]) -> Raster {
    let mut Out = Raster::new(Width, Height);
    let Radius = Height as f32 / 2.0;
    for Y in 0..Height {
        for X in 0..Width {
            let Px = (X as f32 + 0.5).clamp(Radius, Width as f32 - Radius);
            let Distance = ((X as f32 + 0.5 - Px).powi(2) + (Y as f32 + 0.5 - Radius).powi(2)).sqrt();
            if Distance <= Radius - 0.5 {
                Out.put(X, Y, if Distance >= Radius - 2.5 { Edge } else { Fill });
            }
        }
    }
    Out
}

fn mouth(Width: u32, Height: u32, Open: f32) -> Raster {
    let mut Out = Raster::new(Width, Height);
    let (Cx, Cy) = (Width as f32 / 2.0, Height as f32 / 2.0);
    let (Rx, Ry) = (Width as f32 / 2.0 - 1.0, (Height as f32 / 2.0 - 1.0) * Open.max(0.12));
    for Y in 0..Height {
        for X in 0..Width {
            let (Dx, Dy) = ((X as f32 + 0.5 - Cx) / Rx, (Y as f32 + 0.5 - Cy) / Ry);
            if Dx * Dx + Dy * Dy <= 1.0 {
                Out.put(X, Y, [90, 20, 40, 255]);
            }
        }
    }
    Out
}

fn named(Owner: &mut Project, Id: NodeId, Name: &str) {
    if let Some(Found) = Owner.Rig.node_mut(Id) {
        Found.Name = Name.to_string();
    }
}

fn keys(Target: &mut Clip, Node: NodeId, Which: Channel, Points: &[(f32, f32)], Ease: Easing) {
    for (Frame, Value) in Points {
        Target.set_key(Node, Which, *Frame, KeyValue::Number(*Value));
    }
    if let Some(Track) = Target.Tracks.iter_mut().find(|Track| Track.Node == Node && Track.Channel == Which) {
        for Key in &mut Track.Keys {
            Key.Ease = Ease;
        }
    }
}

fn turn(Target: &mut Clip, Owner: &Project, Node: NodeId, Points: &[(f32, f32)], Ease: Easing) {
    let Rest = Owner.Rig.node(Node).map(|Found| Found.Rest.Rotation).unwrap_or(0.0);
    let Shifted: Vec<(f32, f32)> = Points.iter().map(|(Frame, Offset)| (*Frame, Rest + Offset)).collect();
    keys(Target, Node, Channel::Rotation, &Shifted, Ease);
}

pub fn sample_project() -> Project {
    let mut Owner = Project::new_of_kind("Sample rig", ProjectKind::Animation);
    Owner.Rig = Rig::new(260, 220);
    Owner.Rig.Background = Some(Color::rgb(0.93, 0.95, 1.0));
    let Skin = [255, 214, 170, 255];
    let Outline = [70, 50, 60, 255];
    let Shirt = [90, 150, 230, 255];
    let Pants = [70, 80, 120, 255];
    let BodyImage = super::add_image_asset(&mut Owner, "Body", ellipse(56, 70, Shirt, Outline, 2.5));
    let HeadImage = super::add_image_asset(&mut Owner, "Head", ellipse(60, 56, Skin, Outline, 2.5));
    let EyeImage = super::add_image_asset(&mut Owner, "Eye", ellipse(16, 18, [255, 255, 255, 255], Outline, 1.5));
    let PupilImage = super::add_image_asset(&mut Owner, "Pupil", ellipse(8, 8, [30, 30, 40, 255], [30, 30, 40, 255], 1.0));
    let LidImage = super::add_image_asset(&mut Owner, "Eyelid", ellipse(18, 20, Skin, Outline, 1.5));
    let ArmImage = super::add_image_asset(&mut Owner, "Arm", capsule(36, 12, Skin, Outline));
    let LegImage = super::add_image_asset(&mut Owner, "Leg", capsule(36, 14, Pants, Outline));
    let BallImage = super::add_image_asset(&mut Owner, "Ball", ellipse(22, 22, [250, 120, 60, 255], Outline, 2.0));
    let TipImage = super::add_image_asset(&mut Owner, "Antenna tip", ellipse(10, 10, [255, 90, 120, 255], Outline, 1.5));
    let StalkImage = super::add_image_asset(&mut Owner, "Antenna", capsule(22, 4, Outline, Outline));
    let MouthClosed = super::add_image_asset(&mut Owner, "Mouth closed", mouth(18, 12, 0.15));
    let MouthOpen = super::add_image_asset(&mut Owner, "Mouth open", mouth(18, 12, 1.0));

    let Ball = super::new_node(&mut Owner, "Ball", NodeKind::Group, None);
    let Glow = super::add_effect(&mut Owner, EffectKind::Glow, Some(Ball), Some(vec![6.0, 0.8, 0.55]));
    super::place_upright(&mut Owner, Ball, None, 210.0, 70.0);
    let BallSprite = super::add_sprite(&mut Owner, BallImage, Some(Glow), 210.0, 70.0).expect("ball image");
    named(&mut Owner, BallSprite, "Ball picture");
    let Shadow = super::add_effect(&mut Owner, EffectKind::DropShadow, None, Some(vec![3.0, 4.0, 3.0, 0.3]));
    named(&mut Owner, Shadow, "Character shadow");
    let Hips = super::add_bone(&mut Owner, Some(Shadow), 120.0, 160.0, 120.0, 128.0);
    named(&mut Owner, Hips, "Hips");
    for (Side, Sign) in [("Left", -1.0f32), ("Right", 1.0)] {
        let Thigh = super::add_bone(&mut Owner, Some(Hips), 120.0 + Sign * 12.0, 160.0, 120.0 + Sign * 16.0, 182.0);
        let Shin = super::add_bone(&mut Owner, Some(Thigh), 120.0 + Sign * 16.0, 182.0, 120.0 + Sign * 18.0, 204.0);
        named(&mut Owner, Thigh, &format!("{Side} thigh"));
        named(&mut Owner, Shin, &format!("{Side} shin"));
        for Bone in [Thigh, Shin] {
            let Part = super::add_sprite(&mut Owner, LegImage, Some(Bone), 0.0, 0.0).expect("leg image");
            let Found = Owner.Rig.node_mut(Part).expect("leg part");
            Found.Name = format!("{Side} leg part");
            Found.Rest = Transform2 { X: 11.0, ..Default::default() };
            Found.DrawOrder = -2;
        }
    }
    let Body = super::add_sprite(&mut Owner, BodyImage, Some(Hips), 120.0, 140.0).expect("body image");
    let Neck = super::add_bone(&mut Owner, Some(Hips), 120.0, 128.0, 120.0, 106.0);
    named(&mut Owner, Neck, "Neck");
    let Head = super::add_sprite(&mut Owner, HeadImage, Some(Neck), 120.0, 86.0).expect("head image");
    let Stalk = super::add_bone(&mut Owner, Some(Head), 120.0, 62.0, 128.0, 42.0);
    named(&mut Owner, Stalk, "Antenna bone");
    let StalkPart = super::add_sprite(&mut Owner, StalkImage, Some(Stalk), 0.0, 0.0).expect("antenna image");
    Owner.Rig.node_mut(StalkPart).expect("antenna part").Rest = Transform2 { X: 10.0, ..Default::default() };
    let Tip = super::add_sprite(&mut Owner, TipImage, Some(Stalk), 0.0, 0.0).expect("tip image");
    Owner.Rig.node_mut(Tip).expect("tip").Rest = Transform2 { X: 22.0, ..Default::default() };
    Owner.Rig.node_mut(Stalk).expect("antenna bone").Constraints.push(Constraint::Jiggle { Stiffness: 0.18, Damping: 0.18, Gravity: 0.0, Mix: 1.0 });
    let Look = super::new_node(&mut Owner, "Look target", NodeKind::Group, None);
    super::place_upright(&mut Owner, Look, None, 210.0, 70.0);
    let mut Lids = Vec::new();
    for (Name, X) in [("Left eye", 110.0), ("Right eye", 132.0)] {
        let Eye = super::add_sprite(&mut Owner, EyeImage, Some(Head), X, 80.0).expect("eye image");
        let Pupil = super::add_sprite(&mut Owner, PupilImage, Some(Eye), X, 80.0).expect("pupil image");
        let Lid = super::add_sprite(&mut Owner, LidImage, Some(Eye), X, 79.0).expect("lid image");
        named(&mut Owner, Eye, Name);
        named(&mut Owner, Pupil, &format!("{Name} pupil"));
        named(&mut Owner, Lid, &format!("{Name} lid"));
        Owner.Rig.node_mut(Eye).expect("eye").ClipChildren = true;
        Owner.Rig.node_mut(Pupil).expect("pupil").Constraints.push(Constraint::LookAt { Target: Some(Look), Mix: 1.0, Offset: 0.0, UseLimits: false, LimitMin: 0.0, LimitMax: 0.0, Slide: 4.0 });
        let Found = Owner.Rig.node_mut(Lid).expect("lid");
        Found.Visible = false;
        Found.DrawOrder = 5;
        Lids.push(Lid);
    }
    let Mouth = super::new_node(&mut Owner, "Mouth", NodeKind::Group, None);
    super::place_upright(&mut Owner, Mouth, Some(Head), 120.0, 102.0);
    Owner.Rig.node_mut(Mouth).expect("mouth").Constraints.push(Constraint::Switch);
    for (Image, Name) in [(MouthClosed, "Closed"), (MouthOpen, "Open")] {
        let Shape = super::add_sprite(&mut Owner, Image, Some(Mouth), 120.0, 102.0).expect("mouth image");
        named(&mut Owner, Shape, Name);
    }
    let Talk = super::new_node(&mut Owner, "Talk", NodeKind::Controller { Min: 0.0, Max: 1.0 }, None);
    super::place_upright(&mut Owner, Talk, None, 14.0, 14.0);
    Owner.Rig.node_mut(Mouth).expect("mouth").Constraints.push(Constraint::Driver { Source: Some(Talk), From: Channel::Value, To: Channel::Switch, Mode: DriverMode::Threshold { Threshold: 0.5, Above: true } });
    let Blink = super::new_node(&mut Owner, "Blink", NodeKind::Controller { Min: 0.0, Max: 1.0 }, None);
    super::place_upright(&mut Owner, Blink, None, 14.0, 30.0);
    Owner.Rig.node_mut(Blink).expect("blink").Constraints.push(Constraint::Visibility { Targets: Lids, From: Channel::Value, Min: 0.5, Max: 1.0, Invert: false });
    let Reach = super::new_node(&mut Owner, "Hand target", NodeKind::Group, None);
    super::place_upright(&mut Owner, Reach, None, 175.0, 120.0);
    let mut Arms = Vec::new();
    for (Side, Sign) in [("Left", -1.0f32), ("Right", 1.0)] {
        let ShoulderX = 120.0 + Sign * 22.0;
        let Upper = super::add_bone(&mut Owner, Some(Hips), ShoulderX, 124.0, ShoulderX + Sign * 22.0, 138.0);
        let Lower = super::add_bone(&mut Owner, Some(Upper), ShoulderX + Sign * 22.0, 138.0, ShoulderX + Sign * 44.0, 148.0);
        named(&mut Owner, Upper, &format!("{Side} upper arm"));
        named(&mut Owner, Lower, &format!("{Side} forearm"));
        for Bone in [Upper, Lower] {
            let Part = super::add_sprite(&mut Owner, ArmImage, Some(Bone), 0.0, 0.0).expect("arm image");
            let Found = Owner.Rig.node_mut(Part).expect("arm part");
            Found.Name = format!("{Side} arm part");
            Found.Rest = Transform2 { X: 13.0, ..Default::default() };
            Found.DrawOrder = -1;
        }
        Arms.push((Upper, Lower));
    }
    let (RightUpper, RightLower) = Arms[1];
    let (LeftUpper, LeftLower) = Arms[0];
    Owner.Rig.node_mut(RightLower).expect("forearm").Constraints.push(Constraint::TwoBoneIk { Target: Some(Reach), BendPositive: false, Mix: 1.0 });

    let mut Hop = Clip { Id: Owner.alloc_id(), Name: "Hop and look".into(), Length: 36, Fps: 18.0, Loop: true, Tracks: Vec::new() };
    keys(&mut Hop, Hips, Channel::PositionY, &[(0.0, 160.0), (6.0, 160.0), (12.0, 134.0), (18.0, 128.0), (24.0, 160.0), (30.0, 160.0), (35.0, 160.0)], Easing::EaseInOut);
    keys(&mut Hop, Body, Channel::Stretch, &[(0.0, 0.0), (6.0, -0.25), (12.0, 0.3), (18.0, 0.05), (24.0, 0.35), (30.0, -0.2), (35.0, 0.0)], Easing::EaseInOut);
    for Leg in ["Left", "Right"] {
        let Thigh = Owner.Rig.Nodes.iter().find(|Found| Found.Name == format!("{Leg} thigh")).map(|Found| Found.Id).expect("thigh");
        let Shin = Owner.Rig.Nodes.iter().find(|Found| Found.Name == format!("{Leg} shin")).map(|Found| Found.Id).expect("shin");
        let Sign = if Leg == "Left" { 1.0 } else { -1.0 };
        turn(&mut Hop, &Owner, Thigh, &[(0.0, 0.0), (6.0, Sign * 12.0), (12.0, Sign * 35.0), (18.0, Sign * 40.0), (24.0, 0.0), (35.0, 0.0)], Easing::EaseInOut);
        turn(&mut Hop, &Owner, Shin, &[(0.0, 0.0), (6.0, -Sign * 20.0), (12.0, -Sign * 50.0), (18.0, -Sign * 55.0), (24.0, 0.0), (35.0, 0.0)], Easing::EaseInOut);
    }
    for (Frame, X, Y) in [(0.0, 210.0, 60.0), (9.0, 160.0, 30.0), (18.0, 40.0, 50.0), (27.0, 130.0, 25.0), (35.0, 210.0, 60.0)] {
        Hop.set_key(Ball, Channel::PositionX, Frame, KeyValue::Number(X));
        Hop.set_key(Ball, Channel::PositionY, Frame, KeyValue::Number(Y));
        Hop.set_key(Look, Channel::PositionX, Frame, KeyValue::Number(X));
        Hop.set_key(Look, Channel::PositionY, Frame, KeyValue::Number(Y));
        Hop.set_key(Reach, Channel::PositionX, Frame, KeyValue::Number((X + 120.0) / 2.0 + 30.0));
        Hop.set_key(Reach, Channel::PositionY, Frame, KeyValue::Number((Y + 140.0) / 2.0 + 20.0));
    }
    keys(&mut Hop, Talk, Channel::Value, &[(0.0, 0.0), (8.0, 1.0), (10.0, 0.0), (12.0, 1.0), (14.0, 0.0), (26.0, 1.0), (29.0, 0.0)], Easing::Constant);
    keys(&mut Hop, Blink, Channel::Value, &[(0.0, 0.0), (20.0, 1.0), (22.0, 0.0)], Easing::Constant);
    keys(&mut Hop, Glow, Channel::Param(0), &[(0.0, 3.0), (18.0, 12.0), (35.0, 3.0)], Easing::EaseInOut);
    turn(&mut Hop, &Owner, Neck, &[(0.0, 0.0), (18.0, -12.0), (35.0, 0.0)], Easing::EaseInOut);

    let mut Wave = Clip { Id: Owner.alloc_id(), Name: "Wave".into(), Length: 32, Fps: 16.0, Loop: true, Tracks: Vec::new() };
    keys(&mut Wave, RightLower, Channel::Mix(0), &[(0.0, 0.0)], Easing::Constant);
    keys(&mut Wave, RightUpper, Channel::Rotation, &[(0.0, 30.0), (31.0, 30.0)], Easing::Linear);
    keys(&mut Wave, RightLower, Channel::Rotation, &[(0.0, -45.0), (4.0, 15.0), (8.0, -45.0), (12.0, 15.0), (16.0, -45.0), (20.0, 15.0), (24.0, -45.0), (28.0, 15.0), (31.0, -45.0)], Easing::EaseInOut);
    turn(&mut Wave, &Owner, Neck, &[(0.0, 0.0), (8.0, 8.0), (24.0, 8.0), (31.0, 0.0)], Easing::EaseInOut);
    keys(&mut Wave, Talk, Channel::Value, &[(0.0, 0.0), (6.0, 1.0), (8.0, 0.0), (10.0, 1.0), (13.0, 0.0), (18.0, 1.0), (21.0, 0.0)], Easing::Constant);
    keys(&mut Wave, Look, Channel::PositionX, &[(0.0, 120.0)], Easing::Constant);
    keys(&mut Wave, Look, Channel::PositionY, &[(0.0, 200.0)], Easing::Constant);
    keys(&mut Wave, Ball, Channel::Opacity, &[(0.0, 0.0)], Easing::Constant);
    keys(&mut Wave, Body, Channel::Stretch, &[(0.0, 0.0), (8.0, 0.06), (16.0, 0.0), (24.0, 0.06), (31.0, 0.0)], Easing::EaseInOut);
    turn(&mut Wave, &Owner, LeftUpper, &[(0.0, 0.0), (16.0, -8.0), (31.0, 0.0)], Easing::EaseInOut);

    let mut Idle = Clip { Id: Owner.alloc_id(), Name: "Idle".into(), Length: 48, Fps: 12.0, Loop: true, Tracks: Vec::new() };
    keys(&mut Idle, Body, Channel::Stretch, &[(0.0, 0.0), (12.0, 0.05), (24.0, 0.0), (36.0, 0.05), (47.0, 0.0)], Easing::EaseInOut);
    turn(&mut Idle, &Owner, Neck, &[(0.0, 0.0), (24.0, 4.0), (47.0, 0.0)], Easing::EaseInOut);
    keys(&mut Idle, Blink, Channel::Value, &[(0.0, 0.0), (30.0, 1.0), (32.0, 0.0)], Easing::Constant);
    keys(&mut Idle, Look, Channel::PositionX, &[(0.0, 60.0), (16.0, 60.0), (20.0, 200.0), (40.0, 200.0), (44.0, 60.0)], Easing::EaseInOut);
    keys(&mut Idle, Look, Channel::PositionY, &[(0.0, 90.0)], Easing::Constant);
    keys(&mut Idle, Ball, Channel::Opacity, &[(0.0, 0.0)], Easing::Constant);
    keys(&mut Idle, RightLower, Channel::Mix(0), &[(0.0, 0.0)], Easing::Constant);
    turn(&mut Idle, &Owner, LeftLower, &[(0.0, 0.0), (24.0, 6.0), (47.0, 0.0)], Easing::EaseInOut);
    turn(&mut Idle, &Owner, RightLower, &[(0.0, 0.0), (24.0, -6.0), (47.0, 0.0)], Easing::EaseInOut);

    Owner.Rig.Clips = vec![Hop, Wave, Idle];
    Owner.Rig.ActiveClip = 0;
    Owner
}
