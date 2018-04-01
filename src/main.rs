extern crate gtk;
extern crate glib;
extern crate gio;
extern crate gdk;
extern crate epoxy;
extern crate shared_library;
extern crate glutin;
extern crate alacritty;

use std::env::args;
use gio::prelude::*;
use gtk::prelude::*;

#[macro_use]
pub mod util; // order matters for macros
pub mod widget;

fn build_ui(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);

    window.set_title("Galacritty");
    window.set_border_width(0);
    window.set_default_size(1280, 720);

    window.connect_delete_event(clone!(window => move |_, _| {
        window.destroy();
        Inhibit(false)
    }));

    let term = widget::alacritty_widget();
    window.add(&term);

    window.show_all();
}

fn main() {
    let _ = alacritty::logging::initialize(&alacritty::cli::Options::default());

    let application = gtk::Application::new(
        "technology.unrelenting.galacritty",
        gio::ApplicationFlags::empty()
    ).expect("gtk::Application::new");

    application.connect_startup(|app| {
        build_ui(app);
    });
    application.connect_activate(|_| {});

    application.run(&args().collect::<Vec<_>>());
}
