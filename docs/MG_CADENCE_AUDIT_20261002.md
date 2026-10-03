# Tiger MG34 cadence hotfix — 2026-10-02

## Historical rate

The US War Department *Intelligence Bulletin*, Vol II No 4, December 1943, printed page32 (PDFpage38), states “Cyclic rate of fire 800–900 rpm.” It separately lists practical light/heavy rates of150/300rpm; these include aiming and interruptions and are not the spacing inside an automatic burst.

Primary scan: https://www.bulletpicker.com/pdf/Intelligence-Bulletin-Vol-2-No-04.pdf#page=38

The official US Army historical chapter also reports MG34 cyclic800–900rpm, distinguishes it from practical rates, and lists MG42 separately at1335rpm: https://achh.army.mil/history/book-wwii-woundblstcs-chapter1/

The existing Tiger setting0.075s=800rpm is within that documented range and is retained. No claim is made that every gun/ammunition condition had exactly800rpm. No MG42 or accelerated MG34 variant is substituted.

## Fault and correction

The live catch-up path partitions a slow frame into at most50ms simulation steps. At4FPS it can therefore count the correct Tiger rounds, but previously submitted every receiver/muzzle transient from those steps immediately to the output mixer. Several75ms cycles could arrive as one stacked sound every250ms.

A shared transient timeline now collects source/arrival timestamps during live catch-up and submits the prepared sounds against the master mixer sample clock. A late batch is shifted as one unit to available output time; its inter-event spacing is preserved. This cannot reproduce an event in the past, so severe stalls can still add audible latency. Offline explicit-step capture retains its existing clock. Pausing freezes pending source samples.

The MG phase helper emits every shot offset in the elapsed interval and retains fractional phase, replacing the old four-round guard. MG muzzle wavefronts and receiver reports use those offsets. Small-arm payloads retain their emission time so the240Hz flight step cannot advance a new round before its discharge. Receiver burst calibration uses the same exact800/60 rounds per second instead of a hardcoded13.

## Scope and unresolved behavior

- Tiger coax/hull share the original800rpm cadence and existing thermal model. They still have unlimited legacy feed: no finite belt, reserve inventory, or MG reload model existed. No ammunition allowance was invented in this cadence hotfix. P08 partial reload is separate work.
- Legacy enemy T34 coax currently emits one round per1.3s despite its function being called a burst. That is an AI burst-model deficiency, not the Tiger cyclic setting; no MG34 rate is assigned to the Soviet DT.
- Infantry uses the rifle cadence1.6s and a5-round magazine; it is not the Tiger MG path. Its decision cooldown remains tick-quantized.
- Live main-loop gaps greater than5s are explicitly treated as suspension and reset to1/60s. Ordinary slow frames up to5s consume elapsed time; the new sample-clock delivery does not discard their MG cycles. This suspension policy prevents replaying unseen combat after an OS pause; it is not a performance fix.

## Validation

CPU tests were added for shot count/phase at60,4,2,and1FPS, exact shared sample-clock spacing after a late batch, four actual offline-mixer transients remaining75ms apart, and pause preserving the three pending reports and their spacing. Tests have not yet been compiled/run by this agent; root coordinates the build. No GPU or game run performed.
