"""Publish verified local footage to the existing private review, without moving originals."""
from pathlib import Path
from html import escape
import json
import re

ROOT = Path(__file__).resolve().parents[1]
CAP = ROOT / "captures"
PAGE = CAP / "development_review_20261030.html"


def load(prefix):
    return json.loads((CAP / (prefix + ".json")).read_text(encoding="utf-8"))


def card(key, title, category, prefix, body, limits, video=None, poster=None, extra="", receipt=None):
    kind = "video" if video else "stills"
    media = ""
    if video:
        assert (CAP / video).is_file(), video
        media = f'<video controls playsinline preload="none" poster="{escape(poster)}"><source src="{escape(video)}" type="video/mp4"></video>'
        media += f'<p class="openfile"><a href="{escape(video)}">Open clip separately</a></p>'
    elif poster:
        assert (CAP / poster).is_file(), poster
        media = f'<a class="enlarge" href="{escape(poster)}"><img style="width:100%;image-rendering:pixelated" loading="lazy" src="{escape(poster)}" alt="{escape(title)}"></a>'
    return f'''<article class="card {kind}" id="{key}" data-category="{category}" data-kind="{kind}">
<div class="card-head"><span class="eyebrow">31 Oct / actual game recording</span><span class="badge {kind}">{'PLAYABLE CLIP' if video else 'MEASURED REVIEW'}</span><h2>{escape(title)}</h2></div>
{media}<div class="copy"><p><strong>What this shows</strong>{escape(body)}</p><p class="limits"><strong>Limits</strong>{escape(limits)}</p>{extra}</div>
<details><summary>Original evidence</summary><a href="{escape(receipt or (prefix + '.json'))}">Runtime receipt</a></details></article>'''


