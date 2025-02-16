#![windows_subsystem = "windows"]

mod config;
mod launcher;

use anyhow::Result;
use config::Config;
use launcher::{
    send_bg_pause_enabled, send_disable_cloud, send_game_version, send_install_dir,
    send_launcher_completed, send_locale_data_dir, send_user_doc_dir, send_user_save_dir,
    write_ffsound, write_ffvideo,
};
use log::LevelFilter;
use std::{
    ffi::{c_void, CString},
    os::windows::fs::MetadataExt,
    process::Command,
    sync::mpsc::{Receiver, TryRecvError},
};
use windows::{
    core::{s, PCSTR},
    Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::Debug::{
                SetUnhandledExceptionFilter, EXCEPTION_CONTINUE_EXECUTION, EXCEPTION_POINTERS,
            },
            Memory::{
                CreateFileMappingA, MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
                PAGE_READWRITE,
            },
            Threading::{CreateSemaphoreA, ReleaseSemaphore, WaitForSingleObject, INFINITE},
        },
        UI::WindowsAndMessaging::{MessageBoxA, MB_ICONERROR, MB_OK},
    },
};

const APP_NAME: &str = "FF78Launcher";
const LOG_FILE: &str = "FF78Launcher.log";
const PROCESSES: [&str; 11] = [
    // FF7
    "ff7_de.exe",
    "ff7_en.exe",
    "ff7_es.exe",
    "ff7_fr.exe",
    "ff7_ja.exe",
    // FF8
    "ff8_de.exe",
    "ff8_en.exe",
    "ff8_es.exe",
    "ff8_fr.exe",
    "ff8_it.exe",
    "ff8_ja.exe",
];
const AF3DN_FILE: &str = "AF3DN.P";
const GAME_CAN_READ_MSG_SEM: &str = "_gameCanReadMsgSem";
const GAME_DID_READ_MSG_SEM: &str = "_gameDidReadMsgSem";
const LAUNCHER_CAN_READ_MSG_SEM: &str = "_launcherCanReadMsgSem";
const LAUNCHER_DID_READ_MSG_SEM: &str = "_launcherDidReadMsgSem";
const SHARED_MEMORY_WITH_LAUNCHER_NAME: &str = "_sharedMemoryWithLauncher";

static mut HAD_EXCEPTION: bool = false;

#[derive(Debug)]
enum StoreType {
    Standard,
    EStore,
}

#[derive(Debug)]
enum GameType {
    FF7(StoreType),
    FF8,
}

#[derive(Debug)]
pub struct Context {
    game_to_launch: GameType,
    game_lang: String,
    use_ffnx: bool,
    config: Config,
}

#[derive(Debug)]
pub struct LauncherContext {
    game_can_read_sem: HANDLE,
    game_did_read_sem: HANDLE,
    launcher_memory_part: *mut c_void,
}

fn main() -> Result<()> {
    simple_logging::log_to_file(LOG_FILE, LevelFilter::Info)?;
    log::info!("{APP_NAME} launched!");

    unsafe {
        SetUnhandledExceptionFilter(Some(exception_handler));
    };

    match launch_process() {
        Ok(_) => Ok(()),
        Err(err) => {
            log::error!("Launching process failed due: {:?}", err);
            unsafe {
                _ = MessageBoxA(
                    None,
                    s!("Something went wrong while launching the game. Check the log file for more info"),
                    s!("Error"),
                    MB_ICONERROR | MB_OK,
                );
            }
            Err(err)
        }
    }
}

