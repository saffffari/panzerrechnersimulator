# Live audio deadline isolation

The live log contained over 130 million repeated ALSA recover underrun lines (13.93 GB
reported by the root agent). The actual game was routed to the default Bowers & Wilkins Px8
stereo sink through PipeWire ALSA. Sandbox process/server visibility initially hid that fact;
the live host read confirmed it. No device routing change is made.

The actual P08 route is present: successful discharge -> authoritative round -> generated or
owned pistol source variant -> physical own-vehicle ear level -> hearing and shared master.
There is no bounded voice eviction in `retain_transient` that drops pistol sounds. These
facts do not prove audible delivery during the observed backend underrun storm.

Before this change, source mixing, every voice's hearing processing and shared limiting ran
inside the CPAL/ALSA output callback. Live output now renders on a dedicated producer into a
bounded four-block queue (256 stereo frames/block, about 23 ms maximum queued at44.1kHz).
The output callback reads ready fixed blocks without waiting for source DSP. A late producer
returns a whole silent stereo block, retaining queued event ordering and channel alignment.
Silence is a visible failure counter, not a claim that jitter has been solved.

`master_telemetry.live_delivery` reports starved blocks, produced blocks and maximum producer
block computation nanoseconds. Native stress acceptance needs starvation near zero and a
stable ALSA stream; offline mixer tests cannot establish this. Producer clock leads output
by the bounded queue; live audio adds this delay. Offline capture retains its former exact
master/capture source path. Shared hearing, death fade and limiter remain upstream.

Tests: `bounded_output_preserves_delivered_order_across_producer_pause`,
`starved_callback_returns_whole_silent_stereo_blocks_without_waiting`,
`vent_source_is_finite_has_zero_onset_and_a_finite_tail`.

The actual recorded wreck pressure breach now also emits one finite authored roar/hiss/metal
release on the existing delayed acoustic wavefront route. Its source level is an explicitly
estimated armor-strike level minus8dB, not a measured historical pressure reservoir model.

## Opt-in actual device stress fixture
`live_device_audio_delivery_review` runs only when `KESSEL_LIVE_AUDIO_REVIEW_PREFIX`
is supplied. It opens the ordinary device output for30wall-clock seconds, adds an owned
quiet score, engine/tracks/traverse/elevation, authoritative75ms MG fixture events,
P08 variants and main-gun/recoil/loader/case/footstep interactions. It writes a receipt
before asserting zero producer starvation and an advancing device sample clock. The root
runs it against its newly compiled test binary without GPU work. It does not capture audible
output itself or claim that the source fixtures replicate gameplay authority.
`pistol_actual_direct_variant_route_reaches_capture_output` separately verifies the same
P08 direct-variant and physical-ear playback functions reach finite nonzero mixer output.

## Hardware startup burst and delivery clock

The v2 device run reduced starvation from 1,986 blocks to one, but did not pass. CPAL ALSA requests a 100 ms hardware buffer with 25 ms periods. The previous 16-block queue held only 92.9 ms at 44.1 kHz and could not cover hardware startup priming. The bounded queue now derives its depth from that configuration: ceil(44,100 × 0.100 / 256) + 2 = 20 blocks (116.1 ms), plus the current block. This is a startup-buffer bound, not an unrestricted latency increase. A producer-free regression drains a full 100 ms priming burst from a prefilled queue and requires zero starvation.

Telemetry records the first missing block's consumed source-rate frame and wall microseconds since output construction, allowing the actual device review to distinguish startup from ongoing starvation. These are frames delivered into the output/resampling source, not hardware callback frames. The real-device zero-starvation acceptance remains unchanged and must be rerun by the coordinator.
