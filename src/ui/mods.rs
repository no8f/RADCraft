use crate::models::ModSearchResult;
use crate::ui::model::AppModel;
use crate::ui::msg::AppMsg;
use gtk::prelude::*;
use relm4::gtk;
use relm4::{ComponentSender, RelmWidgetExt};

pub fn create_mods_page(
    sender: &ComponentSender<AppModel>,
) -> (
    gtk::Box,
    gtk::SearchEntry,
    gtk::Button,
    gtk::Stack,
    gtk::ListBox,
    gtk::ListBox,
    gtk::DropDown,
) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 24);
    container.set_margin_all(24);
    container.set_vexpand(true);
    container.set_hexpand(true);
    container.set_halign(gtk::Align::Fill);

    // Title label
    let title_label = gtk::Label::builder()
        .label("Mods")
        .halign(gtk::Align::Start)
        .css_classes(vec!["title-1".to_string()])
        .build();

    container.append(&title_label);

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
    stack.set_vexpand(true);
    stack.set_hexpand(true);
    stack.set_halign(gtk::Align::Fill);

    let top_bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    // top_bar.set_margin_all(12); // Removed extra margin to align with new container padding

    let profile_model = gtk::StringList::new(&[]);
    let profile_dropdown = gtk::DropDown::builder()
        .model(&profile_model)
        .hexpand(true)
        .build();
    profile_dropdown.set_hexpand(true);

    let sender_clone = sender.clone();
    profile_dropdown.connect_selected_item_notify(move |dropdown| {
        if let Some(item) = dropdown.selected_item() {
            if let Some(string_obj) = item.downcast_ref::<gtk::StringObject>() {
                let full_string = string_obj.string().to_string();
                if let Some((name, version)) = full_string.rsplit_once(" - ") {
                    let key = format!("{}_{}_fabric", name, version);
                    sender_clone.input(AppMsg::SelectModProfile(key));
                }
            }
        }
    });

    let installed_btn_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    installed_btn_content.append(&gtk::Image::from_icon_name("folder-download-symbolic"));
    installed_btn_content.append(&gtk::Label::new(Some("Installed")));

    let installed_button = gtk::Button::builder().child(&installed_btn_content).build();
    installed_button.add_css_class("suggested-action");

    let browse_btn_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    browse_btn_content.append(&gtk::Image::from_icon_name("system-search-symbolic"));
    browse_btn_content.append(&gtk::Label::new(Some("Browse")));

    let browse_button = gtk::Button::builder().child(&browse_btn_content).build();

    let stack_clone = stack.clone();
    let _installed_btn_clone = installed_button.clone();
    let browse_btn_clone = browse_button.clone();

    installed_button.connect_clicked(move |btn| {
        stack_clone.set_visible_child_name("installed");
        btn.add_css_class("suggested-action");
        browse_btn_clone.remove_css_class("suggested-action");
    });

    let stack_clone = stack.clone();
    let installed_btn_clone = installed_button.clone();
    let _browse_btn_clone = browse_button.clone();

    browse_button.connect_clicked(move |btn| {
        stack_clone.set_visible_child_name("browse");
        btn.add_css_class("suggested-action");
        installed_btn_clone.remove_css_class("suggested-action");
    });

    top_bar.append(&profile_dropdown);
    top_bar.append(&installed_button);
    top_bar.append(&browse_button);

    let installed_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    // installed_box.set_margin_all(12); // Removed to fix alignment
    installed_box.set_vexpand(true);
    installed_box.set_hexpand(true);
    installed_box.set_halign(gtk::Align::Fill);

    let installed_list = gtk::ListBox::new();
    installed_list.add_css_class("boxed-list");

    let installed_scroll = gtk::ScrolledWindow::new();
    installed_scroll.set_vexpand(true);
    installed_scroll.set_hexpand(true);
    installed_scroll.set_halign(gtk::Align::Fill);
    installed_scroll.set_child(Some(&installed_list));

    // Toolbar removed as per request

    installed_box.append(&installed_scroll);

    stack.add_named(&installed_box, Some("installed"));

    let browse_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    // browse_box.set_margin_all(12); // Removed to fix alignment
    browse_box.set_vexpand(true);
    browse_box.set_hexpand(true);
    browse_box.set_halign(gtk::Align::Fill);

    let search_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);

    let search_bar = gtk::SearchEntry::new();
    search_bar.set_placeholder_text(Some("Search Modrinth..."));
    search_bar.set_hexpand(true);

    let search_spinner = gtk::Spinner::new();

    let search_button = gtk::Button::builder()
        .icon_name("system-search-symbolic")
        .tooltip_text("Search")
        .build();

    let _search_spinner = gtk::Spinner::builder().spinning(true).build();

    let search_stack = gtk::Stack::new();
    search_stack.add_named(&search_button, Some("button"));
    search_stack.add_named(&search_spinner, Some("spinner"));
    search_stack.set_visible_child_name("button");
    search_stack.set_transition_type(gtk::StackTransitionType::Crossfade);

    search_box.append(&search_bar);
    search_box.append(&search_stack);

    let browse_list = gtk::ListBox::new();
    browse_list.add_css_class("boxed-list");
    browse_list.set_selection_mode(gtk::SelectionMode::None);

    let browse_scroll = gtk::ScrolledWindow::new();
    browse_scroll.set_vexpand(true);
    browse_scroll.set_hexpand(true);
    browse_scroll.set_halign(gtk::Align::Fill);
    browse_scroll.set_child(Some(&browse_list));

    browse_box.append(&search_box);
    browse_box.append(&browse_scroll);

    stack.add_named(&browse_box, Some("browse"));

    container.append(&top_bar);
    container.append(&stack);

    stack.set_visible_child_name("installed");

    (
        container,
        search_bar,
        search_button,
        search_stack,
        installed_list,
        browse_list,
        profile_dropdown,
    )
}