fn launch_process() -> Result<()> {
    let processes_available: Vec<&str> = PROCESSES
        .into_iter()
        .filter(|process| matches!(std::fs::exists(process), Ok(true)))
        .collect();
    if processes_available.len() > 1 {
        return Err(anyhow::anyhow!(
            "More than one process to start found: {:?}",
            processes_available
        ));
    }
    let Some(mut process_to_start) = processes_available.first().map(|s| s.to_string()) else {
        return Err(anyhow::anyhow!("No process to start found!"));
    };

    let game_to_launch = match &process_to_start {
        name if name.starts_with("ff8") => GameType::FF8,
        name if name.starts_with("ff7_ja")
            && std::fs::metadata(AF3DN_FILE)
                .is_ok_and(|metadata| metadata.file_size() < 1024 * 1024) =>
        {
            GameType::FF7(StoreType::EStore)
        }
        _ => GameType::FF7(StoreType::Standard),
    };

    let use_ffnx =
        std::fs::metadata(AF3DN_FILE).is_ok_and(|metadata| metadata.file_size() > 1024 * 1024);
    let game_lang = process_to_start
        .split('_')
        .take(2)
        .last()
        .map(|end| end.trim_end_matches(".exe").to_string());
    let Some(game_lang) = game_lang else {
        return Err(anyhow::anyhow!(
            "No language found for process: {}",
            process_to_start
        ));
    };

    let config = Config::from_config_file(&(APP_NAME.to_string() + ".toml"), &game_to_launch)?;
    log::info!("config: {:?}", config);

    if config.launch_chocobo {
        process_to_start = format!("chocobo_{}.exe", &game_lang);
    }

    let ctx = Context {
        game_to_launch,
        game_lang: game_lang.to_string(),
        use_ffnx,
        config,
    };

    let process_filename = std::fs::canonicalize(&process_to_start)?
        .file_name()
        .ok_or(anyhow::anyhow!("Filename of process not found"))?
        .to_os_string();
    if !ctx.use_ffnx || ctx.config.launch_chocobo {
        log::info!(
            "Launching process {:?} without FFNx context: {:?}",
            process_filename,
            &ctx
        );
        if !use_ffnx {
            write_ffvideo(&ctx)?;
            write_ffsound(&ctx)?;
        }
        let name_prefix = match ctx.config.launch_chocobo {
            true => "choco",
            false => match ctx.game_to_launch {
                GameType::FF7(_) => "ff7",
                GameType::FF8 => "ff8",
            },
        };
        let game_can_read_name = CString::new(name_prefix.to_owned() + GAME_CAN_READ_MSG_SEM)?;
        let game_did_read_name = CString::new(name_prefix.to_owned() + GAME_DID_READ_MSG_SEM)?;
        let shared_memory_name =
            CString::new(name_prefix.to_owned() + SHARED_MEMORY_WITH_LAUNCHER_NAME)?;
        let game_can_read_sem =
            unsafe { CreateSemaphoreA(None, 0, 1, PCSTR(game_can_read_name.as_ptr() as _))? };
        let game_did_read_sem =
            unsafe { CreateSemaphoreA(None, 0, 1, PCSTR(game_did_read_name.as_ptr() as _))? };
        let shared_memory = unsafe {
            CreateFileMappingA(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                0,
                0x20000,
                PCSTR(shared_memory_name.as_ptr() as _),
            )?
        };
        let view_shared_memory =
            unsafe { MapViewOfFile(shared_memory, FILE_MAP_ALL_ACCESS, 0, 0, 0) };
        let launcher_memory_part = unsafe { view_shared_memory.Value.offset(0x10000) };
        let mut launcher_context = LauncherContext {
            game_can_read_sem,
            game_did_read_sem,
            launcher_memory_part,
        };

        let (thread_kill_tx, thread_kill_rx) = std::sync::mpsc::channel::<()>();
        let process_game_messages_thread = std::thread::spawn(move || {
            handle_game_messages_thread(name_prefix, thread_kill_rx).unwrap();
        });

        let mut output = Command::new(process_filename).spawn()?;
        log::info!("Process launched (process_id: {})!", output.id());

        send_locale_data_dir(&ctx, &mut launcher_context);
        send_user_save_dir(&ctx, &mut launcher_context)?;
        send_user_doc_dir(&ctx, &mut launcher_context)?;
        send_install_dir(&ctx, &mut launcher_context)?;
        send_game_version(&ctx, &mut launcher_context);
        send_disable_cloud(&ctx, &mut launcher_context);
        send_bg_pause_enabled(&ctx, &mut launcher_context);
        send_launcher_completed(&ctx, &mut launcher_context);

        _ = output.wait()?;
        thread_kill_tx.send(())?;

        // Release launcherCanReadSem for game process thread
        let launcher_can_read_name =
            CString::new(name_prefix.to_owned() + LAUNCHER_CAN_READ_MSG_SEM)?;
        let launcher_can_read_sem =
            unsafe { CreateSemaphoreA(None, 0, 1, PCSTR(launcher_can_read_name.as_ptr() as _))? };
        unsafe {
            ReleaseSemaphore(launcher_can_read_sem, 1, None)?;
        }

        process_game_messages_thread
            .join()
            .map_err(|_| anyhow::anyhow!("Process game thread join failed!"))?;

        unsafe {
            _ = UnmapViewOfFile(view_shared_memory);
            _ = CloseHandle(shared_memory);
            _ = CloseHandle(game_did_read_sem);
            _ = CloseHandle(game_can_read_sem);
            _ = CloseHandle(launcher_can_read_sem);
        }
    } else {
        log::info!(
            "Launching process {:?} with FFNx context: {:?}",
            process_filename,
            &ctx
        );
        let mut output = Command::new(process_filename).spawn()?;
        log::info!("Process launched (process_id: {})!", output.id());
        _ = output.wait()?;
    }

    Ok(())
}

