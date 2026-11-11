mod auth;
mod config;
mod java_manager;
mod launcher;
mod library_manager;
mod models;
mod modrinth_client;
mod settings;
mod ui;
mod utils;

use adw::Application;
use gtk4::glib;
use relm4::RelmApp;

use ui::AppModel;

fn main() {
    let app = Application::builder()
        .application_id("dev.no8f.RADCraft")
        .build();

    glib::set_application_name("RADCraft");

    let relm_app = RelmApp::from_app(app);
    relm_app.run::<AppModel>(())
}
