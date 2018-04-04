use pango::{self, Style, Weight};
use pango::prelude::*;

use alacritty::config::{Font, Size};

/// Converts a Pango/Gtk font spec to an Alacritty one
pub fn to_alacritty(fam: pango::FontFamily, size: i32) -> Font {
    let size = size as f32 / pango::SCALE as f32;
    info!("Chosen font family {:?} size {}", fam.get_name(), size);
    let mut newf = Font::default();
    newf.size = Size::new(size);
    if let Some(name) = fam.get_name() {
        newf.normal.family = name.clone();
        newf.bold.family = name.clone();
        newf.italic.family = name;
    } else {
        warn!("You've managed to select a font family with no name, somehow.");
    }
    // Find exact names of "Normal" "Bold" "Italic" suffixes for this family
    for face in fam.list_faces().iter() {
        if let Some(desc) = face.describe() {
            info!("  - has face {:?} style {:?} weight {:?} variant {:?}", face.get_face_name(), desc.get_style(), desc.get_weight(), desc.get_variant());
            match (desc.get_style(), desc.get_weight()) {
                (Style::Normal, Weight::Normal) => {
                    newf.normal.style = face.get_face_name();
                },
                (Style::Normal, Weight::Bold) => {
                    newf.bold.style = face.get_face_name();
                },
                (Style::Italic, Weight::Normal) => {
                    newf.italic.style = face.get_face_name();
                },
                _ => (),
            }
        }
    }
    newf
}
