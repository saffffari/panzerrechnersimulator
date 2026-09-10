//! Optional Linux mouse feedback through HAL's existing Keys tactile service.
//!
//! This adapter never opens HID devices, changes mouse settings, or starts a
//! service. Each accepted cue sends one finite waveform, without a strength
//! override. Missing/busy services are silent, and missed cues are never queued
//! for later playback. Other platforms have a no-op backend.

use crate::haptics::HapticCue;
use std::time::{Duration, Instant};

const MIN_PULSE_GAP: Duration = Duration::from_millis(125);
const RECONNECT_GAP: Duration = Duration::from_secs(1);

/// Mouse feedback is best effort; callers must gate it to active, focused play.
pub struct MouseHaptics {
    enabled: bool,
    active: bool,
    next_pulse: Option<Instant>,
    next_retry: Option<Instant>,
    backend: platform::Backend,
}

impl Default for MouseHaptics {
    fn default() -> Self {
        Self::new(true)
    }
}

impl MouseHaptics {
    /// Construction performs no socket or device I/O.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            active: enabled,
            next_pulse: None,
            next_retry: None,
            backend: platform::Backend::new(),
        }
    }

    /// Returns true only when the local service accepted the datagram, not when
    /// the mouse physically played it. Sends at most one cue per 125 ms.
    pub fn pulse(&mut self, cue: HapticCue) -> bool {
        self.pulse_at(cue, Instant::now())
    }

    fn pulse_at(&mut self, cue: HapticCue, now: Instant) -> bool {
        if !self.active
            || self.next_pulse.is_some_and(|deadline| now < deadline)
            || self.next_retry.is_some_and(|deadline| now < deadline)
        {
            return false;
        }
        if self.backend.send(packet(cue)) {
            self.next_pulse = Some(now + MIN_PULSE_GAP);
            self.next_retry = None;
            true
        } else {
            self.next_retry = Some(now + RECONNECT_GAP);
            false
        }
    }

    /// Suppress future requests and close our socket. Keys exposes no cancel
    /// command: a finite effect already accepted by that service may finish.
    /// No wire message is sent, and there is no local playback queue to drain.
    pub fn stop(&mut self) {
        self.active = false;
        self.backend.disconnect();
    }

    /// Resume future cues if this adapter was enabled at construction. Missed
    /// cues are not replayed, and existing rate/backoff limits remain in force.
    pub fn resume(&mut self) {
        self.active = self.enabled;
    }
}

