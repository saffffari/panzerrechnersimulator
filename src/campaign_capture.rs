//! Render-sampling latch for the campaign film's authoritative Tiger-loss event.
//!
//! Combat still decides whether and when the Tiger is destroyed. This tiny state machine only
//! guarantees that a death which occurs between 24 Hz output samples remains observable by the
//! presentation camera for the requested number of encoded frames.

#[derive(Clone, Debug, Default)]
pub(crate) struct FatalCaptureLatch {
    death_at: Option<f32>,
    first_interior_frame_at: Option<f32>,
    current_interior_hold: f32,
    maximum_interior_hold: f32,
}

impl FatalCaptureLatch {
    /// Observe the authoritative simulation state at simulation rate. Returns true exactly once,
    /// on the living-to-destroyed edge; a persistent dead state can never retrigger the event.
    pub(crate) fn observe_authoritative_state(
        &mut self,
        now: f32,
        was_alive: bool,
        alive: bool,
    ) -> bool {
        if self.death_at.is_none() && was_alive && !alive {
            self.death_at = Some(now);
            self.first_interior_frame_at = None;
            self.current_interior_hold = 0.0;
            self.maximum_interior_hold = 0.0;
            true
        } else {
            false
        }
    }

    /// Account only for a frame that was actually written to the movie encoder. Counting output
    /// samples instead of 60 Hz simulation ticks makes the hold contract true of the video itself.
    pub(crate) fn note_rendered_camera_frame(
        &mut self,
        now: f32,
        interior: bool,
        frame_seconds: f32,
    ) {
        let Some(death_at) = self.death_at else {
            return;
        };
        if now + f32::EPSILON < death_at || !frame_seconds.is_finite() || frame_seconds <= 0.0 {
            return;
        }
        if interior {
            self.first_interior_frame_at.get_or_insert(now);
            self.current_interior_hold += frame_seconds;
            self.maximum_interior_hold = self.maximum_interior_hold.max(self.current_interior_hold);
        } else {
            self.current_interior_hold = 0.0;
        }
    }

    pub(crate) fn requires_full_turret_camera(&self, required_seconds: f32) -> bool {
        self.death_at.is_some() && self.maximum_interior_hold + 1.0e-5 < required_seconds.max(0.0)
    }

    pub(crate) fn has_required_hold(&self, required_seconds: f32, tolerance: f32) -> bool {
        self.death_at.is_some()
            && self.first_interior_frame_at.is_some()
            && self.maximum_interior_hold + tolerance.max(0.0) >= required_seconds.max(0.0)
    }

    pub(crate) fn death_at(&self) -> Option<f32> {
        self.death_at
    }

    pub(crate) fn first_interior_frame_at(&self) -> Option<f32> {
        self.first_interior_frame_at
    }

    pub(crate) fn maximum_interior_hold(&self) -> f32 {
        self.maximum_interior_hold
    }
}
