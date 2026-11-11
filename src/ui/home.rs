use crate::models::Profile;
use crate::ui::model::AppModel;
use crate::ui::msg::AppMsg;
use gtk::prelude::*;
use relm4::ComponentSender;
use relm4::gtk;

pub fn create_home_page(
    sender: &ComponentSender<AppModel>,
    profile_list: &gtk::ListBox,
) -> (gtk::Box, gtk::Button, gtk::Button) {
    let main_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    let content_container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .hexpand(true)
        .vexpand(true)
        .halign(gtk::Align::Fill)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    // Header row with "Home" label and right-anchored "New Profile" button
    let header_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .hexpand(true)
        .halign(gtk::Align::Fill)
        .build();

    let title_label = gtk::Label::builder()
        .label("Home")
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .css_classes(vec!["title-1".to_string()])
        .build();

    let new_profile_header_button_content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    new_profile_header_button_content
        .append(&gtk::Image::builder().icon_name("list-add-symbolic").build());
    new_profile_header_button_content.append(&gtk::Label::new(Some("New Profile")));

    let new_profile_header_button = gtk::Button::builder()
        .halign(gtk::Align::End)
        .valign(gtk::Align::Center)
        .css_classes(vec!["suggested-action".to_string()])
        .child(&new_profile_header_button_content)
        .build();

    let sender_clone = sender.clone();
    new_profile_header_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::OpenCreateProfileDialog);
    });

    header_box.append(&title_label);
    header_box.append(&new_profile_header_button);

    content_container.append(&header_box);

    // Use the provided profile list
    profile_list.set_selection_mode(gtk::SelectionMode::None);
    profile_list.add_css_class("boxed-list");

    content_container.append(profile_list);

    // Large "New Profile" button shown instead of the "No profiles yet..." box when there are no profiles
    let empty_btn_content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(24)
        .margin_end(24)
        .build();
    empty_btn_content.append(
        &gtk::Image::builder()
            .icon_name("list-add-symbolic")
            .pixel_size(24)
            .build(),
    );
    empty_btn_content.append(
        &gtk::Label::builder()
            .label("New Profile")
            .css_classes(vec!["title-3".to_string()])
            .build(),
    );

    let empty_new_profile_button = gtk::Button::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .vexpand(true)
        .width_request(260)
        .height_request(64)
        .css_classes(vec!["suggested-action".to_string(), "pill".to_string()])
        .child(&empty_btn_content)
        .visible(false)
        .build();

    let sender_clone = sender.clone();
    empty_new_profile_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::OpenCreateProfileDialog);
    });

    content_container.append(&empty_new_profile_button);

    main_box.append(&content_container);
    (
        main_box,
        new_profile_header_button,
        empty_new_profile_button,
    )
}

pub fn update_profile_list(
    profile_list: &gtk::ListBox,
    new_profile_header_button: &gtk::Button,
    empty_new_profile_button: &gtk::Button,
    profiles: &std::collections::HashMap<String, Profile>,
    sender: &ComponentSender<AppModel>,
) {
    // Clear existing children
    while let Some(child) = profile_list.first_child() {
        profile_list.remove(&child);
    }

    if profiles.is_empty() {
        profile_list.set_visible(false);
        new_profile_header_button.set_visible(false);
        empty_new_profile_button.set_visible(true);
    } else {
        profile_list.set_visible(true);
        new_profile_header_button.set_visible(true);
        empty_new_profile_button.set_visible(false);
        for (name, profile) in profiles {
            let row = create_profile_row(name, profile, sender);
            profile_list.append(&row);
        }
    }
}

fn create_profile_row(
    name: &str,
    profile: &Profile,
    sender: &ComponentSender<AppModel>,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();

    let box_container = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(6)
        .build();

    // Profile info
    let info_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .halign(gtk::Align::Start)
        .build();

    let name_label = gtk::Label::builder()
        .label(&profile.username)
        .halign(gtk::Align::Start)
        .css_classes(vec!["title-4".to_string()])
        .build();

    // Format playtime
    let hours = profile.playtime_seconds / 3600;
    let minutes = (profile.playtime_seconds % 3600) / 60;
    let playtime_str = if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        "No playtime".to_string()
    };

    let version_display = if profile.is_fabric {
        format!("{} (Fabric)", profile.version)
    } else {
        profile.version.clone()
    };

    let details_label = gtk::Label::builder()
        .label(format!(
            "{} • {} MB • {}",
            version_display, profile.ram_mb, playtime_str
        ))
        .halign(gtk::Align::Start)
        .css_classes(vec!["dim-label".to_string()])
        .build();

    info_box.append(&name_label);
    info_box.append(&details_label);

    // Buttons
    let button_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();

    let launch_button = gtk::Button::builder()
        .label("Launch")
        .css_classes(vec!["suggested-action".to_string()])
        .valign(gtk::Align::Center)
        .build();

    let sender_clone = sender.clone();
    let name_clone = name.to_string();
    launch_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::LaunchProfile(name_clone.clone()));
    });

    let delete_button = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .css_classes(vec!["destructive-action".to_string()])
        .valign(gtk::Align::Center)
        .build();

    let sender_clone = sender.clone();
    let name_clone = name.to_string();
    delete_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::RequestDeleteProfile(name_clone.clone()));
    });

    button_box.append(&launch_button);
    button_box.append(&delete_button);

    box_container.append(&info_box);
    box_container.append(&button_box);

    row.set_child(Some(&box_container));
    row.set_activatable(false);
    row
}
