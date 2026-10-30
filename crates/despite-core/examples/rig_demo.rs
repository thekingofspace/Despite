#![allow(non_snake_case)]

use despite_core::export::{GifOptions, SheetOptions, build_sprite_sheet, encode_gif};
use despite_core::io::{SaveOptions, save_project};

fn main() {
    let Owner = despite_core::animation::sample::sample_project();
    let Folder = std::path::Path::new("target/rig_demo");
    std::fs::create_dir_all(Folder).expect("output folder");
    for (Index, Clip) in Owner.Rig.Clips.iter().enumerate() {
        let Frames = despite_core::animation::clip_frames(&Owner, Index, None);
        let Name = despite_core::export::sanitize_file_name(&Clip.Name);
        std::fs::write(Folder.join(format!("{Name}.gif")), encode_gif(&Frames, &GifOptions::default()).expect("gif encodes")).expect("write gif");
        build_sprite_sheet(&Frames, &SheetOptions::default()).Pixels.save_png(&Folder.join(format!("{Name}_sheet.png"))).expect("write sheet");
        println!("{}: {} frames", Clip.Name, Frames.len());
    }
    save_project(&Folder.join("Sample rig.despite"), &Owner, None, &SaveOptions::default()).expect("save project");
}
