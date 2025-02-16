use std::{io::Write, os::windows::ffi::OsStrExt};

use anyhow::Result;
use windows::Win32::{
    System::{
        Com::CoTaskMemFree,
        Threading::{ReleaseSemaphore, WaitForSingleObject, INFINITE},
    },
    UI::Shell::{FOLDERID_Documents, SHGetKnownFolderPath, KF_FLAG_DEFAULT},
};

use crate::{Context, GameType, LauncherContext, StoreType, APP_NAME};

const FF7_USER_SAVE_DIR: u32 = 10;
const FF7_DOC_DIR: u32 = 11;
const FF7_INSTALL_DIR: u32 = 12;
const FF7_LOCALE_DATA_DIR: u32 = 13;
const FF7_GAME_VERSION: u32 = 18;
const FF7_DISABLE_CLOUD: u32 = 22;
const FF7_END_USER_INFO: u32 = 24;

const FF8_USER_SAVE_DIR: u32 = 9;
const FF8_DOC_DIR: u32 = 10;
const FF8_INSTALL_DIR: u32 = 11;
const FF8_LOCALE_DATA_DIR: u32 = 12;
const FF8_GAME_VERSION: u32 = 17;
const FF8_DISABLE_CLOUD: u32 = 21;
const FF8_BG_PAUSE_ENABLED: u32 = 23;
const FF8_END_USER_INFO: u32 = 24;

const ESTORE_USER_SAVE_DIR: u32 = 9;
const ESTORE_DOC_DIR: u32 = 10;
const ESTORE_INSTALL_DIR: u32 = 11;
const ESTORE_LOCALE_DATA_DIR: u32 = 12;
const ESTORE_GAME_VERSION: u32 = 17;
const ESTORE_END_USER_INFO: u32 = 20;

pub fn send_locale_data_dir(ctx: &Context, launcher_ctx: &mut LauncherContext) {
    let payload: Vec<u16> = (String::from("lang-") + &ctx.game_lang)
        .encode_utf16()
        .collect();
    let mut bytes = Vec::<u8>::new();
    bytes.extend_from_slice(
        &match ctx.game_to_launch {
            GameType::FF7(StoreType::Standard) => FF7_LOCALE_DATA_DIR,
            GameType::FF7(StoreType::EStore) => ESTORE_LOCALE_DATA_DIR,
            GameType::FF8 => FF8_LOCALE_DATA_DIR,
        }
        .to_le_bytes(),
    );
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.append(&mut payload.iter().flat_map(|b| b.to_le_bytes()).collect());
    unsafe {
        std::ptr::copy(
            bytes.as_ptr(),
            launcher_ctx.launcher_memory_part as _,
            bytes.len(),
        );
    };
    log::info!(
        "send_locale_data_dir -> {}, {}, {}",
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        String::from_utf16_lossy(&payload)
    );

    wait_for_game(launcher_ctx);
}

pub fn send_user_save_dir(ctx: &Context, launcher_ctx: &mut LauncherContext) -> Result<()> {
    let mut payload = get_game_metadata_path(ctx)?;
    if std::fs::exists("save").is_ok_and(|v| v) {
        payload += "\\save";
    } else {
        let paths = std::fs::read_dir(&payload)?;
        let user_path = paths
            .filter_map(|p| p.ok().map(|p| p.path()))
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .expect("Always have filename")
                        .to_string_lossy()
                        .starts_with("user_")
            })
            .last();
        if let Some(user_path) = user_path {
            payload += "\\";
            payload += user_path
                .file_name()
                .expect("Always have filename")
                .to_string_lossy()
                .as_ref()
        }
    }
    let payload: Vec<u16> = payload.encode_utf16().collect();

    let mut bytes = Vec::<u8>::new();
    bytes.extend_from_slice(
        &match ctx.game_to_launch {
            GameType::FF7(StoreType::Standard) => FF7_USER_SAVE_DIR,
            GameType::FF7(StoreType::EStore) => ESTORE_USER_SAVE_DIR,
            GameType::FF8 => FF8_USER_SAVE_DIR,
        }
        .to_le_bytes(),
    );
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.append(&mut payload.iter().flat_map(|b| b.to_le_bytes()).collect());
    unsafe {
        std::ptr::copy(
            bytes.as_ptr(),
            launcher_ctx.launcher_memory_part as _,
            bytes.len(),
        );
    };
    log::info!(
        "send_user_save_dir -> {}, {}, {}",
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        String::from_utf16_lossy(&payload)
    );

    wait_for_game(launcher_ctx);
    Ok(())
}

