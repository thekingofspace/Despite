#![allow(non_snake_case, non_upper_case_globals)]

use std::f32::consts::TAU;
use std::path::PathBuf;

use despite_core::brush::{Brush, Stroke, StrokeContext, StrokePoint, advanced_presets};
use despite_core::export::{FrameSource, GifOptions, collect_frames, encode_gif};
use despite_core::io::{SaveOptions, save_project};
use despite_core::{Color, LayerId, Project};

fn circle(CenterX: f32, CenterY: f32, Radius: f32, Steps: usize) -> Vec<(f32, f32)> {
    (0..=Steps).map(|Index| {
        let Angle = TAU * Index as f32 / Steps as f32;
        (CenterX + Radius * Angle.cos(), CenterY + Radius * Angle.sin())
    }).collect()
}

fn arc(CenterX: f32, CenterY: f32, Radius: f32, From: f32, To: f32, Steps: usize) -> Vec<(f32, f32)> {
    (0..=Steps).map(|Index| {
        let Angle = From + (To - From) * Index as f32 / Steps as f32;
        (CenterX + Radius * Angle.cos(), CenterY + Radius * Angle.sin())
    }).collect()
}

fn star(CenterX: f32, CenterY: f32, Outer: f32, Inner: f32) -> Vec<(f32, f32)> {
    (0..=10).map(|Index| {
        let Angle = -TAU / 4.0 + TAU * Index as f32 / 10.0;
        let Radius = if Index % 2 == 0 { Outer } else { Inner };
        (CenterX + Radius * Angle.cos(), CenterY + Radius * Angle.sin())
    }).collect()
}

fn character(OffsetX: f32) -> Vec<Vec<(f32, f32)>> {
    vec![
        circle(OffsetX + 60.0, 70.0, 34.0, 48),
        circle(OffsetX + 47.0, 62.0, 5.0, 12),
        circle(OffsetX + 73.0, 62.0, 5.0, 12),
        arc(OffsetX + 60.0, 74.0, 16.0, 0.35, 2.8, 20),
        vec![(OffsetX + 40.0, 40.0), (OffsetX + 34.0, 22.0), (OffsetX + 50.0, 37.0)],
        vec![(OffsetX + 80.0, 40.0), (OffsetX + 86.0, 22.0), (OffsetX + 70.0, 37.0)],
        star(OffsetX + 60.0, 136.0, 16.0, 7.0),
    ]
}

fn draw_boiled(Target: &mut Project, Image: despite_core::ImageId, Layer: LayerId, Chosen: &Brush, Paths: &[Vec<(f32, f32)>], Ink: Color) {
    let Frames = Chosen.Advanced.boil_frames();
    let Group = Target.alloc_id();
    let NewIds: Vec<LayerId> = (1..Frames).map(|_| Target.alloc_layer_id()).collect();
    let Found = Target.image_mut(Image).expect("image exists");
    let Index = Found.layer_index(Layer).expect("layer exists");
    let Members = Found.make_boil_group(Index, Frames, Group, &NewIds);
    for (Step, Member) in Members.into_iter().enumerate() {
        let Entry = &mut Found.Layers[Member];
        for (PathIndex, Path) in Paths.iter().enumerate() {
            let Context = StrokeContext {
                Primary: Ink,
                Seed: 0xB011 + (Step as u64 + 1).wrapping_mul(0x9E37_79B9) + PathIndex as u64 * 7919,
                ..Default::default()
            };
            let mut Active = Stroke::begin(Chosen, Context, Entry);
            let Pixels = Entry.pixels_mut();
            for (PointIndex, (X, Y)) in Path.iter().enumerate() {
                let Progress = PointIndex as f32 / Path.len().max(2) as f32;
                let Pressure = 0.55 + 0.45 * (Progress * std::f32::consts::PI).sin();
                Active.add_point(Pixels, StrokePoint::new(*X, *Y, Pressure));
            }
            Active.finish(Pixels);
        }
    }
}

fn main() {
    let Output = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("target/boil_demo"));
    std::fs::create_dir_all(&Output).expect("create output folder");
    let Presets = advanced_presets();
    let Find = |Name: &str| Presets.iter().find(|Entry| Entry.Name.starts_with(Name)).cloned().expect("preset exists");
    let Brushes = [
        (Find("Shaky Pencil"), Color::from_hex("#2b2430").unwrap()),
        (Find("Wobbly Ink"), Color::from_hex("#3a1f5c").unwrap()),
        (Find("Crayon Scribble"), Color::from_hex("#c2410c").unwrap()),
    ];
    let mut Demo = Project::with_image("Boil demo", 360, 170);
    let Image = Demo.Images[0].Id;
    {
        let Found = Demo.image_mut(Image).unwrap();
        Found.Name = "Boil demo".into();
        Found.FrameDelayMs = 110;
        Found.Layers[0].Name = "Paper".into();
        Found.Layers[0].pixels_mut().fill(Color::from_hex("#fbf6ec").unwrap().to_rgba8());
    }
    for (Column, (Chosen, Ink)) in Brushes.iter().enumerate() {
        let Layer = Demo.add_layer(Image, &Chosen.Name).expect("layer added");
        let mut Sized = Chosen.clone();
        Sized.Size = match Column {
            0 => 3.0,
            1 => 3.5,
            _ => 6.0,
        };
        draw_boiled(&mut Demo, Image, Layer, &Sized, &character(Column as f32 * 120.0), *Ink);
    }
    let Frames = collect_frames(&Demo, &FrameSource::Image(Image));
    let Gif = encode_gif(&Frames, &GifOptions { Speed: 1, ..Default::default() }).expect("encode gif");
    let GifPath = Output.join("boil_demo.gif");
    std::fs::write(&GifPath, Gif).expect("write gif");
    let ProjectPath = Output.join("boil_demo.despite");
    save_project(&ProjectPath, &Demo, None, &SaveOptions::default()).expect("save project");
    let Strip = despite_core::export::build_sprite_sheet(&Frames, &despite_core::export::SheetOptions { Layout: despite_core::export::SheetLayout::Vertical, Padding: 4, ..Default::default() });
    Strip.Pixels.save_png(&Output.join("boil_demo_frames.png")).expect("write frame strip");
    println!("{} frames", Frames.len());
    println!("{}", GifPath.display());
    println!("{}", ProjectPath.display());
}