def main():
    cards = []
    medical_runs = sorted((p for p in CAP.glob("foundation_medical_20261031_b*.json")
                           if re.fullmatch(r"foundation_medical_20261031_b\d+", p.stem)),
                          key=lambda p: int(p.stem.rsplit("_b", 1)[1]), reverse=True)
    medical = next((p.stem for p in medical_runs if load(p.stem).get("pass")), None)
    if medical:
        r = load(medical)
        excerpts = (CAP / (medical + "_highlights.mp4")).is_file()
        medical_video = medical + ("_highlights.mp4" if excerpts else ".mp4")
        cards.append(card("field-medical-recovery", "One wounded crewman: field care to the medical tent", "crew", medical,
            f"{len(r['checks'])} checks passed along the recorded journey: use the finite medical kit, move the existing winch clear, load the same wounded man, drive back using fuel, unload him through the real gate and deliver him to a bed for a named medic's care. His identity and injuries persist.",
            ("Edited normal-speed excerpts; the full journey takes nearly six minutes, including a slow assisted drag to the tent. " if excerpts else "") + "Initial injury and field placement are staged; subsequent movement uses ordinary gameplay contacts. Recorded offline at 12 fps, silent, with initial enemies disabled. The patient keeps his seated leg pose on the bed; supported limb settling remains unfinished. The final view also needs a better camera angle. Assisted handling does not simulate human lifting forces. This is the generic carrier, not the proposed Sd.Kfz. 251, and does not establish combat survival or live frame rate.",
            video=medical_video, poster=medical + "_02_loaded.png",
            extra=f'<p><a href="{medical}.mp4">Full uncut journey ({r["seconds"]:.0f} seconds)</a> · <a href="{medical}_01_field_care.png">Field care</a> · <a href="{medical}_04_admitted.png">Bed pose needing work</a></p>'))
    ground_runs = sorted((ROOT / "artifacts" / "expedition-20261030").glob("ground_canyon_b*_a.json"),
                         key=lambda p: int(p.stem.split("_b", 1)[1].split("_", 1)[0]), reverse=True)
    ground = next((p for p in ground_runs if (CAP / (p.stem + "_overview.png")).is_file()), None)
    if ground:
        r = json.loads(ground.read_text(encoding="utf-8"))
        counts = {state: sum(item.get("status") == state for item in r["placements"])
                  for state in ("admitted", "unresolved_original_pose_retained", "preserved_owner_pose")}
        admitted = counts["admitted"]
        unresolved = counts["unresolved_original_pose_retained"]
        # Preserve the original evidence path; copy the receipt into the local media server root.
        (CAP / ground.name).write_bytes(ground.read_bytes())
        cards.append(card("battlefield-grounding", "Battlefield placement audit — remaining work visible", "world", ground.stem,
            f"Actual canyon map: {admitted} of {admitted + unresolved} independent wreck placements pass the ground-contact and balance checks. {unresolved} remain unresolved and retain their original positions. {counts['preserved_owner_pose']} connected or intentionally placed pieces and {len(r.get('embedded_rock_formations', []))} embedded rock formations are reported separately.",
            "Placement uses each original model's shape and an estimated weight distribution. Falling motion, suspension and contact with adjacent objects still need further work. Unresolved objects remain visible. This exterior inspection view does not establish live frame rate.",
            poster=ground.stem + "_overview.png"))
    dial = "foundation_gunner_optic_20261031_b46"
    if (CAP / (dial + ".png")).is_file():
        cards.append(card("deeper-orange-dial", "Deeper orange instrument lighting", "interior", dial,
            "Fresh in-game gunner view at 640 × 360. The dial's physical illuminator is a deeper orange and its emitted power is reduced by ten percent. The adjacent indicator lamps retain their existing colors.",
            "Appearance check. This short offscreen capture does not establish live performance; KSP was left untouched as requested.",
            poster=dial + ".png",
            extra='<p><a href="foundation_gunner_optic_20261031_b44.png">Previous dial capture</a></p>'))
    survivor = "foundation_passenger_20261031_b46"
    if (CAP / (survivor + ".json")).is_file() and load(survivor).get("pass"):
        cards.append(card("last-passenger", "The last surviving passenger can take over", "crew", survivor,
            "Five checks passed: the existing survivor is offered, cancellation stops the wait, saving and rebuilding the world preserves all eleven state owners, the passenger physically exits, and control transfers to that same person at the exit with unchanged wounds and ammunition.",
            "Staged losses and an initially seated passenger; ordinary exit and succession behavior. These exterior stills are diagnostic views. They do not verify combat survival, boarding, or full articulated body clearance. The carrier shown is still the generic prototype, not the proposed Sd.Kfz. 251.",
            poster=survivor + "_03_exit_result.png",
            extra=f'<p><a href="{survivor}_01_closed_ramp.png">Closed ramp</a> · <a href="{survivor}_02_pending_exit.png">Exit in progress</a></p>'))
    loader = "foundation_loader_moving_20261031_b57"
    moving_loader = (CAP / (loader + "_manifest.json")).is_file()
    if not moving_loader:
        loader = "foundation_loader_20261031_b44"
    manifest = CAP / (loader + "_manifest.json")
    if manifest.is_file() and (CAP / (loader + ".mp4")).is_file():
        recorded = json.loads(manifest.read_text(encoding="utf-8"))
        if recorded.get("outcome_checks_passed") and recorded.get("required_timeline_actions_passed"):
            loader_extra = '<p><a href="foundation_loader_20261031_b44.mp4">Earlier parked reload for comparison</a></p>' if moving_loader else ""
            resume_path = CAP / "foundation_world_20261031_b58.json"
            if moving_loader and resume_path.is_file():
                resume = json.loads(resume_path.read_text(encoding="utf-8"))
                if resume.get("pass") and resume.get("loader_active_work_test", {}).get("pass") and resume.get("corpse_allocation_test", {}).get("pass"):
                    loader_extra += '<p>Saving and resuming during both carrying and ramming also passed the <a href="foundation_world_20261031_b58.json">battlefield reconstruction check</a>. That comparison uses muted audio and does not verify live sound continuity.</p>'
            cards.append(card("foundation-loader", "Loader: carrying and ramming while the tank moves" if moving_loader else "Loader: shot, extraction, reload and braking", "crew", loader,
                "Eighteen seconds inside the compartment. One normal shot, one finite rack round and a completed reload. The same round is carried at 5–6 seconds and rammed at 7 seconds while the tank moves. It stops between 7 and 8 seconds; the reload is complete by 8. The round now follows common hull motion even when the loader pauses." if moving_loader else "Eighteen seconds inside the compartment. One normal trigger press fires the gun, the loader extracts the case, takes one existing round from its rack, reloads and returns to standing. The driver then accelerates and brakes. Moving hand targets now update on their simulation tick instead of waiting for the idle animation clock.",
                "Diagnostic compartment camera; enemy return fire is held. Recorded offline at 24 fps, with game sound; this does not establish live frame rate. One-second samples do not prove exact contact throughout the motion. Fingers do not yet wrap around objects, and the rack hides part of the crouch. Full body and cartridge swept clearance remains unfinished. This is a correction review, not a finished-animation claim.",
                loader + ".mp4", loader + ("_t6.png" if moving_loader else ".png"), receipt=loader + "_manifest.json",
                extra=loader_extra))
    candidates = ["firstperson_combat_20261031_b39_seed12628273", "firstperson_combat_20261031_b38_seed12628273"]
    prefix = next((p for p in candidates if (CAP / (p + ".mp4")).is_file() and load(p).get("status") == "recorded"), None)
    if prefix:
        r = load(prefix)
        o = r["interior_observed"]
        cards.append(card("first-person-combat", "Inside the fighting compartment", "crew", prefix,
            f"{r['elapsed_seconds']:g} continuous seconds from the ordinary commander eye: {o['actual_main_gun_discharges']} real main-gun discharges and {o['actual_loader_cycles']} completed NPC reload cycles. Enemy fire, crew movement and recorded game sound remain active. The camera turns using normal look controls toward the loader's work.",
            "Recorded offline at 24 frames per second, played at normal simulation speed. This is not proof of live frame rate. Initial arena placement is staged; shots, damage and reload outcomes are not injected. Dark clothing and the cabin lighting still make fine hand contact difficult to judge. This is an automated controller, not an independent human playtest.",
            prefix + ".mp4", prefix + ("_review_t26s.png" if (CAP / (prefix + "_review_t26s.png")).is_file() else "_poster.png")))
    battle = "combined_battle_20261031_b38_seed12628273"
    if (CAP / (battle + "_first65s.mp4")).is_file():
        r = load(battle)
        assert r["status"] == "recorded"
        cards.append(card("combined-combat", "Nine tanks and 28 infantry", "combat", battle,
            "The opening 65 seconds of a continuous three-minute arena battle: three Tigers, six T-34s and 28 infantry, using installed tank policies, ordinary weapon rules and finite ammunition. Close vehicle and infantry camera cuts supplement the opening wide view.",
            "The run stalled with three Tigers and thirteen Soviet riflemen alive after the opposing armour was disabled. It did not produce a last-man-standing winner. Allied armour still needs infantry engagement behavior. External cameras retain the player's enclosed Tiger audio listener. Offline 24 fps recording; not a live performance test.",
            battle + "_first65s.mp4", battle + "_frame20.png",
            f'<p><a href="{battle}.mp4">Full uncut 180-second run</a></p>'))
    perf = "liveframe_20261031_b42_pipelined_ultra640_commander"
    if (CAP / (perf + ".json")).is_file():
        rows = []
        for p in [perf, "liveframe_20261031_b42_pipelined_ultra1024_commander"]:
            r = load(p)
            rows.append(f'<tr><td>{r["internal_resolution"][0]} × {r["internal_resolution"][1]}</td><td>{r["completed_frames_per_second"]:.1f} FPS</td><td>{r["mean_completed_frame_interval_ms"]:.1f} ms</td></tr>')
        table = '<table style="width:100%;text-align:left;border-spacing:0 8px"><thead><tr><th>Ultra / RTX 4090 / b42</th><th>Completed throughput</th><th>Mean interval</th></tr></thead><tbody>' + ''.join(rows) + '</tbody></table>'
        cards.append(card("renderer-performance", "Earlier FPS runs — clean baseline pending", "interior", perf,
            "Same-build quiet commander view, including solo crew and ordinary interior rendering, allowing CPU/GPU work to overlap. Each result covers 180 completed frames after 60 warmup frames and includes the final GPU wait. 640×360 scales exactly eight times to the Studio Display; 1024×576 matches the newer DeltaP resolution and scales exactly five times.",
            "GPU contention was not controlled in these earlier runs. On 31 October the RTX 4090 measured 97–99% busy with KSP open while this game was stopped; the owner chose to keep KSP open. These numbers therefore do not establish standalone game performance or whether the 30–60 FPS target is met. They are short headless measurements, not sustained combat minima or input latency. Two culling experiments remain disabled in normal play. A clean baseline is pending. The reference still is an additional render, not a timed frame.",
            poster=perf + ".png", extra=table))
    panel = "crew_status_panel_20261031_b39"
    if (CAP / (panel + ".json")).is_file() and load(panel).get("pass"):
        cards.append(card("crew-layout", "Two front seats, three turret seats", "crew", panel,
            "The physical crew lamps now follow the seating arrangement: driver and radio operator in front, gunner, commander and loader behind. The same source-mounted positions drive painted role symbols, picking and the illuminated assignment wire.",
            "This is a fictional gameplay instrument. Wounded and dead states are explicitly staged through the casualty system. Close-ups use a diagnostic inspection camera; wide shots use the ordinary commander eye. The clip demonstrates selection, not the complete seat-transfer journey.",
            panel + "_pulse_and_selection.mp4", panel + "_night_healthy_close.png"))
    fuel = "tiger_refuel_20261031_b43"
    fuel_visual = ROOT / "artifacts" / "expedition-20261030" / "tiger-refuel-b43-visual-review.json"
    if (CAP / (fuel + ".json")).is_file() and load(fuel).get("pass") and fuel_visual.is_file() and json.loads(fuel_visual.read_text(encoding="utf-8")).get("pass"):
        r = load(fuel)
        count = len(r["checks"])
        cards.append(card("physical-refuelling", "Open, pour, interrupt, resume and seal", "world", fuel,
            f"{count} checks passed through the ordinary interaction path: both retained caps open, an existing can supplies exactly five litres, and both caps close again. A cold reconstruction preserves all eleven battlefield state owners midway through pouring; twelve following ticks match uninterrupted play. Releasing the control, looking away, a full tank and keeper death stop transfer.",
            "Automated equipment review. The operator and finite can are staged on the actual rear deck; climbing onto it is not tested. Initial enemies are disabled, but the ordinary campaign director remains active. Death is tested on a disposable saved branch. No liquid simulation, complete arm animation or historical valve procedure is claimed.",
            poster=fuel + "_03_pouring.png",
            extra=f'<p><a href="{fuel}_01_closed.png">Before opening</a> · <a href="{fuel}_02_open.png">Both caps removed</a> · <a href="{fuel}_04_restored.png">Mid-pour restore</a> · <a href="{fuel}_05_closed.png">Caps sealed again</a></p>'))
    text = PAGE.read_text(encoding="utf-8")
    text = text.replace('Development review · 30 October 2026</title>', 'Development review · 31 October 2026</title>')
    text = text.replace('29–30 October 2026', '29–31 October 2026')
    text = text.replace('The loader clip has sound; the remaining clips are silent.', 'The loader and new combat clips have game sound; the other clips are silent.')
    wide_video_rule = '@media(max-width:1050px){.grid{grid-template-columns:1fr}}'
    if wide_video_rule not in text:
        text = text.replace('</style>', wide_video_rule + '</style>', 1)
    text = re.sub(r'<!-- renderer-20261031:start -->.*?<!-- renderer-20261031:end -->', '', text, flags=re.S)
    text = text.replace('<div class="grid">', '<div class="grid"><!-- renderer-20261031:start -->' + ''.join(cards) + '<!-- renderer-20261031:end -->', 1)
    new_links = []
    if medical:
        new_links.append('<a href="#field-medical-recovery" data-reveal="field-medical-recovery">casualty recovery</a>')
    if ground:
        new_links.append('<a href="#battlefield-grounding" data-reveal="battlefield-grounding">battlefield placement audit</a>')
    new_links.extend(['<a href="#foundation-loader" data-reveal="foundation-loader">loader review</a>',
                      '<a href="#deeper-orange-dial" data-reveal="deeper-orange-dial">deeper orange dial</a>',
                      '<a href="#first-person-combat" data-reveal="first-person-combat">first-person combat</a>'])
    note = '<p class="pending-note">Latest: ' + ', '.join(new_links) + '. Offline videos do not establish live frame rate.</p>'
    text = re.sub(r'<p class="pending-note">.*?</p>', note, text, count=1, flags=re.S)
    total = len(re.findall(r'<article\b', text)); videos = len(re.findall(r'<video\b', text)); pending = len(re.findall(r'data-kind="pending"', text))
    pending_stat = f'<span><b>{pending}</b> missing media</span>' if pending else ''
    text = re.sub(r'<div class="stats">.*?</div>', f'<div class="stats"><span><b>{videos}</b> playable clips</span><span><b>{total-videos-pending}</b> still-image reviews</span>{pending_stat}</div>', text, count=1, flags=re.S)
    text = re.sub(r'\d+ feature reviews · latest completion evidence first', f'{total} feature reviews · latest completion evidence first', text)
    media_refs = re.findall(r'(?:src|poster)="([^"]+)"', text)
    missing_media = [ref for ref in media_refs
                     if not ref.startswith(('http:', 'https:', 'data:'))
                     and not (CAP / ref).is_file()]
    assert not missing_media, f"Missing gallery media: {missing_media}"
    PAGE.write_text(text, encoding="utf-8", newline="\n")
    print(json.dumps({"page": str(PAGE), "new_cards": len(cards), "videos": videos,
                      "validated_media_references": len(media_refs)}))


if __name__ == "__main__":
    main()