fn packet(cue: HapticCue) -> &'static [u8] {
    match cue {
        HapticCue::MainGun => b"pulse sharp_collision",
        HapticCue::OwnTankHit { penetrated: true } => b"pulse damp_collision",
        HapticCue::OwnTankHit { penetrated: false } => b"pulse subtle_collision",
        HapticCue::MainGunMisfire => b"pulse sharp_state_change",
        HapticCue::RoundLoaded => b"pulse damp_state_change",
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::io;
    use std::os::unix::net::UnixDatagram;
    use std::path::PathBuf;

    pub(super) struct Backend {
        pub(super) path: PathBuf,
        socket: Option<UnixDatagram>,
    }

    impl Backend {
        pub(super) fn new() -> Self {
            Self {
                path: PathBuf::from("/run/keysd-haptics.sock"),
                socket: None,
            }
        }

        pub(super) fn send(&mut self, packet: &[u8]) -> bool {
            let result = (|| -> io::Result<()> {
                if self.socket.is_none() {
                    let socket = UnixDatagram::unbound()?;
                    socket.set_nonblocking(true)?;
                    socket.connect(&self.path)?;
                    self.socket = Some(socket);
                }
                let Some(socket) = &self.socket else {
                    return Err(io::Error::from(io::ErrorKind::NotConnected));
                };
                if socket.send(packet)? == packet.len() {
                    Ok(())
                } else {
                    Err(io::Error::from(io::ErrorKind::WriteZero))
                }
            })();
            if result.is_err() {
                self.disconnect();
            }
            result.is_ok()
        }

        pub(super) fn disconnect(&mut self) {
            self.socket = None;
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    pub(super) struct Backend;

    impl Backend {
        pub(super) fn new() -> Self {
            Self
        }
        pub(super) fn send(&mut self, _packet: &[u8]) -> bool {
            false
        }
        pub(super) fn disconnect(&mut self) {}
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::io;
    use std::os::unix::net::UnixDatagram;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_SOCKET: AtomicUsize = AtomicUsize::new(0);

    // Every test replaces the endpoint before any pulse. Tests never connect
    // to the actual mouse service, even when it is running on the test host.
    struct MockService {
        dir: PathBuf,
        path: PathBuf,
    }

    impl MockService {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "panzer-mouse-haptics-{}-{}",
                std::process::id(),
                NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&dir).unwrap();
            let path = dir.join("haptics.sock");
            Self { dir, path }
        }

        fn bind(&self) -> UnixDatagram {
            let socket = UnixDatagram::bind(&self.path).unwrap();
            socket.set_nonblocking(true).unwrap();
            socket
        }

        fn adapter(&self, enabled: bool) -> MouseHaptics {
            let mut adapter = MouseHaptics::new(enabled);
            adapter.backend.path = self.path.clone();
            adapter
        }
    }

    impl Drop for MockService {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
            let _ = std::fs::remove_dir(&self.dir);
        }
    }

    fn receive(socket: &UnixDatagram) -> Vec<u8> {
        let mut buffer = [0; 128];
        let size = socket.recv(&mut buffer).unwrap();
        buffer[..size].to_vec()
    }

    fn assert_empty(socket: &UnixDatagram) {
        let error = socket.recv(&mut [0; 128]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    }

    #[test]
    fn named_cues_preserve_service_strength() {
        let service = MockService::new();
        let receiver = service.bind();
        let mut adapter = service.adapter(true);
        let start = Instant::now();
        let cases = [
            (HapticCue::MainGun, "pulse sharp_collision"),
            (
                HapticCue::OwnTankHit { penetrated: true },
                "pulse damp_collision",
            ),
            (
                HapticCue::OwnTankHit { penetrated: false },
                "pulse subtle_collision",
            ),
            (HapticCue::MainGunMisfire, "pulse sharp_state_change"),
            (HapticCue::RoundLoaded, "pulse damp_state_change"),
        ];
        for (index, (cue, expected)) in cases.into_iter().enumerate() {
            assert!(adapter.pulse_at(cue, start + MIN_PULSE_GAP * index as u32));
            assert_eq!(receive(&receiver), expected.as_bytes());
        }
        assert_empty(&receiver);
    }

    #[test]
    fn burst_drops_cues_instead_of_queueing_them() {
        let service = MockService::new();
        let receiver = service.bind();
        let mut adapter = service.adapter(true);
        let start = Instant::now();
        assert!(adapter.pulse_at(HapticCue::MainGun, start));
        assert_eq!(receive(&receiver), b"pulse sharp_collision");
        for millis in 1..125 {
            assert!(!adapter.pulse_at(
                HapticCue::MainGunMisfire,
                start + Duration::from_millis(millis)
            ));
        }
        assert_empty(&receiver);
        assert!(adapter.pulse_at(HapticCue::RoundLoaded, start + MIN_PULSE_GAP));
        assert_eq!(receive(&receiver), b"pulse damp_state_change");
        assert_empty(&receiver);
    }

    #[test]
    fn stop_has_no_wire_command_and_resume_never_replays_or_bypasses_cooldown() {
        let service = MockService::new();
        let receiver = service.bind();
        let mut adapter = service.adapter(true);
        let start = Instant::now();
        assert!(adapter.pulse_at(HapticCue::MainGun, start));
        assert_eq!(receive(&receiver), b"pulse sharp_collision");
        adapter.stop();
        assert_empty(&receiver);
        assert!(!adapter.pulse_at(HapticCue::MainGunMisfire, start));
        adapter.resume();
        assert!(!adapter.pulse_at(HapticCue::RoundLoaded, start));
        assert_empty(&receiver);
        assert!(adapter.pulse_at(HapticCue::RoundLoaded, start + MIN_PULSE_GAP));
        assert_eq!(receive(&receiver), b"pulse damp_state_change");
        adapter.stop();
        assert!(!adapter.pulse_at(HapticCue::MainGun, start + Duration::from_secs(10)));
        assert_empty(&receiver);
    }

    #[test]
    fn missing_service_retries_only_a_fresh_cue_after_backoff() {
        let service = MockService::new();
        let mut adapter = service.adapter(true);
        let start = Instant::now();
        assert!(!adapter.pulse_at(HapticCue::MainGun, start));
        let receiver = service.bind();
        assert!(!adapter.pulse_at(HapticCue::MainGunMisfire, start + MIN_PULSE_GAP));
        assert_empty(&receiver);
        assert!(adapter.pulse_at(HapticCue::RoundLoaded, start + RECONNECT_GAP));
        assert_eq!(receive(&receiver), b"pulse damp_state_change");
        assert_empty(&receiver);
    }

    #[test]
    fn disabled_adapter_stays_silent_after_resume() {
        let service = MockService::new();
        let receiver = service.bind();
        let mut adapter = service.adapter(false);
        adapter.resume();
        assert!(!adapter.pulse(HapticCue::MainGun));
        adapter.stop();
        adapter.resume();
        assert!(!adapter.pulse(HapticCue::RoundLoaded));
        assert_empty(&receiver);
    }

    #[test]
    fn service_restart_recovers_on_a_fresh_event_without_replaying_failed_cues() {
        let service = MockService::new();
        let receiver = service.bind();
        let mut adapter = service.adapter(true);
        let start = Instant::now();
        assert!(adapter.pulse_at(HapticCue::MainGun, start));
        assert_eq!(receive(&receiver), b"pulse sharp_collision");
        drop(receiver);
        std::fs::remove_file(&service.path).unwrap();
        let replacement = service.bind();
        // A connected Unix datagram still points at the old endpoint. The
        // failed send closes it; only a later, new cue reconnects.
        assert!(!adapter.pulse_at(HapticCue::MainGunMisfire, start + MIN_PULSE_GAP));
        assert_empty(&replacement);
        assert!(adapter.pulse_at(
            HapticCue::RoundLoaded,
            start + MIN_PULSE_GAP + RECONNECT_GAP
        ));
        assert_eq!(receive(&replacement), b"pulse damp_state_change");
        assert_empty(&replacement);
    }
}
