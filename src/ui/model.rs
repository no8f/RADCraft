use crate::auth::MicrosoftAccount;
use crate::launcher::MinecraftLauncher;
use crate::models::{MinecraftVersion, ModSearchResult, ModVersion, Profile, Section};
use crate::modrinth_client::ModrinthClient;
use crate::settings::Settings;
use adw::prelude::*;
use relm4::{ComponentSender, gtk};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum AppState {
    Loading,
    Ready {
        current_section: Section,
    },
    Downloading {
        version: String,
        progress: f64,
        status: String,
    },
    #[allow(dead_code)]
    Launching {
        version: String,
    },
    GameRunning {
        #[allow(dead_code)]
        version: String,
    },
    Error {
        message: String,
    },
}

impl Default for AppState {
    fn default() -> Self {
        AppState::Loading
    }
}

pub struct AppModel {
    pub state: AppState,
    pub launcher: Option<MinecraftLauncher>,
    pub modrinth: ModrinthClient,
    pub window: Option<adw::ApplicationWindow>,

    // Data
    pub profiles: HashMap<String, Profile>,
    pub available_versions: Vec<MinecraftVersion>,
    pub sorted_versions: Vec<String>,

    // Microsoft Account & Auth State
    pub microsoft_account: Option<MicrosoftAccount>,
    pub account_avatar_path: Option<PathBuf>,
    pub auth_dialog_open: bool,
    pub auth_user_code: Option<String>,
    pub auth_verification_uri: Option<String>,
    pub auth_error: Option<String>,
    pub auth_task_handle: Option<tokio::task::AbortHandle>,

    // Inputs
    pub input_username: String,
    pub input_version: Option<String>,
    pub input_ram: u32,
    pub input_install_fabric: bool,
    pub fabric_switch_enabled: bool,
    pub create_dialog_open: bool,

    // Settings & Logs
    pub settings: Settings,
    pub logs: gtk::TextBuffer,

    // UI State
    pub error_message: Option<String>,

    pub sidebar_collapsed: bool,

    pub versions_updated: bool,
    pub version_list_model: Option<gtk::StringList>,
    pub is_searching: bool,

    // Mods UI State
    pub mod_search_results: Vec<ModSearchResult>,
    pub mod_versions: HashMap<String, Vec<ModVersion>>,
    pub selected_mod_versions: HashMap<String, String>,
    pub mod_search_entry: Option<gtk::SearchEntry>,
    pub mod_browse_list: Option<gtk::ListBox>,
    pub mod_installed_list: Option<gtk::ListBox>,
    pub selected_mod_profile: Option<String>,
    pub mod_profile_list_model: Option<gtk::StringList>,

    // Track installed mods: ProjectID -> Filename
    pub installed_mods: HashMap<String, String>,

    pub toast_overlay: Option<adw::ToastOverlay>,

    // Icon Download Queue
    pub icon_download_queue: VecDeque<(String, String)>, // (ProjectID, URL)
    pub is_downloading_icon: bool,

    // Selection Sync
    pub pending_mod_selection: Option<u32>,
    pub pending_launch_profile: Option<String>,
    pub mod_profile_list_updated: bool,

    // Component sender for UI updates
    pub sender: ComponentSender<AppModel>,

    pub java_dialog_request: Option<u32>,

    // Shared Tokio Runtime
    pub rt: std::sync::Arc<tokio::runtime::Runtime>,
}

impl AppModel {
    pub fn is_selected_version_installed(&self, project_id: &str) -> bool {
        let Some(installed_file) = self.installed_mods.get(project_id) else {
            return false;
        };
        if let Some(versions) = self.mod_versions.get(project_id) {
            if let Some(selected_vid) = self.selected_mod_versions.get(project_id) {
                if let Some(ver) = versions.iter().find(|v| &v.id == selected_vid) {
                    return ver.files.iter().any(|f| &f.filename == installed_file);
                }
            }
        }
        true
    }