fn handle_game_messages_thread(name_prefix: &str, thread_kill_rx: Receiver<()>) -> Result<()> {
    log::info!("Starting game message queue thread...");

    let launcher_can_read_name = CString::new(name_prefix.to_owned() + LAUNCHER_CAN_READ_MSG_SEM)?;
    let launcher_did_read_name = CString::new(name_prefix.to_owned() + LAUNCHER_DID_READ_MSG_SEM)?;

    let launcher_can_read_sem =
        unsafe { CreateSemaphoreA(None, 0, 1, PCSTR(launcher_can_read_name.as_ptr() as _))? };
    let launcher_did_read_sem =
        unsafe { CreateSemaphoreA(None, 0, 1, PCSTR(launcher_did_read_name.as_ptr() as _))? };

    loop {
        match thread_kill_rx.try_recv() {
            Ok(_) | Err(TryRecvError::Disconnected) => {
                break;
            }
            Err(TryRecvError::Empty) => {}
        }

        log::info!("Game message thread waiting for launcherCanReadSem semaphore...");
        unsafe { WaitForSingleObject(launcher_can_read_sem, INFINITE) };
        log::info!("Game message thread releasing launcherDidReadSem semaphore...");
        _ = unsafe { ReleaseSemaphore(launcher_did_read_sem, 1, None) };
    }
    unsafe {
        _ = CloseHandle(launcher_did_read_sem);
        _ = CloseHandle(launcher_can_read_sem);
    }
    log::info!("Game message queue thread terminated!");
    Ok(())
}

unsafe extern "system" fn exception_handler(ep: *const EXCEPTION_POINTERS) -> i32 {
    if HAD_EXCEPTION {
        log::error!("ExceptionHandler: crash while running another Exception Handler. Exiting.");
        SetUnhandledExceptionFilter(None);
        return EXCEPTION_CONTINUE_EXECUTION;
    }

    HAD_EXCEPTION = true;
    let exception_record = &*(*ep).ExceptionRecord;
    log::error!(
        "Exception 0x{:x}, address 0x{:x}",
        exception_record.ExceptionCode.0,
        exception_record.ExceptionAddress as i32
    );
    SetUnhandledExceptionFilter(None);
    EXCEPTION_CONTINUE_EXECUTION
}
