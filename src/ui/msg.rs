use crate::auth::MicrosoftAccount;
use crate::models::{MinecraftVersion, ModSearchResult, Profile, Section, Theme};
use crate::settings::Settings;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug)]
pub enum AppMsg {
    LaunchProfile(String),
    DeleteProfile(String),
    UsernameChanged(String),
    VersionSelected(String),
    RamChanged(u32),
    ToggleFabric(bool),
    SaveProfile,
    OpenCreateProfileDialog,
    CloseCreateProfileDialog,
    ShowAboutDialog,
    AccountButtonClicked,
    DeviceCodeReceived(Result<(String, String), String>),
    CopyCodeAndOpenBrowser,
    CancelMicrosoftLogin,
    MicrosoftLoginSuccess(MicrosoftAccount),
    MicrosoftLoginFailed(String),
    MicrosoftAccountLoaded(Option<MicrosoftAccount>),
    AccountAvatarLoaded(Option<PathBuf>),
    MicrosoftLogout,
    VersionsLoaded(Result<Vec<MinecraftVersion>, String>),
    ProfilesLoaded(Result<HashMap<String, Profile>, String>),
    DownloadProgress(f64, String),
    GameStarted,
    LaunchCompleted,
    NavigateToSection(Section),
    BackToMainMenu,
    OpenMinecraftFolder,
    ThemeSelected(Theme),
    ToggleHideLogs(bool),
    ToggleHideMods(bool),
    ToggleSidebar,
    Log(String),

    Error(String),
    RequestDeleteProfile(String),
    SettingsLoaded(Settings),
    SessionEnded(String, u64),
    RefreshInstalledMods,
    SelectModProfile(String),
    // Modrinth Messages
    SearchMods(String),
    ModsSearched(Result<Vec<ModSearchResult>, String>),
    InstallMod(String),                // Project ID
    UninstallMod(String),              // Filename
    DownloadModIcon(String, String),   // Project ID, URL
    ModIconDownloaded(String, String), // project_id, path
    ProcessIconQueue,
    ModActionButtonClicked(String), // project_id (Toggle Install/Uninstall)
    ModInstallFinished(String, ()), // project_id, success (bool unused)
    ModUninstallFinished(String),   // project_id
    RegisterInstalledMod(String, String), // project_id, filename
    ShowToast(String),
    ClearPendingSelection,
    ModDropdownUpdated,
    OpenModrinthPage(String),
    ShowJavaDialog(u32),
    JavaDownloadConfirmed,
    JavaDownloadCancelled,
    InstallJavaAndLaunch,
}
