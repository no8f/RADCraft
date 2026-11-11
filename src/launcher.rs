use anyhow::{Result, anyhow};

use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::fs;
use tokio::process::Command as TokioCommand;

use crate::auth::MicrosoftAccount;
use crate::config::LauncherConfig;
use crate::java_manager::JavaManager;
use crate::library_manager::LibraryManager;
use crate::models::{AssetIndexFile, MinecraftVersion, VersionJson, VersionManifest};
use crate::utils::is_library_allowed;
use futures::stream::{self, StreamExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::AsyncWriteExt;

#[derive(Clone)]
pub struct MinecraftLauncher {
    pub config: LauncherConfig,
    pub java_manager: JavaManager,
    pub library_manager: LibraryManager,
}

impl MinecraftLauncher {
    pub fn new() -> Result<Self> {
        let config = LauncherConfig::new()?;
        let java_manager = JavaManager::new(config.runtimes_dir.clone());
        let library_manager = LibraryManager::new(config.versions_dir.clone());
        Ok(Self {
            config,
            java_manager,
            library_manager,
        })
    }

    pub async fn get_available_versions(&self) -> Result<Vec<MinecraftVersion>> {
        let url = "https://launchermeta.mojang.com/mc/game/version_manifest.json";
        let response = reqwest::get(url).await?;
        let manifest: VersionManifest = response.json().await?;

        let release_versions: Vec<MinecraftVersion> = manifest
            .versions
            .into_iter()
            .filter(|v| v.version_type == "release")
            .collect();

        Ok(release_versions)
    }

    fn heuristic_java_version(version: &str) -> u32 {
        let version_id =
            if version.contains("fabric") || version.contains("quilt") || version.contains("forge")
            {
                version.split('-').last().unwrap_or(version)
            } else {
                version
            };

        let parts: Vec<&str> = version_id.split('.').collect();
        if parts.len() >= 2 {
            if let (Ok(major), Ok(minor)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                if major >= 26 {
                    return 25;
                }
                if major == 1 {
                    if minor > 20 {
                        return 21;
                    } else if minor == 20 {
                        if let Some(sub_str) = parts.get(2) {
                            if let Ok(sub) = sub_str.parse::<u32>() {
                                if sub >= 5 {
                                    return 21;
                                }
                            }
                        }
                        return 17;
                    } else if minor >= 18 {
                        return 17;
                    } else if minor == 17 {
                        return 16;
                    }
                }
            }
        }

        8 // Default for older versions without java_version field
    }

    pub async fn get_required_java_version(&self, version: &str) -> Result<u32> {
        let version_dir = self.config.versions_dir.join(version);
        let version_file = version_dir.join(format!("{}.json", version));

        if !version_file.exists() {
            // Ensure the version JSON is downloaded before falling back to heuristics
            let _ = self.ensure_version_ready(version).await;
        }

        if !version_file.exists() {
            return Ok(Self::heuristic_java_version(version));
        }

        let version_data = fs::read_to_string(&version_file).await?;
        let version_json: VersionJson = serde_json::from_str(&version_data)?;

        // Check java_version field
        if let Some(v) = version_json.java_version {
            return Ok(v.major_version);
        }

        if let Some(parent_id) = version_json.inherits_from {
            // Recursive check
            return Box::pin(self.get_required_java_version(&parent_id)).await;
        }

        // Fallback heuristic check on the ID itself if it looks like a vanilla version
        Ok(Self::heuristic_java_version(version))
    }

    pub async fn prepare_java<F>(&self, version: &str, on_progress: F) -> Result<PathBuf>
    where
        F: Fn(f64, String) + Send + Sync + 'static + Clone,
    {
        let required_version = self.get_required_java_version(version).await?;

        // Try to find valid java locally
        if let Ok(path) = self.java_manager.find_java(Some(required_version)) {
            return Ok(path);
        }

        // Not found, download
        let path = self
            .java_manager
            .download_and_install_java(required_version, on_progress)
            .await?;

        Ok(path)
    }

    pub async fn build_classpath(&self, start_version: &str) -> Result<String> {
        let mut classpath_paths: Vec<PathBuf> = Vec::new();
        let mut seen_artifacts: std::collections::HashSet<String> =
            std::collections::HashSet::new(); // group:artifact[:classifier]
        let os_name = crate::utils::get_os_name();

        let mut current_version_id = Some(start_version.to_string());
        let mut vanilla_jar_path: Option<PathBuf> = None;

        while let Some(version) = current_version_id {
            let version_dir = self.config.versions_dir.join(&version);
            let version_file = version_dir.join(format!("{}.json", version));

            if !version_file.exists() {
                if version == start_version {
                    return Err(anyhow!("Version JSON not found: {:?}", version_file));
                } else {
                    break;
                }
            }

            let version_data = tokio::fs::read_to_string(&version_file).await?;
            let version_json: VersionJson = serde_json::from_str(&version_data)?;

            for lib in &version_json.libraries {
                let allowed = is_library_allowed(lib, os_name);
                if !allowed {
                    continue;
                }

                let mut lib_path_buf: Option<PathBuf> = None;
                let mut maven_key: Option<String> = None;

                if let Some(downloads) = &lib.downloads {
                    if let Some(artifact) = &downloads.artifact {
                        lib_path_buf = Some(self.config.libraries_dir.join(&artifact.path));
                    }
                }

                if lib_path_buf.is_none() {
                    let parts: Vec<&str> = lib.name.split(':').collect();
                    if parts.len() >= 2 {
                        let group = parts[0].replace('.', "/");
                        let artifact_id = parts[1];
                        let version = parts.get(2).unwrap_or(&"");
                        let suffix = if parts.len() > 3 {
                            format!("-{}", parts[3])
                        } else {
                            "".to_string()
                        };

                        let path = format!(
                            "{}/{}/{}/{}-{}{}.jar",
                            group, artifact_id, version, artifact_id, version, suffix
                        );
                        let p = self.config.libraries_dir.join(path);
                        if p.exists() {
                            lib_path_buf = Some(p);
                        }
                    }
                }

                let parts: Vec<&str> = lib.name.split(':').collect();
                if parts.len() >= 2 {
                    let key = if let Some(classifier) = parts.get(3) {
                        format!("{}:{}:{}", parts[0], parts[1], classifier)
                    } else {
                        format!("{}:{}", parts[0], parts[1])
                    };
                    maven_key = Some(key);
                }

                if let (Some(path), Some(key)) = (lib_path_buf, maven_key) {
                    if !seen_artifacts.contains(&key) {
                        seen_artifacts.insert(key);
                        classpath_paths.push(path);
                    }
                }
            }

            if let Some(parent) = version_json.inherits_from {
                current_version_id = Some(parent);
            } else {
                // Base version (Vanilla) -> jar path
                let jar_path = version_dir.join(format!("{}.jar", version));
                vanilla_jar_path = Some(jar_path);
                current_version_id = None;
            }
        }

        if let Some(jar) = vanilla_jar_path {
            classpath_paths.push(jar);
        }

        let cp_string = classpath_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(":");

        Ok(cp_string)
    }

    async fn download_file(url: &str, path: &Path) -> Result<()> {
        if path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let response = reqwest::get(url).await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to download file from {}: {}",
                url,
                response.status()
            ));
        }
        let bytes = response.bytes().await?;

        // Ensure parent dir exists again just in case (race condition in parallel)
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = fs::File::create(path).await?;
        file.write_all(&bytes).await?;
        Ok(())
    }

    async fn download_file_with_progress<F>(
        url: &str,
        path: &Path,
        label: &str,
        on_progress: Option<F>,
    ) -> Result<()>
    where
        F: Fn(f64, String) + Send + Sync + 'static + Clone,
    {
        if path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let response = reqwest::get(url).await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to download file from {}: {}",
                url,
                response.status()
            ));
        }
        let total_size = response.content_length().unwrap_or(0);
        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;

        let mut file = fs::File::create(path).await?;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            downloaded += chunk.len() as u64;
            file.write_all(&chunk).await?;

            if total_size > 0 {
                if let Some(cb) = &on_progress {
                    let pct = downloaded as f64 / total_size as f64;
                    cb(
                        pct,
                        format!(
                            "{} ({:.1} / {:.1} MB)",
                            label,
                            downloaded as f64 / 1024.0 / 1024.0,
                            total_size as f64 / 1024.0 / 1024.0
                        ),
                    );
                }
            }
        }
        file.flush().await?;
        Ok(())
    }

    pub async fn prepare_assets<F>(
        &self,
        version_json: &VersionJson,
        on_progress: Option<F>,
    ) -> Result<()>
    where
        F: Fn(f64, String) + Send + Sync + 'static + Clone,
    {
        if let Some(asset_index) = &version_json.asset_index {
            let indexes_dir = self.config.assets_dir.join("indexes");
            let index_path = indexes_dir.join(format!("{}.json", asset_index.id));

            Self::download_file(&asset_index.url, &index_path).await?;

            let index_content = fs::read_to_string(&index_path).await?;
            let index: AssetIndexFile = serde_json::from_str(&index_content)?;

            let objects_dir = self.config.assets_dir.join("objects");
            let legacy_virtual_dir = self.config.assets_dir.join("virtual/legacy");

            if index.is_virtual {
                fs::create_dir_all(&legacy_virtual_dir).await?;
            }

            // Collect all objects that need processing
            let mut pending_objects = Vec::new();
            for (name, object) in index.objects {
                // Check if we need to download or copy virtual
                let hash_head = &object.hash[0..2];
                let object_path = objects_dir.join(hash_head).join(&object.hash);

                let needs_download = !object_path.exists();
                let needs_virtual = index.is_virtual && !legacy_virtual_dir.join(&name).exists();

                if needs_download || needs_virtual {
                    pending_objects.push((
                        name,
                        object,
                        object_path,
                        needs_download,
                        needs_virtual,
                    ));
                }
            }

            let total_items = pending_objects.len();
            let processed_count = Arc::new(AtomicUsize::new(0));

            if total_items > 0 {
                if let Some(cb) = &on_progress {
                    cb(0.0, format!("Downloading {} assets...", total_items));
                }

                // Concurrent download using buffered stream
                let bodies = stream::iter(pending_objects)
                    .map(
                        |(name, object, object_path, needs_download, needs_virtual)| {
                            let processed_count = processed_count.clone();
                            let on_progress = on_progress.clone();
                            let legacy_virtual_dir = legacy_virtual_dir.clone();

                            async move {
                                if needs_download {
                                    let hash_head = &object.hash[0..2];
                                    let url = format!(
                                        "https://resources.download.minecraft.net/{}/{}",
                                        hash_head, object.hash
                                    );
                                    if let Err(e) = Self::download_file(&url, &object_path).await {
                                        eprintln!("Failed to download asset {}: {}", name, e);
                                        // Continue anyway, don't fail everything for one asset
                                    }
                                }

                                if needs_virtual {
                                    let virtual_path = legacy_virtual_dir.join(&name);
                                    if !virtual_path.exists() {
                                        if let Some(parent) = virtual_path.parent() {
                                            let _ = fs::create_dir_all(parent).await;
                                        }
                                        let _ = fs::copy(&object_path, &virtual_path).await;
                                    }
                                }

                                let current = processed_count.fetch_add(1, Ordering::SeqCst) + 1;
                                if current % 10 == 0 || current == total_items {
                                    if let Some(cb) = &on_progress {
                                        cb(
                                            current as f64 / total_items as f64,
                                            format!(
                                                "Downloading assets: {}/{}",
                                                current, total_items
                                            ),
                                        );
                                    }
                                }
                            }
                        },
                    )
                    .buffer_unordered(20); // Parallel downloads

                bodies.collect::<Vec<()>>().await;
            }
        }
        Ok(())
    }

    pub async fn ensure_version_ready(&self, version: &str) -> Result<()> {
        let version_dir = self.config.versions_dir.join(version);
        let version_file = version_dir.join(format!("{}.json", version));

        if version_file.exists() {
            return Ok(());
        }

        // Need to find URL from manifest
        let manifest = self.get_available_versions().await?;
        let version_info = manifest.iter().find(|v| v.id == version);

        if let Some(v_info) = version_info {
            Self::download_file(&v_info.url, &version_file).await?;
            Ok(())
        } else {
            Err(anyhow!("Version {} not found in manifest", version))
        }
    }

    pub async fn launch_minecraft<F>(
        &self,
        version: &str,
        username: &str,
        ram_mb: u32,
        game_dir: &Path,
        account: Option<&MicrosoftAccount>,
        on_progress: Option<F>,
    ) -> Result<TokioCommand>
    where
        F: Fn(f64, String) + Send + Sync + 'static + Clone,
    {
        self.ensure_version_ready(version).await?;

        let version_dir = self.config.versions_dir.join(version);
        let version_file = version_dir.join(format!("{}.json", version));

        let version_data = fs::read_to_string(&version_file).await?;
        let version_json: VersionJson = serde_json::from_str(&version_data)?;

        let required_java = self.get_required_java_version(version).await?;
        let java_path = self.java_manager.find_java(Some(required_java))?;

        let jar_version = version_json.inherits_from.as_deref().unwrap_or(version);
        let jar_dir = self.config.versions_dir.join(jar_version);
        let jar_path = jar_dir.join(format!("{}.jar", jar_version));

        // Create jar directory if it doesn't exist (e.g for new versions)
        if !jar_dir.exists() {
            fs::create_dir_all(&jar_dir).await?;
        }

        // If inheriting, ensure parent JSON is ready (so we can get download URL and assetIndex)
        if jar_version != version {
            self.ensure_version_ready(jar_version).await?;
        }

        // Load parent/base VersionJson when inheriting so we have client download URL, assetIndex, and mainClass
        let source_json: VersionJson = if jar_version == version {
            version_json.clone()
        } else {
            let v_dir = self.config.versions_dir.join(jar_version);
            let v_file = v_dir.join(format!("{}.json", jar_version));
            let d = fs::read_to_string(&v_file).await?;
            serde_json::from_str(&d)?
        };

        // Check/Download JAR
        if !jar_path.exists() {
            if let Some(downloads) = &source_json.downloads {
                if let Some(client) = &downloads.client {
                    let jar_cb = on_progress.clone().map(|cb| {
                        move |pct: f64, msg: String| {
                            cb(0.20 + 0.20 * pct, msg);
                        }
                    });
                    Self::download_file_with_progress(
                        &client.url,
                        &jar_path,
                        &format!("Downloading Minecraft {}...", jar_version),
                        jar_cb,
                    )
                    .await?;
                }
            }
        }

        if !jar_path.exists() {
            // If still not exists, try to fallback to main version jar if inherits is present but we are launching child
            return Err(anyhow!(
                "Version JAR not found at: {:?} and no download URL available",
                jar_path
            ));
        }

        let natives_version = version_json.inherits_from.as_deref().unwrap_or(version);
        let natives_dir = self
            .config
            .versions_dir
            .join(natives_version)
            .join("natives");

        // Check/Repair Natives
        if let Some(cb) = &on_progress {
            cb(0.40, "Checking native libraries...".to_string());
        }
        self.library_manager
            .check_and_extract_natives(natives_version)
            .await?;

        // Check/Download Libraries
        let lib_cb = on_progress.clone().map(|cb| {
            move |pct: f64, msg: String| {
                cb(0.45 + 0.20 * pct, msg);
            }
        });
        self.library_manager
            .check_and_download_libraries(natives_version, lib_cb)
            .await?;

        // Prepare Assets (use source_json if version_json inherits from a base version without its own assetIndex)
        let assets_json = if version_json.asset_index.is_some() {
            &version_json
        } else {
            &source_json
        };
        let assets_cb = on_progress.clone().map(|cb| {
            move |pct: f64, msg: String| {
                cb(0.65 + 0.33 * pct, msg);
            }
        });
        self.prepare_assets(assets_json, assets_cb).await?;

        let main_class = version_json
            .main_class
            .clone()
            .or_else(|| source_json.main_class.clone())
            .unwrap_or_else(|| "net.minecraft.client.main.Main".to_string());
        let asset_index_id = version_json
            .asset_index
            .as_ref()
            .or(source_json.asset_index.as_ref())
            .map(|a| a.id.clone());

        let classpath = self.build_classpath(version).await?;

        let launch_username = account.map(|a| a.username.as_str()).unwrap_or(username);

        let mut command = TokioCommand::new(java_path);
        command
            .arg("-Xmx".to_string() + &ram_mb.to_string() + "M")
            .arg("-Xms".to_string() + &(ram_mb / 2).to_string() + "M")
            .arg("-Djava.library.path=".to_string() + &natives_dir.display().to_string())
            .arg("-cp")
            .arg(classpath)
            .arg(main_class)
            .arg("--username")
            .arg(launch_username)
            .arg("--version")
            .arg(version)
            .arg("--gameDir")
            .arg(game_dir)
            .arg("--assetsDir")
            .arg(&self.config.assets_dir);

        if let Some(id) = asset_index_id {
            command.arg("--assetIndex").arg(id);
        }

        if let Some(acc) = account {
            command
                .arg("--uuid")
                .arg(&acc.uuid)
                .arg("--accessToken")
                .arg(&acc.access_token)
                .arg("--userType")
                .arg("msa");
        } else {
            command.arg("--accessToken").arg("0");
        }

        command
            .arg("--userProperties")
            .arg("{}")
            .current_dir(&version_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Ok(command)
    }

    // High Level Launch Orchestration
    pub async fn prepare_and_launch<F>(
        &self,
        base_version: String,
        username: String,
        ram_mb: u32,
        is_fabric: bool,
        game_dir_override: Option<PathBuf>,
        account: Option<MicrosoftAccount>,
        on_progress: F,
    ) -> Result<TokioCommand>
    where
        F: Fn(f64, String) + Send + Sync + 'static + Clone,
    {
        let mut version_to_launch = base_version.clone();

        // Refresh Microsoft token if signed in and expired
        let active_account = if let Some(acc) = account {
            if acc.is_expired() {
                on_progress(0.02, "Refreshing Microsoft account session...".into());
                match crate::auth::refresh_minecraft_account(&acc).await {
                    Ok(refreshed) => {
                        let _ = refreshed.save(&self.config.minecraft_dir).await;
                        Some(refreshed)
                    }
                    Err(e) => {
                        return Err(anyhow!(
                            "Microsoft session expired and refresh failed: {}",
                            e
                        ));
                    }
                }
            } else {
                Some(acc)
            }
        } else {
            None
        };

        // 1. Check JAVA FIRST (Before Fabric)
        on_progress(0.05, "Verifying Java...".into());
        let required_java = self.get_required_java_version(&base_version).await?;

        let java_p = match self.java_manager.find_java(Some(required_java)) {
            Ok(p) => p,
            Err(_) => {
                return Err(anyhow!(
                    "Java Runtime {} is missing. Please ensure it is installed.",
                    required_java
                ));
            }
        };

        // 2. Handle Fabric
        if is_fabric {
            on_progress(0.10, "Checking Fabric...".into());
            let fabric_installed = self.find_installed_fabric_version(&base_version).await;

            if let Some(fabric_id) = fabric_installed {
                version_to_launch = fabric_id;
            } else {
                on_progress(0.15, "Installing Fabric...".into());
                match self
                    .install_fabric(&base_version, Some(java_p.clone()))
                    .await
                {
                    Ok(new_id) => version_to_launch = new_id,
                    Err(e) => return Err(anyhow!("Failed to install Fabric: {}", e)),
                }
            }
        }

        // 3. Prepare Game Dir
        let game_dir = if let Some(dir) = game_dir_override {
            dir
        } else {
            self.config.minecraft_dir.clone()
        };

        if !game_dir.exists() {
            let _ = fs::create_dir_all(&game_dir).await;
        }

        on_progress(0.20, "Preparing Minecraft files...".into());

        // 4. Launch
        let cmd = self
            .launch_minecraft(
                &version_to_launch,
                &username,
                ram_mb,
                &game_dir,
                active_account.as_ref(),
                Some(on_progress.clone()),
            )
            .await;

        on_progress(1.0, "Game Started".into());
        cmd
    }

    pub async fn find_installed_fabric_version(&self, mc_version: &str) -> Option<String> {
        if let Ok(mut entries) = tokio::fs::read_dir(&self.config.versions_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Some(name) = entry.file_name().to_str() {
                    if name.contains("fabric-loader") && name.ends_with(&format!("-{}", mc_version))
                    {
                        return Some(name.to_string());
                    }
                }
            }
        }
        None
    }

    pub async fn install_fabric(
        &self,
        mc_version: &str,
        java_path_buf: Option<PathBuf>,
    ) -> Result<String> {
        // 1. Download Fabric Installer
        let installer_url = "https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.1.0/fabric-installer-1.1.0.jar";
        let cache_dir = self.config.minecraft_dir.join("cache");
        fs::create_dir_all(&cache_dir).await?;
        let installer_path = cache_dir.join("fabric-installer.jar");

        if !installer_path.exists() {
            let resp = reqwest::get(installer_url).await?;
            let bytes = resp.bytes().await?.to_vec();
            use tokio::io::AsyncWriteExt;
            let mut out = tokio::fs::File::create(&installer_path).await?;
            out.write_all(&bytes).await?;
        }

        let java_path = if let Some(p) = java_path_buf {
            p
        } else {
            self.java_manager.find_java(None)?
        };

        let mut command = TokioCommand::new(java_path);
        command
            .arg("-jar")
            .arg(&installer_path)
            .arg("client")
            .arg("-dir")
            .arg(&self.config.minecraft_dir)
            .arg("-mcversion")
            .arg(mc_version)
            .arg("-noprofile")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = command.output().await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Fabric installation failed: {}", err));
        }

        let versions_dir = self.config.versions_dir.clone();
        let mut best_match: Option<String> = None;
        let mut latest_time = std::time::SystemTime::UNIX_EPOCH;

        let mut read_dir = tokio::fs::read_dir(&versions_dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.contains("fabric-loader") && name.ends_with(&format!("-{}", mc_version))
                    {
                        if let Ok(metadata) = entry.metadata().await {
                            if let Ok(modified) = metadata.modified() {
                                if modified > latest_time {
                                    latest_time = modified;
                                    best_match = Some(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        best_match.ok_or_else(|| anyhow!("Could not find installed Fabric version directory"))
    }
}