pub fn send_user_doc_dir(ctx: &Context, launcher_ctx: &mut LauncherContext) -> Result<()> {
    let payload: Vec<u16> = get_game_metadata_path(ctx)?.encode_utf16().collect();
    let mut bytes = Vec::<u8>::new();
    bytes.extend_from_slice(
        &match ctx.game_to_launch {
            GameType::FF7(StoreType::Standard) => FF7_DOC_DIR,
            GameType::FF7(StoreType::EStore) => ESTORE_DOC_DIR,
            GameType::FF8 => FF8_DOC_DIR,
        }
        .to_le_bytes(),
    );
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.append(&mut payload.iter().flat_map(|b| b.to_le_bytes()).collect());
    bytes.push(0);
    unsafe {
        std::ptr::copy(
            bytes.as_ptr(),
            launcher_ctx.launcher_memory_part as _,
            bytes.len(),
        );
    };
    log::info!(
        "send_user_doc_dir -> {}, {}, {}",
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        String::from_utf16_lossy(&payload)
    );

    wait_for_game(launcher_ctx);
    Ok(())
}

pub fn send_install_dir(ctx: &Context, launcher_ctx: &mut LauncherContext) -> Result<()> {
    let cwd = std::path::absolute(".")?;
    let payload: Vec<u16> = cwd.into_os_string().encode_wide().collect();
    let mut bytes = Vec::<u8>::new();
    bytes.extend_from_slice(
        &match ctx.game_to_launch {
            GameType::FF7(StoreType::Standard) => FF7_INSTALL_DIR,
            GameType::FF7(StoreType::EStore) => ESTORE_INSTALL_DIR,
            GameType::FF8 => FF8_INSTALL_DIR,
        }
        .to_le_bytes(),
    );
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.append(&mut payload.iter().flat_map(|b| b.to_le_bytes()).collect());
    unsafe {
        std::ptr::copy(
            bytes.as_ptr(),
            launcher_ctx.launcher_memory_part as _,
            bytes.len(),
        );
    };
    log::info!(
        "send_install_dir -> {:?}, {:?}, {}",
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        String::from_utf16_lossy(&payload)
    );

    wait_for_game(launcher_ctx);
    Ok(())
}

pub fn send_game_version(ctx: &Context, launcher_ctx: &mut LauncherContext) {
    let payload: Vec<u16> = (APP_NAME.to_string() + " 1.0.0").encode_utf16().collect();
    let mut bytes = Vec::<u8>::new();
    bytes.extend_from_slice(
        &match ctx.game_to_launch {
            GameType::FF7(StoreType::Standard) => FF7_GAME_VERSION,
            GameType::FF7(StoreType::EStore) => ESTORE_GAME_VERSION,
            GameType::FF8 => FF8_GAME_VERSION,
        }
        .to_le_bytes(),
    );
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.append(&mut payload.iter().flat_map(|b| b.to_le_bytes()).collect());
    unsafe {
        std::ptr::copy(
            bytes.as_ptr(),
            launcher_ctx.launcher_memory_part as _,
            bytes.len(),
        );
    };
    log::info!(
        "send_game_version -> {:?}, {:?}, {}",
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        String::from_utf16_lossy(&payload)
    );

    wait_for_game(launcher_ctx);
}

pub fn send_disable_cloud(ctx: &Context, launcher_ctx: &mut LauncherContext) {
    if let GameType::FF7(StoreType::EStore) = ctx.game_to_launch {
        return;
    }

    let mut launcher_game_part = Vec::<u8>::new();
    launcher_game_part.extend_from_slice(
        &match ctx.game_to_launch {
            GameType::FF7(_) => FF7_DISABLE_CLOUD,
            GameType::FF8 => FF8_DISABLE_CLOUD,
        }
        .to_le_bytes(),
    );
    unsafe {
        std::ptr::copy(
            launcher_game_part.as_ptr(),
            launcher_ctx.launcher_memory_part as _,
            launcher_game_part.len(),
        );
    };
    log::info!("send_disable_cloud -> {launcher_game_part:?}");

    wait_for_game(launcher_ctx);
}

pub fn send_bg_pause_enabled(ctx: &Context, launcher_ctx: &mut LauncherContext) {
    if let GameType::FF7(_) = ctx.game_to_launch {
        return;
    }

    let mut launcher_game_part = Vec::<u8>::new();
    launcher_game_part.extend_from_slice(
        &match ctx.game_to_launch {
            GameType::FF7(_) => unreachable!(),
            GameType::FF8 => FF8_BG_PAUSE_ENABLED,
        }
        .to_le_bytes(),
    );
    launcher_game_part.extend_from_slice(&1u32.to_le_bytes());
    unsafe {
        std::ptr::copy(
            launcher_game_part.as_ptr(),
            launcher_ctx.launcher_memory_part as _,
            launcher_game_part.len(),
        );
    };
    log::info!("send_bg_pause_enabled -> {launcher_game_part:?}");

    wait_for_game(launcher_ctx);
}

