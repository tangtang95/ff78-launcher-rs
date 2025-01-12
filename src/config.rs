use anyhow::Result;
use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsA, DEVMODEA, ENUM_CURRENT_SETTINGS};

use crate::GameType;

#[derive(Debug)]
pub struct Config {
    pub fullscreen: bool,
    pub window_width: u32,
    pub window_height: u32,
    pub refresh_rate: u32,
    pub enable_linear_filtering: bool,
    pub keep_aspect_ratio: bool,
    pub original_mode: bool,
    pub pause_game_on_background: bool,
    pub sfx_volume: i32,
    pub music_volume: i32,
    pub launch_chocobo: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fullscreen: Default::default(),
            window_width: Default::default(),
            window_height: Default::default(),
            refresh_rate: Default::default(),
            enable_linear_filtering: Default::default(),
            keep_aspect_ratio: Default::default(),
            original_mode: Default::default(),
            pause_game_on_background: Default::default(),
            sfx_volume: 100,
            music_volume: 100,
            launch_chocobo: Default::default(),
        }
    }
}

impl Config {
    pub fn from_config_file(path: &str, game_type: &GameType) -> Result<Self> {
        let file_contents = std::fs::read(path);
        let file_contents = file_contents.unwrap_or_default();
        let table: toml::Table = toml::from_str(std::str::from_utf8(&file_contents)?)?;

        let fullscreen = table
            .get("fullscreen")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let mut window_width = table
            .get("window_width")
            .and_then(|value| value.as_integer())
            .unwrap_or(0)
            .max(0) as u32;
        let mut window_height = table
            .get("window_height")
            .and_then(|value| value.as_integer())
            .unwrap_or(0)
            .max(0) as u32;
        let mut refresh_rate = table
            .get("refresh_rate")
            .and_then(|value| value.as_integer())
            .unwrap_or(0)
            .max(0) as u32;

        if window_width == 0 && window_height == 0 {
            let mut display_settings = DEVMODEA::default();
            let display_settings_found = unsafe {
                EnumDisplaySettingsA(None, ENUM_CURRENT_SETTINGS, &mut display_settings).as_bool()
            };
            log::info!("Display settings found: {}x{} (refresh rate: {})", display_settings.dmPelsWidth, display_settings.dmPelsHeight, display_settings.dmDisplayFrequency);
            if fullscreen && display_settings_found {
                window_width = display_settings.dmPelsWidth;
                window_height = display_settings.dmPelsHeight;
                if refresh_rate == 0 {
                    refresh_rate = display_settings.dmDisplayFrequency;
                }
            } else {
                window_width = 640;
                window_height = 480;
                if refresh_rate == 0 {
                    refresh_rate = 60;
                }
            }
        }

        let mut pause_game_on_background = table
            .get("pause_game_on_background")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let mut launch_chocobo = table
            .get("launch_chocobo")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if let GameType::FF7(_) = game_type {
            pause_game_on_background = false;
            launch_chocobo = false;
        }

        Ok(Config {
            fullscreen,
            window_width,
            window_height,
            refresh_rate,
            enable_linear_filtering: table
                .get("enable_linear_filtering")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            keep_aspect_ratio: table
                .get("keep_aspect_ratio")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            original_mode: table
                .get("original_mode")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            pause_game_on_background,
            sfx_volume: table
                .get("sfx_volume")
                .and_then(|value| value.as_integer())
                .unwrap_or(0)
                .max(0) as i32,
            music_volume: table
                .get("music_volume")
                .and_then(|value| value.as_integer())
                .unwrap_or(0)
                .max(0) as i32,
            launch_chocobo,
        })
    }
}