    // Helper to update button state based on installation status
    pub fn update_mod_button_state(&self, project_id: &str) {
        if let Some(list) = &self.mod_browse_list {
            let is_installed = self.is_selected_version_installed(project_id);
            let has_versions = self
                .mod_versions
                .get(project_id)
                .map(|v| !v.is_empty())
                .unwrap_or(true);
            let icon_name = if is_installed {
                "user-trash-symbolic"
            } else {
                "folder-download-symbolic"
            };
            let tooltip = if is_installed { "Uninstall" } else { "Install" };
            let sensitive = has_versions || is_installed;

            let mut sibling = list.first_child();
            while let Some(child) = sibling {
                if let Some(row) = child.downcast_ref::<gtk::ListBoxRow>() {
                    if let Some(box_widget) = row.child() {
                        if let Some(bx) = box_widget.downcast_ref::<gtk::Box>() {
                            let mut box_child = bx.first_child();
                            while let Some(b_child) = box_child {
                                if let Some(button) = b_child.downcast_ref::<gtk::Button>() {
                                    if button.widget_name() == format!("btn_{}", project_id) {
                                        button.set_icon_name(icon_name);
                                        button.set_tooltip_text(Some(tooltip));
                                        button.set_sensitive(sensitive);

                                        if is_installed {
                                            button.add_css_class("destructive-action");
                                        } else {
                                            button.remove_css_class("destructive-action");
                                        }
                                        break;
                                    }
                                }
                                box_child = b_child.next_sibling();
                            }
                        }
                    }
                }
                sibling = child.next_sibling();
            }
        }
    }

    pub fn update_mod_versions_dropdown(
        &self,
        project_id: &str,
        versions: &[ModVersion],
        selected_idx: u32,
    ) {
        if let Some(list) = &self.mod_browse_list {
            let target_name = format!("ver_{}", project_id);
            let mut sibling = list.first_child();
            while let Some(child) = sibling {
                if let Some(row) = child.downcast_ref::<gtk::ListBoxRow>() {
                    if let Some(box_widget) = row.child() {
                        if let Some(bx) = box_widget.downcast_ref::<gtk::Box>() {
                            let mut box_child = bx.first_child();
                            while let Some(b_child) = box_child {
                                if let Some(dropdown) = b_child.downcast_ref::<gtk::DropDown>() {
                                    if dropdown.widget_name() == target_name {
                                        dropdown.remove_css_class("mod-version-release");
                                        dropdown.remove_css_class("mod-version-beta");
                                        dropdown.remove_css_class("mod-version-alpha");

                                        if versions.is_empty() {
                                            let model =
                                                gtk::StringList::new(&["|none|Unavailable"]);
                                            dropdown.set_model(Some(&model));
                                            dropdown.set_sensitive(false);
                                        } else {
                                            let items: Vec<String> = versions
                                                .iter()
                                                .map(|v| {
                                                    format!(
                                                        "{}|{}|{}",
                                                        v.id,
                                                        v.version_type.to_lowercase(),
                                                        v.version_number
                                                    )
                                                })
                                                .collect();
                                            let item_refs: Vec<&str> =
                                                items.iter().map(|s| s.as_str()).collect();
                                            let model = gtk::StringList::new(&item_refs);
                                            dropdown.set_model(Some(&model));
                                            dropdown.set_sensitive(true);
                                            dropdown.set_selected(selected_idx);

                                            if let Some(v) = versions.get(selected_idx as usize) {
                                                match v.version_type.to_lowercase().as_str() {
                                                    "release" => dropdown
                                                        .add_css_class("mod-version-release"),
                                                    "beta" => {
                                                        dropdown.add_css_class("mod-version-beta")
                                                    }
                                                    "alpha" => {
                                                        dropdown.add_css_class("mod-version-alpha")
                                                    }
                                                    _ => dropdown
                                                        .add_css_class("mod-version-release"),
                                                }
                                            }
                                        }
                                        return;
                                    }
                                }
                                box_child = b_child.next_sibling();
                            }
                        }
                    }
                }
                sibling = child.next_sibling();
            }
        }
    }
}
