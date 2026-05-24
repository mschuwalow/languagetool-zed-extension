use std::fs;
use std::path::{Path, PathBuf};
use zed_extension_api::{self as zed, settings::LspSettings, GithubRelease, Result};

const LSP_NAME: &str = "languagetool-lsp";
const VERSION_FILE: &str = ".languagetool-lsp-version";
const GITHUB_REPO_OWNER: &str = "mschuwalow";
const GITHUB_REPO_NAME: &str = "languagetool-lsp";
const GET_PRE_RELEASE: bool = false;

struct LanguageToolExtension {
    binary_cache: Option<PathBuf>,
}

#[derive(Clone)]
struct LanguageToolBinary {
    path: PathBuf,
}

impl LanguageToolBinary {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl LanguageToolExtension {
    fn find_system_binary(&self, worktree: &zed::Worktree) -> Option<LanguageToolBinary> {
        worktree
            .which(LSP_NAME)
            .map(PathBuf::from)
            .map(LanguageToolBinary::new)
    }

    fn get_cached_binary(&self) -> Option<LanguageToolBinary> {
        self.binary_cache
            .as_ref()
            .filter(|path| path.exists())
            .cloned()
            .map(LanguageToolBinary::new)
    }

    fn get_binary(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<LanguageToolBinary> {
        if let Some(binary) = self.find_system_binary(worktree) {
            return Ok(binary);
        }

        if let Some(binary) = self.get_cached_binary() {
            return Ok(binary);
        }

        self.ensure_latest_binary(language_server_id)
    }

    fn ensure_latest_binary(
        &mut self,
        language_server_id: &zed::LanguageServerId,
    ) -> Result<LanguageToolBinary> {
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let result = match self.check_for_update() {
            Ok(Some(release)) => {
                zed::set_language_server_installation_status(
                    language_server_id,
                    &zed::LanguageServerInstallationStatus::Downloading,
                );
                self.install_release(&release)
            }
            Ok(None) => self.load_existing_binary(),
            Err(update_err) => self.load_existing_binary().map_err(|_| {
                format!(
                    "Could not check GitHub for {LSP_NAME} and no cached binary is available. \
                     Check that github.com is reachable. Underlying error: {update_err}"
                )
            }),
        };

        match result {
            Ok(binary) => {
                self.binary_cache = Some(binary.path.clone());
                zed::set_language_server_installation_status(
                    language_server_id,
                    &zed::LanguageServerInstallationStatus::None,
                );
                Ok(binary)
            }
            Err(err) => {
                zed::set_language_server_installation_status(
                    language_server_id,
                    &zed::LanguageServerInstallationStatus::Failed(err.clone()),
                );
                Err(err)
            }
        }
    }

    fn check_for_update(&self) -> Result<Option<GithubRelease>> {
        let repo = format!("{GITHUB_REPO_OWNER}/{GITHUB_REPO_NAME}");
        let release = zed::latest_github_release(
            &repo,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: GET_PRE_RELEASE,
            },
        )?;

        if self.read_version_file().as_deref() == Ok(release.version.as_str()) {
            return Ok(None);
        }

        Ok(Some(release))
    }

    fn install_release(&self, release: &GithubRelease) -> Result<LanguageToolBinary> {
        let asset = self.find_compatible_asset(release)?;
        let version_dir = self.version_dir(&release.version);
        let binary_path = version_dir.join(self.binary_filename());

        if !binary_path.exists() {
            self.download_asset(asset, &version_dir, &binary_path)?;
            self.write_version_file(&release.version)?;
            self.cleanup_old_versions(&version_dir)?;
        }

        Ok(LanguageToolBinary::new(binary_path))
    }

    fn load_existing_binary(&self) -> Result<LanguageToolBinary> {
        let version = self.read_version_file()?;
        let binary_path = self.version_dir(&version).join(self.binary_filename());
        if !binary_path.exists() {
            return Err(format!(
                "Cached binary not found at {}",
                binary_path.display()
            ));
        }
        Ok(LanguageToolBinary::new(binary_path))
    }

