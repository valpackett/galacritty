use gtk;
use gio;
use gdk;

use alacritty;
#[macro_use]
extern crate log;

use std::env::args;
use std::rc::Rc;
use std::cell::RefCell;

use gio::prelude::*;
use gio::{Menu, MenuExt, MenuItem, SimpleAction};
use gtk::prelude::*;

#[macro_use]
pub mod util; // order matters for macros
pub mod font;
pub mod widget;

fn build_actions(app: gtk::Application,
                 window: gtk::ApplicationWindow,
                 clipboard: gtk::Clipboard,
                 glarea: gtk::GLArea,
                 state: Rc<RefCell<Option<widget::State>>>) {
    let about_action = SimpleAction::new("HelpAbout", None);
    about_action.connect_activate(clone!(window => move |_, _| {
        let about = gtk::AboutDialog::new();
        about.set_transient_for(Some(&window));
        about.set_program_name("Galacritty");
        about.set_version(env!("CARGO_PKG_VERSION"));
        about.set_logo_icon_name("technology.unrelenting.galacritty");
        about.set_authors(&[env!("CARGO_PKG_AUTHORS")]);
        about.set_comments(env!("CARGO_PKG_DESCRIPTION"));
        about.connect_response(|about, _| about.destroy());
        about.run();
    }));
    app.add_action(&about_action);

    let paste_action = SimpleAction::new("Paste", None);
    paste_action.connect_activate(clone!(glarea, state => move |_, _| {
        if let Some(text) = clipboard.wait_for_text() {
            let mut state = state.borrow_mut();
            if let Some(ref mut state) = *state {
                // TODO: bracketed paste support
                state.event_queue.push(widget::Event::StringInput(text.replace("\r\n", "\r").replace("\n", "\r")));
            }
            glarea.queue_draw();
        }
    }));
    window.add_action(&paste_action);
    app.set_accels_for_action("win.Paste", &["<Control><Shift>v", "<Shift>Insert"]);

    let font_decr_action = SimpleAction::new("FontDecrease", None);
    font_decr_action.connect_activate(clone!(glarea, state => move |_, _| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(widget::Event::ChangeFontSize(-1));
        }
        glarea.queue_draw();
    }));
    window.add_action(&font_decr_action);
    app.set_accels_for_action("win.FontDecrease", &["<Control>minus", "<Control>KP_Subtract"]);

    let font_incr_action = SimpleAction::new("FontIncrease", None);
    font_incr_action.connect_activate(clone!(glarea, state => move |_, _| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(widget::Event::ChangeFontSize(1));
        }
        glarea.queue_draw();
    }));
    window.add_action(&font_incr_action);
    app.set_accels_for_action("win.FontIncrease", &["<Control>equal", "<Control>plus", "<Control>KP_Add"]);

    let font_choose_action = SimpleAction::new("FontChoose", None);
    font_choose_action.connect_activate(clone!(glarea, window, state => move |_, _| {
        glarea.set_auto_render(false);
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            let curf = state.config.font();
            let dial = gtk::FontChooserDialog::new("Choose Terminal Font", Some(&window));
            dial.set_preview_text("if c0de$[1337] { \"hello\".world(); /* test */ }");
            dial.set_font(&format!("{} {}", curf.normal.family, curf.size.as_f32_pts()));
            let acc : i32 = gtk::ResponseType::Ok.into();
            if dial.run() == acc {
                if let Some(fam) = dial.get_font_family() {
                    let newf = font::to_alacritty(fam, dial.get_font_size());
                    let fontdiff = (newf.size.as_f32_pts() - curf.size.as_f32_pts()) as i8;
                    state.config.set_font(newf);
                    state.event_queue.push(widget::Event::ChangeFontSize(fontdiff));
                    // force reload the glyph cache if the size didn't change
                    state.event_queue.push(widget::Event::HiDPIFactorChanged(glarea.get_scale_factor() as f32));
                }
            }
            dial.destroy();
        }
        glarea.set_auto_render(true);
        glarea.queue_draw();
    }));
    window.add_action(&font_choose_action);

    let font_reset_action = SimpleAction::new("FontReset", None);
    font_reset_action.connect_activate(move |_, _| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(widget::Event::ResetFontSize);
        }
        glarea.queue_draw();
    });
    window.add_action(&font_reset_action);
    app.set_accels_for_action("win.FontReset", &["<Control>0", "<Control>KP_0"]);
}

fn build_main_menu() -> Menu {
    let menu = Menu::new();

    let section = Menu::new();
    section.append_item(&MenuItem::new("About", "app.HelpAbout"));
    menu.append_section(None, &section);

    menu.freeze();
    menu
}

fn build_header_bar() -> gtk::HeaderBar {
    let header_bar = gtk::HeaderBar::new();

    let font_decr_btn = gtk::Button::new_from_icon_name("zoom-out-symbolic", gtk::IconSize::SmallToolbar.into());
    font_decr_btn.set_can_focus(false);
    font_decr_btn.set_tooltip_text("Decrease font size");
    font_decr_btn.set_action_name("win.FontDecrease");
    header_bar.pack_start(&font_decr_btn);

    let font_choose_btn = gtk::Button::new_from_icon_name("font-select-symbolic", gtk::IconSize::SmallToolbar.into());
    font_choose_btn.set_can_focus(false);
    font_choose_btn.set_tooltip_text("Choose font");
    font_choose_btn.set_action_name("win.FontChoose");
    header_bar.pack_start(&font_choose_btn);

    let font_incr_btn = gtk::Button::new_from_icon_name("zoom-in-symbolic", gtk::IconSize::SmallToolbar.into());
    font_incr_btn.set_can_focus(false);
    font_incr_btn.set_tooltip_text("Increase font size");
    font_incr_btn.set_action_name("win.FontIncrease");
    header_bar.pack_start(&font_incr_btn);

    let paste_btn = gtk::Button::new_from_icon_name("edit-paste-symbolic", gtk::IconSize::SmallToolbar.into());
    paste_btn.set_can_focus(false);
    paste_btn.set_tooltip_text("Paste from clipboard");
    paste_btn.set_action_name("win.Paste");
    header_bar.pack_end(&paste_btn);

    header_bar.set_show_close_button(true);
    header_bar
}

fn build_ui(app: &gtk::Application) {
    gtk::Window::set_default_icon_name("technology.unrelenting.galacritty");

    let window = gtk::ApplicationWindow::new(app);

    window.set_title("Galacritty");
    window.set_border_width(0);
    window.set_default_size(1280, 720);

    window.connect_delete_event(clone!(window => move |_, _| {
        window.destroy();
        Inhibit(false)
    }));

    let clipboard = gtk::Clipboard::get(&gdk::Atom::intern("CLIPBOARD"));

    let header_bar = build_header_bar();
    window.set_titlebar(Some(&header_bar));

    let (glarea, state) = widget::alacritty_widget(window.clone(), header_bar);

    build_actions(app.clone(), window.clone(), clipboard, glarea.clone(), state.clone());

    app.set_app_menu(Some(&build_main_menu()));
    window.add(&glarea);
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