fn create_version_item_factory() -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup(|_, obj| {
        if let Some(list_item) = obj.downcast_ref::<gtk::ListItem>() {
            let item_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(6)
                .valign(gtk::Align::Center)
                .build();

            let badge = gtk::Label::builder().valign(gtk::Align::Center).build();

            let ver_label = gtk::Label::builder()
                .valign(gtk::Align::Center)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .max_width_chars(18)
                .build();

            item_box.append(&badge);
            item_box.append(&ver_label);
            list_item.set_child(Some(&item_box));
        }
    });

    factory.connect_bind(|_, obj| {
        if let Some(list_item) = obj.downcast_ref::<gtk::ListItem>() {
            let Some(item) = list_item.item() else { return };
            let Some(string_obj) = item.downcast_ref::<gtk::StringObject>() else {
                return;
            };
            let Some(child) = list_item.child() else {
                return;
            };
            let Some(item_box) = child.downcast_ref::<gtk::Box>() else {
                return;
            };

            let Some(first) = item_box.first_child() else {
                return;
            };
            let Some(second) = first.next_sibling() else {
                return;
            };
            let Some(badge) = first.downcast_ref::<gtk::Label>() else {
                return;
            };
            let Some(ver_label) = second.downcast_ref::<gtk::Label>() else {
                return;
            };

            let raw = string_obj.string();
            let parts: Vec<&str> = raw.splitn(3, '|').collect();
            let v_type = parts.get(1).copied().unwrap_or("none");
            let v_num = parts.get(2).copied().unwrap_or(raw.as_str());

            badge.remove_css_class("mod-badge-release");
            badge.remove_css_class("mod-badge-beta");
            badge.remove_css_class("mod-badge-alpha");

            match v_type {
                "release" => {
                    badge.set_text("Release");
                    badge.add_css_class("mod-badge-release");
                    badge.set_visible(true);
                }
                "beta" => {
                    badge.set_text("Beta");
                    badge.add_css_class("mod-badge-beta");
                    badge.set_visible(true);
                }
                "alpha" => {
                    badge.set_text("Alpha");
                    badge.add_css_class("mod-badge-alpha");
                    badge.set_visible(true);
                }
                _ => {
                    badge.set_visible(false);
                }
            }

            ver_label.set_text(v_num);
        }
    });

    factory
}