    fn read_version_file(&self) -> Result<String> {
        fs::read_to_string(VERSION_FILE)
            .map(|s| s.trim().to_string())
            .map_err(|err| match err.kind() {
                std::io::ErrorKind::NotFound => "no cached binary yet".to_string(),
                _ => format!("failed to read {VERSION_FILE}: {err}"),
            })
    }

    fn write_version_file(&self, version: &str) -> Result<()> {
        fs::write(VERSION_FILE, version)
            .map_err(|err| format!("failed to write {VERSION_FILE}: {err}"))
    }

    fn version_dir(&self, version: &str) -> PathBuf {
        PathBuf::from(format!("{LSP_NAME}-{version}"))
    }

    fn binary_filename(&self) -> String {
        let (os, _) = zed::current_platform();
        if os == zed::Os::Windows {
            format!("{LSP_NAME}.exe")
        } else {
            LSP_NAME.to_string()
        }
    }

    fn asset_name(&self, os: zed::Os, arch: zed::Architecture) -> Result<(String, String)> {
        let arch = match arch {
            zed::Architecture::Aarch64 => "aarch64",
            zed::Architecture::X8664 => "x86_64",
            zed::Architecture::X86 => return Err("x86 architecture is not supported".into()),
        };

        let (os, extension) = match os {
            zed::Os::Linux => ("unknown-linux-musl", "tar.gz"),
            zed::Os::Mac => ("apple-darwin", "tar.gz"),
            zed::Os::Windows => ("pc-windows-msvc", "zip"),
        };

        let target = format!("{arch}-{os}");
        Ok((format!("{LSP_NAME}-{target}.{extension}"), target))
    }

    fn find_compatible_asset<'a>(
        &self,
        release: &'a GithubRelease,
    ) -> Result<&'a zed::GithubReleaseAsset> {
        let (os, arch) = zed::current_platform();
        let (asset_name, target) = self.asset_name(os, arch)?;
        release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("No {LSP_NAME} release asset found for {target}"))
    }

    fn download_asset(
        &self,
        asset: &zed::GithubReleaseAsset,
        version_dir: &Path,
        binary_path: &Path,
    ) -> Result<()> {
        let (os, _) = zed::current_platform();
        let version_dir = version_dir
            .to_str()
            .ok_or_else(|| "invalid version directory".to_string())?;

        zed::download_file(
            &asset.download_url,
            version_dir,
            if os == zed::Os::Windows {
                zed::DownloadedFileType::Zip
            } else {
                zed::DownloadedFileType::GzipTar
            },
        )
        .map_err(|err| format!("failed to download {LSP_NAME}: {err}"))?;

        let binary_path = binary_path
            .to_str()
            .ok_or_else(|| "invalid binary path".to_string())?;
        zed::make_file_executable(binary_path)
            .map_err(|err| format!("failed to make {LSP_NAME} executable: {err}"))
    }

    fn cleanup_old_versions(&self, current_version_dir: &Path) -> Result<()> {
        let current = current_version_dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "invalid version directory name".to_string())?;

        for entry in fs::read_dir(".").map_err(|err| format!("failed to read directory: {err}"))? {
            let entry = entry.map_err(|err| format!("failed to read directory entry: {err}"))?;
            let Some(name) = entry.file_name().to_str().map(ToString::to_string) else {
                continue;
            };

            if name.starts_with(&format!("{LSP_NAME}-")) && name != current && entry.path().is_dir()
            {
                let _ = fs::remove_dir_all(entry.path());
            }
        }

        Ok(())
    }
}

impl zed::Extension for LanguageToolExtension {
    fn new() -> Self {
        Self { binary_cache: None }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary = self.get_binary(language_server_id, worktree)?;
        let command = binary
            .path
            .to_str()
            .ok_or_else(|| "failed to convert binary path to string".to_string())?
            .to_string();

        Ok(zed::Command {
            command,
            args: vec![
                format!("--root={}", worktree.root_path()),
                "serve".to_string(),
            ],
            env: Vec::new(),
        })
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        let options = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.initialization_options);
        Ok(options)
    }
}

zed::register_extension!(LanguageToolExtension);