pub fn send_launcher_completed(ctx: &Context, launcher_ctx: &mut LauncherContext) {
    let mut launcher_game_part = Vec::<u8>::new();
    launcher_game_part.extend_from_slice(
        &match ctx.game_to_launch {
            GameType::FF7(StoreType::Standard) => FF7_END_USER_INFO,
            GameType::FF7(StoreType::EStore) => ESTORE_END_USER_INFO,
            GameType::FF8 => FF8_END_USER_INFO,
        }
        .to_le_bytes(),
    );
    unsafe {
        std::ptr::copy(
            launcher_game_part.as_ptr(),
            launcher_ctx.launcher_memory_part as _,
            launcher_game_part.len(),
        );
    };
    log::info!("send_launcher_completed -> {launcher_game_part:?}");

    wait_for_game(launcher_ctx);
}

pub fn write_ffvideo(ctx: &Context) -> Result<()> {
    let filename = match ctx.game_to_launch {
        GameType::FF7(_) => "ff7video.cfg",
        GameType::FF8 => "ff8video.cfg",
    };
    let filepath = get_game_metadata_path(ctx)? + "\\" + filename;
    let mut file = std::fs::File::create(filepath)?;
    match ctx.game_to_launch {
        GameType::FF7(_) => {
            file.write_all(&ctx.config.window_width.to_be_bytes())?;
            file.write_all(&ctx.config.window_height.to_be_bytes())?;
            file.write_all(&ctx.config.refresh_rate.to_be_bytes())?;
            file.write_all(&u32::from(ctx.config.fullscreen).to_be_bytes())?;
            file.write_all(&0u32.to_be_bytes())?;
            file.write_all(&u32::from(ctx.config.keep_aspect_ratio).to_be_bytes())?;
            file.write_all(&u32::from(ctx.config.enable_linear_filtering).to_be_bytes())?;
            file.write_all(&u32::from(ctx.config.original_mode).to_be_bytes())?;
        }
        GameType::FF8 => {
            file.write_all(&ctx.config.window_width.to_le_bytes())?;
            file.write_all(&ctx.config.window_height.to_le_bytes())?;
            file.write_all(&ctx.config.refresh_rate.to_le_bytes())?;
            file.write_all(&u32::from(ctx.config.fullscreen).to_le_bytes())?;
            file.write_all(&0u32.to_le_bytes())?;
            file.write_all(&u32::from(ctx.config.keep_aspect_ratio).to_le_bytes())?;
            file.write_all(&u32::from(ctx.config.enable_linear_filtering).to_le_bytes())?;
            file.write_all(&u32::from(ctx.config.original_mode).to_le_bytes())?;
            file.write_all(&u32::from(ctx.config.pause_game_on_background).to_le_bytes())?;
        }
    }
    Ok(())
}

pub fn write_ffsound(ctx: &Context) -> Result<()> {
    let filename = match ctx.game_to_launch {
        GameType::FF7(_) => "ff7sound.cfg",
        GameType::FF8 => "ff8sound.cfg",
    };
    let filepath = get_game_metadata_path(ctx)? + "\\" + filename;
    let mut file = std::fs::File::create(filepath)?;
    file.write_all(&ctx.config.sfx_volume.to_le_bytes())?;
    file.write_all(&ctx.config.music_volume.to_le_bytes())?;
    Ok(())
}

fn get_game_metadata_path(ctx: &Context) -> Result<String> {
    let mut game_install_path = String::new();
    if !matches!(ctx.game_to_launch, GameType::FF7(StoreType::EStore))
        && !std::fs::exists("data/music_2").is_ok_and(|b| b)
    {
        let doc_path = unsafe {
            let doc_path_pw = SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None)?;
            let doc_path = doc_path_pw.to_string()?;
            CoTaskMemFree(Some(doc_path_pw.as_ptr() as _));
            doc_path
        };
        game_install_path += &doc_path;
        game_install_path += "\\Square Enix\\FINAL FANTASY ";
        game_install_path += match ctx.game_to_launch {
            GameType::FF7(_) => "VII Steam",
            GameType::FF8 => "VIII Steam",
        }
    } else {
        let cwd = std::env::current_dir()?
            .to_str()
            .ok_or(anyhow::anyhow!("cwd cannot be converted to string"))?
            .to_string();
        game_install_path += &cwd;
    }
    Ok(game_install_path)
}

fn wait_for_game(launcher_ctx: &mut LauncherContext) {
    unsafe {
        // Wait for the game
        _ = ReleaseSemaphore(launcher_ctx.game_can_read_sem, 1, None);
        WaitForSingleObject(launcher_ctx.game_did_read_sem, INFINITE);
    }
}
