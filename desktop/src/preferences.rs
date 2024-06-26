mod read;
mod write;

pub mod storage;

use crate::cli::Opt;
use crate::log::FilenamePattern;
use crate::preferences::read::{read_bookmarks, read_preferences};
use crate::preferences::write::{BookmarksWriter, PreferencesWriter};
use anyhow::{Context, Error};
use ruffle_core::backend::ui::US_ENGLISH;
use ruffle_frontend_utils::bookmarks::Bookmarks;
use ruffle_frontend_utils::parse::DocumentHolder;
use ruffle_render_wgpu::clap::{GraphicsBackend, PowerPreference};
use std::sync::{Arc, Mutex};
use sys_locale::get_locale;
use unic_langid::LanguageIdentifier;

/// The preferences that relate to the application itself.
///
/// This structure is safe to clone, internally it holds an Arc to any mutable properties.
///
/// The general priority order for preferences should look as follows, where top is "highest priority":
/// - User-selected movie-specific setting (if applicable, such as through Open Advanced)
/// - Movie-specific settings (if applicable and we implement this, stored on disk)
/// - CLI (if applicable)
/// - Persisted preferences (if applicable, saved to toml)
/// - Ruffle defaults
#[derive(Clone)]
pub struct GlobalPreferences {
    /// As the CLI holds properties ranging from initial movie settings (ie url),
    /// to application itself (ie render backend),
    /// this field is available for checking where needed.
    // TODO: This should really not be public and we should split up CLI somehow,
    // or make it all getters in here?
    pub cli: Opt,

    /// The actual, mutable user preferences that are persisted to disk.
    preferences: Arc<Mutex<DocumentHolder<SavedGlobalPreferences>>>,

    bookmarks: Arc<Mutex<DocumentHolder<Bookmarks>>>,
}

impl GlobalPreferences {
    pub fn load(cli: Opt) -> Result<Self, Error> {
        std::fs::create_dir_all(&cli.config).context("Failed to create configuration directory")?;
        let preferences_path = cli.config.join("preferences.toml");
        let preferences = if preferences_path.exists() {
            let contents = std::fs::read_to_string(&preferences_path)
                .context("Failed to read saved preferences")?;
            let result = read_preferences(&contents);
            for warning in result.warnings {
                // TODO: A way to display warnings to users, generally
                tracing::warn!("{warning}");
            }
            result.result
        } else {
            Default::default()
        };

        let bookmarks_path = cli.config.join("bookmarks.toml");
        let bookmarks = if bookmarks_path.exists() {
            let contents = std::fs::read_to_string(&bookmarks_path)
                .context("Failed to read saved bookmarks")?;
            let result = read_bookmarks(&contents);
            for warning in result.warnings {
                tracing::warn!("{warning}");
            }
            result.result
        } else {
            Default::default()
        };

        Ok(Self {
            cli,
            preferences: Arc::new(Mutex::new(preferences)),
            bookmarks: Arc::new(Mutex::new(bookmarks)),
        })
    }

    pub fn graphics_backends(&self) -> GraphicsBackend {
        self.cli.graphics.unwrap_or_else(|| {
            self.preferences
                .lock()
                .expect("Preferences is not reentrant")
                .graphics_backend
        })
    }

    pub fn graphics_power_preference(&self) -> PowerPreference {
        self.cli.power.unwrap_or_else(|| {
            self.preferences
                .lock()
                .expect("Preferences is not reentrant")
                .graphics_power_preference
        })
    }

    pub fn language(&self) -> LanguageIdentifier {
        self.preferences
            .lock()
            .expect("Preferences is not reentrant")
            .language
            .clone()
    }

    pub fn output_device_name(&self) -> Option<String> {
        self.preferences
            .lock()
            .expect("Preferences is not reentrant")
            .output_device
            .clone()
    }

    pub fn mute(&self) -> bool {
        self.preferences
            .lock()
            .expect("Preferences is not reentrant")
            .mute
    }

    pub fn preferred_volume(&self) -> f32 {
        self.cli.volume.unwrap_or_else(|| {
            self.preferences
                .lock()
                .expect("Preferences is not reentrant")
                .volume
        })
    }

    pub fn log_filename_pattern(&self) -> FilenamePattern {
        self.preferences
            .lock()
            .expect("Preferences is not reentrant")
            .log
            .filename_pattern
    }

    pub fn bookmarks(&self, fun: impl FnOnce(&Bookmarks)) {
        fun(&self.bookmarks.lock().expect("Bookmarks is not reentrant"))
    }

    pub fn have_bookmarks(&self) -> bool {
        let bookmarks = &self.bookmarks.lock().expect("Bookmarks is not reentrant");

        !bookmarks.is_empty() && !bookmarks.iter().all(|x| x.is_invalid())
    }

    pub fn storage_backend(&self) -> storage::StorageBackend {
        self.cli.storage.unwrap_or_else(|| {
            self.preferences
                .lock()
                .expect("Preferences is not reentrant")
                .storage
                .backend
        })
    }

    pub fn write_preferences(&self, fun: impl FnOnce(&mut PreferencesWriter)) -> Result<(), Error> {
        let mut preferences = self
            .preferences
            .lock()
            .expect("Preferences is not reentrant");

        let mut writer = PreferencesWriter::new(&mut preferences);
        fun(&mut writer);

        let serialized = preferences.serialize();
        std::fs::write(self.cli.config.join("preferences.toml"), serialized)
            .context("Could not write preferences to disk")
    }

    pub fn write_bookmarks(&self, fun: impl FnOnce(&mut BookmarksWriter)) -> Result<(), Error> {
        let mut bookmarks = self.bookmarks.lock().expect("Bookmarks is not reentrant");

        let mut writer = BookmarksWriter::new(&mut bookmarks);
        fun(&mut writer);

        let serialized = bookmarks.serialize();
        std::fs::write(self.cli.config.join("bookmarks.toml"), serialized)
            .context("Could not write bookmarks to disk")
    }
}

#[derive(PartialEq, Debug)]
pub struct SavedGlobalPreferences {
    pub graphics_backend: GraphicsBackend,
    pub graphics_power_preference: PowerPreference,
    pub language: LanguageIdentifier,
    pub output_device: Option<String>,
    pub mute: bool,
    pub volume: f32,
    pub log: LogPreferences,
    pub storage: StoragePreferences,
}

impl Default for SavedGlobalPreferences {
    fn default() -> Self {
        let preferred_locale = get_locale();
        let locale = preferred_locale
            .and_then(|l| l.parse().ok())
            .unwrap_or_else(|| US_ENGLISH.clone());
        Self {
            graphics_backend: Default::default(),
            graphics_power_preference: Default::default(),
            language: locale,
            output_device: None,
            mute: false,
            volume: 1.0,
            log: Default::default(),
            storage: Default::default(),
        }
    }
}

#[derive(PartialEq, Debug, Default)]
pub struct LogPreferences {
    pub filename_pattern: FilenamePattern,
}

#[derive(PartialEq, Debug, Default)]
pub struct StoragePreferences {
    pub backend: storage::StorageBackend,
}