pub fn create_mod_search_result_row(
    mod_data: &ModSearchResult,
    sender: &ComponentSender<AppModel>,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let box_container = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    box_container.set_margin_all(12);

    let icon = gtk::Image::from_icon_name("extension");
    icon.set_pixel_size(48);
    icon.set_widget_name(&mod_data.project_id);

    let info_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let title = gtk::Label::builder()
        .label(&mod_data.title)
        .halign(gtk::Align::Start)
        .css_classes(vec!["title-4"])
        .build();

    let desc_text = mod_data.description.clone().unwrap_or_default();
    let description = gtk::Label::builder()
        .label(&desc_text)
        .halign(gtk::Align::Start)
        .wrap(true)
        .max_width_chars(45)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(2)
        .build();

    info_box.append(&title);
    info_box.append(&description);

    // Colored Version Combobox (Release / Beta / Alpha)
    let initial_versions = gtk::StringList::new(&["|loading|Loading..."]);
    let button_factory = create_version_item_factory();
    let list_factory = create_version_item_factory();

    let version_dropdown = gtk::DropDown::builder()
        .model(&initial_versions)
        .factory(&button_factory)
        .list_factory(&list_factory)
        .valign(gtk::Align::Center)
        .tooltip_text("Select mod version (Release / Beta / Alpha)")
        .sensitive(false)
        .name(&format!("ver_{}", mod_data.project_id))
        .css_classes(vec!["mod-version-combo".to_string()])
        .build();

    let project_id_ver = mod_data.project_id.clone();
    let sender_ver = sender.clone();
    version_dropdown.connect_selected_item_notify(move |dropdown| {
        dropdown.remove_css_class("mod-version-release");
        dropdown.remove_css_class("mod-version-beta");
        dropdown.remove_css_class("mod-version-alpha");

        if let Some(item) = dropdown.selected_item() {
            if let Some(string_obj) = item.downcast_ref::<gtk::StringObject>() {
                let raw = string_obj.string();
                let parts: Vec<&str> = raw.splitn(3, '|').collect();
                let v_id = parts.first().copied().unwrap_or("");
                let v_type = parts.get(1).copied().unwrap_or("none");

                match v_type {
                    "release" => dropdown.add_css_class("mod-version-release"),
                    "beta" => dropdown.add_css_class("mod-version-beta"),
                    "alpha" => dropdown.add_css_class("mod-version-alpha"),
                    _ => {}
                }

                if !v_id.is_empty() {
                    sender_ver.input(AppMsg::SelectModVersion(
                        project_id_ver.clone(),
                        v_id.to_string(),
                    ));
                }
            }
        }
    });

    let download_button = gtk::Button::builder()
        .icon_name("folder-download-symbolic")
        .tooltip_text("Install")
        .valign(gtk::Align::Center)
        .name(&format!("btn_{}", mod_data.project_id))
        .build();

    let project_id = mod_data.project_id.clone();
    let sender_clone = sender.clone();
    download_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::ModActionButtonClicked(project_id.clone()));
    });

    let view_button = gtk::Button::builder()
        .icon_name("web-browser-symbolic")
        .tooltip_text("View on Modrinth")
        .valign(gtk::Align::Center)
        .build();

    let project_id_clone_2 = mod_data.project_id.clone();
    let sender_clone_2 = sender.clone();
    view_button.connect_clicked(move |_| {
        sender_clone_2.input(AppMsg::OpenModrinthPage(project_id_clone_2.clone()));
    });

    box_container.append(&icon);
    box_container.append(&info_box);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    box_container.append(&spacer);
    box_container.append(&version_dropdown);
    box_container.append(&view_button);
    box_container.append(&download_button);

    row.set_child(Some(&box_container));
    row
}
