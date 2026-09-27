//! Author: [Seclususs](https://github.com/seclususs)

use crate::controllers::{blocker, cleaner, cpu, signal, storage};
use crate::daemon::{bridge, logging, runtime, state, types};

use std::{os, sync, thread, time};

static MAIN_THREAD: sync::Mutex<Option<thread::JoinHandle<()>>> = sync::Mutex::new(None);

/// # Safety
/// Initializes the Rust runtime and starts background services.
/// # Requirements
/// * `signal_fd` must be a valid, open file descriptor.
/// * **Ownership Transfer**: The ownership of `signal_fd` is transferred to Rust.
///   The C++ caller must NOT close or use this FD after calling this function,
///   as Rust will close it upon shutdown.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn start_services(signal_fd: i32) -> i32 {
    let owned_signal_fd: os::fd::OwnedFd = unsafe { os::fd::FromRawFd::from_raw_fd(signal_fd) };

    {
        match MAIN_THREAD.lock() {
            Ok(guard) => {
                if guard.is_some() {
                    log::error!("Attempted to start services while already running!");
                    return -1;
                }
            }
            Err(e) => {
                log::error!("MAIN_THREAD mutex poison detected: {e}. Resetting...");
                return -1;
            }
        }
    }

    logging::init();

    let (tx, rx) = sync::mpsc::channel::<()>();
    let result = std::panic::catch_unwind(move || {
        log::debug!("Service entry point reached. Signal FD: {signal_fd}");

        state::SHUTDOWN_REQUESTED.store(false, sync::atomic::Ordering::Release);

        let handle = thread::Builder::new()
            .name("MainLoop".into())
            .stack_size(128 * 1024)
            .spawn(move || {
                if let Err(e) = tx.send(()) {
                    log::error!("Failed to send handshake: {e}.");
                }

                runtime::wait_for_boot_completion("MainLoop");

                if state::SHUTDOWN_REQUESTED.load(sync::atomic::Ordering::Acquire) {
                    return;
                }

                log::debug!("Constructing Service Vector...");
                let mut services = Vec::new();

                services.push(runtime::RecoverableService::new("Signal", move || {
                    let instance_fd = owned_signal_fd
                        .try_clone()
                        .map_err(types::QosError::IoError)?;

                    Ok(Box::new(signal::SignalController::new(instance_fd)))
                }));

                services.push(runtime::RecoverableService::new("Storage", || {
                    Ok(Box::new(storage::StorageController::new()?))
                }));

                services.push(runtime::RecoverableService::new("CPU", || {
                    Ok(Box::new(cpu::CpuController::new()?))
                }));

                services.push(runtime::RecoverableService::new("Cleaner", || {
                    Ok(Box::new(cleaner::CleanerController::new()?))
                }));

                services.push(runtime::RecoverableService::new("Blocker", || {
                    Ok(Box::new(blocker::BlockerController::new()?))
                }));

                let svc_len = services.len();
                log::debug!("Initializing Event Loop with {svc_len} services...");

                if let Err(e) = runtime::run_event_loop(services) {
                    log::error!("Fatal error in event loop: {e}");
                }
            })
            .expect("Failed to spawn MainLoop thread");

        match MAIN_THREAD.lock() {
            Ok(mut guard) => *guard = Some(handle),
            Err(poisoned) => *poisoned.into_inner() = Some(handle),
        }
    });

    if let Err(cause) = result {
        log::error!("Critical Panic during startup: {cause:?}");
        bridge::notify_service_death("Startup Panic");
        return -1;
    }

    match rx.recv_timeout(time::Duration::from_secs(5)) {
        Ok(()) => 0,
        Err(e) => {
            log::error!("Handshake failed: {e}");
            -1
        }
    }
}

/// # Safety
/// Joins the main event loop thread.
/// The caller must ensure that this function is **not** called from within the
/// Rust background thread itself (e.g., via a callback), as attempting to
/// join the current thread will result in a deadlock or panic.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn join_threads() {
    log::debug!("Requested join threads.");
    let handle_opt = match MAIN_THREAD.lock() {
        Ok(mut guard) => guard.take(),
        Err(poisoned) => poisoned.into_inner().take(),
    };

    if let Some(handle) = handle_opt
        && let Err(e) = handle.join()
    {
        log::error!("Main thread panicked during join: {e:?}");
    }
}
