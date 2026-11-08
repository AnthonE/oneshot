//! The desktop window's icon: the reticle (`tools/logo/make.py`'s
//! `icon-a-reticle`, 128 px), set once the window exists. X11 and Windows
//! show it; Wayland and macOS take theirs from the desktop entry or bundle.

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::WINIT_WINDOWS;

const PNG: &[u8] = include_bytes!("window_icon.png");

/// Sets the icon on the first frame the window is there, then never again.
pub fn set(mut done: Local<bool>, window: Query<Entity, With<PrimaryWindow>>, _main: NonSendMarker) {
    if *done {
        return;
    }
    let Ok(entity) = window.single() else { return };
    WINIT_WINDOWS.with_borrow(|windows| {
        let Some(w) = windows.get_window(entity) else { return };
        w.set_window_icon(icon());
        *done = true;
    });
}

fn icon() -> Option<winit::window::Icon> {
    let img = image::load_from_memory_with_format(PNG, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    let (w, h) = img.dimensions();
    winit::window::Icon::from_rgba(img.into_raw(), w, h).ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_icon_decodes() {
        assert!(super::icon().is_some());
    }
}
