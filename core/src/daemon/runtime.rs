//! Author: [Seclususs](https://github.com/seclususs)

use crate::config::runtime;
use crate::daemon::{bridge, state, traits, types};
use crate::hal::properties;

use rustix::event;
use std::{io, os, sync, thread, time};

const EPOLL_ERROR_SLEEP_MS: u64 = 500;

pub struct RecoverableService {
    pub name: &'static str,
    pub handler: Option<Box<dyn traits::EventHandler>>,
    pub factory: Box<dyn Fn() -> types::Result<Box<dyn traits::EventHandler>> + Send + Sync>,
    pub cooldown_start: Option<time::Instant>,
    pub last_tick: time::Instant,
    pub registered_in_epoll: bool,
    pub is_permanently_disabled: bool,
}

impl RecoverableService {
    pub fn new<F>(name: &'static str, factory: F) -> Self
    where
        F: Fn() -> types::Result<Box<dyn traits::EventHandler>> + Send + Sync + 'static,
    {
        let initial_cooldown = time::Instant::now()
            .checked_sub(time::Duration::from_secs(runtime::COOLDOWN_DURATION_SEC))
            .unwrap_or(time::Instant::now());

        Self {
            name,
            handler: None,
            factory: Box::new(factory),
            cooldown_start: Some(initial_cooldown),
            last_tick: time::Instant::now(),
            registered_in_epoll: false,
            is_permanently_disabled: false,
        }
    }

    fn try_initialize(&mut self) -> bool {
        if self.is_permanently_disabled {
            return false;
        }

        match (self.factory)() {
            Ok(handler) => {
                log::debug!("Service '{}' initialized successfully.", self.name);
                self.handler = Some(handler);
                self.cooldown_start = None;
                self.last_tick = time::Instant::now();
                true
            }

            Err(e) => {
                match &e {
                    types::QosError::IoError(io_err)
                        if io_err.kind() == io::ErrorKind::NotFound =>
                    {
                        log::error!(
                            "Service '{}' failed FATALLY: {e}. Disabling permanently.",
                            self.name
                        );
                        self.is_permanently_disabled = true;
                    }

                    types::QosError::SystemCheckFailed(msg)
                    | types::QosError::PermissionDenied(msg) => {
                        log::error!(
                            "Service '{}' failed: {msg}. Disabling permanently.",
                            self.name
                        );
                        self.is_permanently_disabled = true;
                    }
                    _ => {
                        log::warn!(
                            "Failed to initialize service '{}': {e}. Retrying...",
                            self.name
                        );
                        self.cooldown_start = Some(time::Instant::now());
                    }
                }
                false
            }
        }
    }

    fn unregister_if_active(&mut self, epoll_fd: os::fd::RawFd, id: u64) {
        if self.registered_in_epoll {
            if let Some(ref h) = self.handler
                && let Some(fd) = h.as_raw_fd()
            {
                epoll_mod(
                    epoll_fd,
                    fd,
                    id,
                    libc::EPOLL_CTL_DEL,
                    event::epoll::EventFlags::empty(),
                );
            }
            self.registered_in_epoll = false;
        }
    }
}

fn epoll_mod(
    epoll_fd: os::fd::RawFd,
    fd: os::fd::RawFd,
    id: u64,
    op: i32,
    events: event::epoll::EventFlags,
) -> bool {
    let epoll_fd = unsafe { os::fd::BorrowedFd::borrow_raw(epoll_fd) };
    let target_fd = unsafe { os::fd::BorrowedFd::borrow_raw(fd) };
    let event_data = event::epoll::EventData::new_u64(id);

    let res = match op {
        libc::EPOLL_CTL_ADD => event::epoll::add(epoll_fd, target_fd, event_data, events),
        libc::EPOLL_CTL_DEL => event::epoll::delete(epoll_fd, target_fd),
        libc::EPOLL_CTL_MOD => event::epoll::modify(epoll_fd, target_fd, event_data, events),
        _ => return false,
    };

    if let Err(e) = res {
        let errno = e.raw_os_error();

        if (op == libc::EPOLL_CTL_DEL && errno == libc::ENOENT)
            || (op == libc::EPOLL_CTL_ADD && errno == libc::EEXIST)
        {
            return true;
        }

        log::warn!("Epoll op {op} failed for ID {id}: {e}");
        return false;
    }
    true
}

fn is_fatal_runtime_error(e: &types::QosError) -> bool {
    match e {
        types::QosError::IoError(io) => matches!(
            io.kind(),
            io::ErrorKind::NotFound | io::ErrorKind::BrokenPipe | io::ErrorKind::PermissionDenied
        ),
        types::QosError::SystemCheckFailed(_) | types::QosError::PermissionDenied(_) => true,
        _ => false,
    }
}

pub fn wait_for_boot_completion(tag: &str) {
    log::debug!("Waiting for sys.boot_completed (Task: {tag})...");
    let mut retry_count = 0;

    loop {
        if state::SHUTDOWN_REQUESTED.load(sync::atomic::Ordering::Acquire) {
            return;
        }

        if let Ok(val) = properties::get_system_property("sys.boot_completed")
            && val == "1"
        {
            break;
        }

        retry_count += 1;
        if retry_count > runtime::BOOT_WAIT_RETRY_LIMIT {
            log::warn!("Boot property timeout (Task: {tag}).");
            break;
        }

        thread::sleep(time::Duration::from_secs(runtime::BOOT_POLL_INTERVAL_SEC));
    }

    if !state::SHUTDOWN_REQUESTED.load(sync::atomic::Ordering::Acquire) {
        log::debug!(
            "Stabilizing for {}s (Task: {tag})...",
            runtime::STABILIZATION_DELAY_SEC
        );

        thread::sleep(time::Duration::from_secs(runtime::STABILIZATION_DELAY_SEC));
    }
}

#[allow(clippy::too_many_lines, clippy::cast_possible_wrap)]
pub fn run_event_loop(mut services: Vec<RecoverableService>) -> types::Result<()> {
    let epoll_fd = event::epoll::create(event::epoll::CreateFlags::CLOEXEC).map_err(|e| {
        types::QosError::SystemCheckFailed(format!("Failed to create epoll: {e}").into())
    })?;

    let mut context = state::DaemonContext::new();
    let cooldown_dur = time::Duration::from_secs(runtime::COOLDOWN_DURATION_SEC);

    let mut events: [libc::epoll_event; runtime::MAX_EVENTS] =
        [libc::epoll_event { events: 0, u64: 0 }; runtime::MAX_EVENTS];

    while !state::SHUTDOWN_REQUESTED.load(sync::atomic::Ordering::Acquire) {
        let now = time::Instant::now();
        let mut next_wakeup =
            now + time::Duration::from_millis(runtime::MAX_EPOLL_TIMEOUT_MS as u64);

        for (i, service) in services.iter_mut().enumerate() {
            if service.is_permanently_disabled {
                continue;
            }

            if service.handler.is_none() {
                let Some(start) = service.cooldown_start else {
                    continue;
                };

                let elapsed = now.duration_since(start);
                if elapsed < cooldown_dur {
                    let wakeup_time = now + cooldown_dur.checked_sub(elapsed).unwrap_or_default();
                    if wakeup_time < next_wakeup {
                        next_wakeup = wakeup_time;
                    }
                    continue;
                }

                if !service.try_initialize() {
                    continue;
                }

                let Some(ref h) = service.handler else {
                    continue;
                };

                if let Some(fd) = h.as_raw_fd() {
                    let flags = h.get_poll_flags();

                    if epoll_mod(
                        os::fd::AsRawFd::as_raw_fd(&epoll_fd),
                        fd,
                        i as u64,
                        libc::EPOLL_CTL_ADD,
                        flags,
                    ) {
                        service.registered_in_epoll = true;
                    } else {
                        service.handler = None;
                        service.cooldown_start = Some(now);
                    }
                } else {
                    service.registered_in_epoll = false;
                }

                continue;
            }

            let Some(ref handler) = service.handler else {
                continue;
            };

            let interval_ms = handler.get_timeout_ms();
            if interval_ms > 0 {
                let deadline = service.last_tick + time::Duration::from_millis(interval_ms as u64);
                if deadline < next_wakeup {
                    next_wakeup = deadline;
                }
            }
        }

        let min_wait_ms = next_wakeup.saturating_duration_since(now).as_millis() as i32;

        let nfds = unsafe {
            libc::epoll_wait(
                os::fd::AsRawFd::as_raw_fd(&epoll_fd),
                events.as_mut_ptr(),
                runtime::MAX_EVENTS as i32,
                min_wait_ms,
            )
        };

        if state::SHUTDOWN_REQUESTED.load(sync::atomic::Ordering::Acquire) {
            break;
        }

        if nfds < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() != io::ErrorKind::Interrupted {
                thread::sleep(time::Duration::from_millis(EPOLL_ERROR_SLEEP_MS));
            }
            continue;
        }

        for event in events.iter().take(nfds as usize) {
            let id = event.u64 as usize;

            let Some(service) = services.get_mut(id) else {
                continue;
            };

            let Some(ref mut handler) = service.handler else {
                continue;
            };

            match handler.on_event(&mut context) {
                Ok(traits::LoopAction::Continue) => {
                    service.last_tick = time::Instant::now();
                }
                Err(e) => {
                    log::error!("Service '{}' event error: {e}", service.name);
                    service.unregister_if_active(os::fd::AsRawFd::as_raw_fd(&epoll_fd), id as u64);
                    service.handler = None;
                    service.cooldown_start = if is_fatal_runtime_error(&e) {
                        service.is_permanently_disabled = true;
                        None
                    } else {
                        Some(time::Instant::now())
                    };
                }
            }
        }

        let now_after_wait = time::Instant::now();

        for (i, service) in services.iter_mut().enumerate() {
            if service.is_permanently_disabled {
                continue;
            }

            let Some(ref mut handler) = service.handler else {
                continue;
            };

            let interval_ms = handler.get_timeout_ms();
            if interval_ms <= 0 {
                continue;
            }

            let elapsed = now_after_wait.duration_since(service.last_tick).as_millis();
            if elapsed < (interval_ms as u128) {
                continue;
            }

            service.last_tick = now_after_wait;

            match handler.on_timeout(&mut context) {
                Ok(traits::LoopAction::Continue) => {}
                Err(e) => {
                    log::error!("Service '{}' timeout error: {e}", service.name);
                    service.unregister_if_active(os::fd::AsRawFd::as_raw_fd(&epoll_fd), i as u64);
                    service.handler = None;
                    service.cooldown_start = if is_fatal_runtime_error(&e) {
                        service.is_permanently_disabled = true;
                        None
                    } else {
                        Some(time::Instant::now())
                    };
                }
            }
        }
    }

    for (i, service) in services.iter_mut().enumerate() {
        service.unregister_if_active(os::fd::AsRawFd::as_raw_fd(&epoll_fd), i as u64);
    }

    bridge::notify_service_death("Shutdown Clean");
    Ok(())
}
