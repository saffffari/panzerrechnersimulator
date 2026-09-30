#!/usr/bin/env python3
"""Build a local player-view review page from this pass's actual media and receipts."""
from pathlib import Path
import html
import json

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "captures" / "astra"


def load(name):
    return json.loads((OUT / name).read_text())


def esc(value):
    return html.escape(str(value), quote=True)


def build():
    before = "sound_mass_20260929_duel_before"
    after = "sound_mass_20260929_duel_after"
    for prefix in (before, after):
        if not (OUT / f"{prefix}.mp4").is_file():
            raise SystemExit(f"Missing review film: {prefix}.mp4")
    candidate = load(after + "_manifest.json")
    before_cal = load("sound_mass_20260929_baseline_receipt.json")
    after_cal_name = ("sound_mass_20260929_candidate_final_receipt.json"
                      if (OUT / "sound_mass_20260929_candidate_final_receipt.json").is_file()
                      else "sound_mass_20260929_candidate_receipt.json")
    after_cal = load(after_cal_name)
    rows = []
    for key, title in [
        ("engine_cross_country_buttoned", "Engine under load"),
        ("hit_close_side_penetration", "Dry side penetration"),
        ("hit_heavy_122mm_penetration", "Internal filler burst"),
    ]:
        a = next(e for e in before_cal["events"] if e["event"] == key)
        b = next(e for e in after_cal["events"] if e["event"] == key)
        rows.append(f"<tr><th>{esc(title)}</th><td>{a['laf_dbfs']:.1f} → {b['laf_dbfs']:.1f}</td>"
                    f"<td>{a['limiter_db']:.1f} → {b['limiter_db']:.1f}</td></tr>")
    fire_cards = []
    for prefix, title in [
        ("fire_aimed_lmb_20260929_review", "Aim at the burning surface"),
        ("fire_wrong_aim_lmb_20260929_review", "Spray away from the fire"),
    ]:
        if (OUT / f"{prefix}_audiofixed.mp4").is_file():
            prefix += "_audiofixed"
        media, receipt = OUT / f"{prefix}.mp4", OUT / f"{prefix}_manifest.json"
        if not media.is_file() or not receipt.is_file():
            continue
        data = load(receipt.name)
        status = "Checks passed" if data.get("outcome_checks_passed") else "Failed take — retained for diagnosis"
        fire_cards.append(f'<article><h3>{esc(title)}</h3><video controls preload="metadata" '
                          f'src="{esc(media.name)}" poster="{prefix}.png"></video>'
                          f'<p>{esc(status)} · <a href="{esc(receipt.name)}">Receipt</a></p></article>')
    master = candidate.get("audio_final", {}).get("master", {})
    hammer = ''
    hammer_prefix = 'toolkit_free_hammer_20260929_review'
    if (OUT / f'{hammer_prefix}_pickupfixed.mp4').is_file():
        hammer_prefix += '_pickupfixed'
    if (OUT / f'{hammer_prefix}.mp4').is_file():
        hammer_data = load(hammer_prefix + '_manifest.json')
        hammer_status = 'Checks passed' if hammer_data.get('outcome_checks_passed') else 'Checks failed'
        hammer_description = (
            'Actual toolbox pickup, release, quick clicks and a hold. The healthy Tiger receives '
            'no repair credit. This checks hand response; it does not prove that a broken '
            'traverse can be reached and repaired.'
            if hammer_data.get('outcome_checks_passed') else
            'Failed pickup take, retained for diagnosis. This film does not establish usable '
            'hammer swings or traverse repair.'
        )
        hammer = (f'<h2>LMB hammer response</h2><p>{hammer_description}</p><article><video controls '
                  f'preload="metadata" src="{hammer_prefix}.mp4" poster="{hammer_prefix}.png"></video>'
                  f'<p>{hammer_status} · <a href="{hammer_prefix}_manifest.json">Receipt</a></p></article>')
    # Keep telemetry names from the runtime rather than assuming a threshold field spelling.
    master_text = json.dumps(master, indent=2)
    cupola = ''
    optic_before = 'cupola_lower_field_20260929_baseline'
    optic_after = 'cupola_lower_field_20260929_corrected'
    if all((OUT / f'{prefix}.mp4').is_file() for prefix in (optic_before, optic_after)):
        optic_cards = []
        for title, prefix in [('Before: centred virtual pupil', optic_before),
                              ('After: observer direction preserved', optic_after)]:
            receipt = load(prefix + '_manifest.json')
            status = 'State checks passed' if receipt.get('outcome_checks_passed') else 'State checks failed'
            optic_cards.append(f'<article><h3>{title}</h3><video controls preload="metadata" '
                               f'src="{prefix}.mp4" poster="{prefix}_leftfront.png"></video>'
                               f'<p>{status} · <a href="{prefix}_manifest.json">Receipt</a></p></article>')
        cupola = ('<h2 id="cupola">Vision block: lower-field obstruction</h2>'
                  '<p>The healthy front-left commander’s block looks past a retained roof cap. '
                  'The old raster view lost the focused eye’s upward direction and put that cap '
                  'across the centre. The correction preserves the source relay’s ray direction; '
                  'the actual lower-field obstruction remains. Each film compares this block with '
                  'the front block. This does not identify an unconfirmed block in a live session.</p>'
                  '<div class="grid">' + ''.join(optic_cards) + '</div>')
    passed = candidate.get("outcome_checks_passed", False)
    page = '''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Panzerrechnersystem · Sound and consequence</title>
<style>
:root{color-scheme:dark;font:16px/1.55 system-ui,sans-serif;background:#111516;color:#e8e5da}
*{box-sizing:border-box}body{max-width:1220px;margin:auto;padding:40px 28px 80px}
.eyebrow{letter-spacing:.16em;text-transform:uppercase;font-size:12px;color:#b7b09a}
h1{font-size:clamp(30px,5vw,58px);line-height:1.04;margin:18px 0}h2{font-size:23px;margin:34px 0 16px}
h3{font-size:18px;margin:0 0 14px}p{max-width:80ch;color:#bfc3bd}video{width:100%;background:#000;border:1px solid #3a403c;display:block}
.bar{display:flex;flex-wrap:wrap;align-items:center;gap:10px;margin:16px 0}
button{border:1px solid #77785c;background:#222823;color:#eee9d6;padding:10px 15px;cursor:pointer;font:inherit}
button[aria-pressed=true]{background:#dbbf72;color:#171b18;border-color:#dbbf72}button:focus-visible{outline:3px solid #eee;outline-offset:3px}
.status{color:#dcbe76;margin-left:auto}.grid{display:grid;grid-template-columns:1fr 1fr;gap:24px}
article,details{background:#191f1d;border:1px solid #343d37;padding:20px}summary{cursor:pointer}a{color:#ddc989}
table{border-collapse:collapse;width:100%;margin:18px 0}th,td{text-align:left;border-bottom:1px solid #3b413b;padding:12px}
pre{overflow:auto;font-size:12px;color:#b8c8bd}small{color:#a7afa5}.note{border-left:3px solid #aa8754;padding-left:17px}
@media(max-width:720px){body{padding:26px 16px}.grid{grid-template-columns:1fr}.status{margin-left:0}th,td{padding:8px}}
</style>
<div class="eyebrow">Panzerrechnersystem · 29 September 2026 · local review</div>
<h1>Sound and consequence</h1>
<p>The same Tiger, the same T-34, the same player view. Compare the machinery under load,
the own gun, and the incoming hit that ignites ammunition. Keep playback volume fixed.</p>
<h2>Buttoned cupola · 150 m duel</h2>
<div class="bar"><button id="before" aria-pressed="false">Before</button>
<button id="after" aria-pressed="true">After</button><span class="status" id="label">After · candidate</span></div>
<video id="duel" controls preload="metadata" src="AFTER.mp4" poster="AFTER.png"></video>
<div class="bar"><button data-time="0">Idle / incoming fire</button><button data-time="6">Own gun</button>
<button data-time="12">Engine load</button><button data-time="17">Penetration / ammunition</button></div>
<p class="note">This is a staged training encounter with a live enemy and normal projectile/damage outcomes.
The own shot at 7 s is explicitly staged. The incoming ammunition ignition is produced by the simulation.
Death fades to black and silence. The original 34 s recording had a recorder defect that counted another
scheduled shot after death; compare its first 20 s. The candidate ends at 20 s and blocks post-death input.</p>
<p>Candidate encounter: <strong>CHECKS</strong>. <a href="BEFORE_manifest.json">Original receipt</a> ·
<a href="AFTER_manifest.json">Candidate receipt</a></p>
FIRE
HAMMER
CUPOLA
<h2>What changed</h2>
<p>Combustion speeds up with RPM while the engine's structural frequencies stay fixed. Confirmed
internal explosions have broadband pressure and rupture, with steel ringing underneath. The source-level
and hearing laws remain; the limiter's recovery and reporting are corrected. Already-emitted explosion
sounds can now finish travelling during the death fade; gameplay stays frozen.</p>
<details><summary>Measurements and limits</summary>
<p>Digital LAF is a measurement of the reproduced waveform. It is not subjective approval or a measured
Tiger pressure history. Negative limiter values mean attenuation. Each calibration event is measured alone.</p>
<table><thead><tr><th>Calibration event</th><th>LAF dBFS(A), before → after</th><th>Peak limiter reduction dB, before → after</th></tr></thead><tbody>ROWS</tbody></table>
<p><a href="sound_mass_20260929_baseline_receipt.json">Before calibration</a> ·
<a href="AFTER_CALIBRATION">After calibration</a></p>
<p>The engine remains synthesized. Modal damping and blast timbre are declared estimates.
The direct impact retains the incoming shell's calibration. Queued rack/cookoff voices use fixed TNT estimates, not a solved inventory-dependent cabin blast.
The old limiter duration counter included tiny residual attenuation; use the new threshold counters.</p>
<pre>MASTER</pre></details>
<p><small>Private local evaluation. No third-party source library is packaged in this page.
Armor deformation, online fragment contacts, finite extinguisher charge and further atmospheric lighting
remain development work; these films do not claim those features.</small></p>
<script>
const player=document.getElementById('duel');let revision=0;
function choose(name){const n=++revision,t=Math.min(player.currentTime||0,19.9),playing=!player.paused;
 player.pause();const prefix=name==='before'?'BEFORE':'AFTER';player.poster=prefix+'.png';player.src=prefix+'.mp4';
 document.getElementById('before').setAttribute('aria-pressed',name==='before');
 document.getElementById('after').setAttribute('aria-pressed',name==='after');
 document.getElementById('label').textContent=name==='before'?'Before · d7947c0':'After · candidate';
 player.addEventListener('loadedmetadata',()=>{if(n!==revision)return;player.currentTime=t;if(playing)player.play().catch(()=>{});},{once:true});}
document.getElementById('before').onclick=()=>choose('before');document.getElementById('after').onclick=()=>choose('after');
document.querySelectorAll('[data-time]').forEach(b=>b.onclick=()=>{player.currentTime=Number(b.dataset.time);player.play().catch(()=>{});});
document.querySelectorAll('video').forEach(v=>v.addEventListener('play',()=>document.querySelectorAll('video').forEach(other=>{if(other!==v)other.pause();})));
player.addEventListener('timeupdate',()=>{if(player.currentTime>=20){player.pause();player.currentTime=19.99;}});
</script></html>'''
    fire = ('<h2>Fire and suppression · physical pickup and aimed foam</h2><p>Both start with the same '
            'declared surface ignition. A correct aim and wrong aim must produce different outcomes through '
            'deposited foam. Fire and spray do not yet have dedicated continuous sound sources.</p><div class="grid">'
            + ''.join(fire_cards) + '</div>') if fire_cards else ''
    for key, value in (("AFTER_CALIBRATION", after_cal_name), ("BEFORE", before), ("AFTER", after), ("CHECKS", "checks passed" if passed else "checks failed"),
                       ("ROWS", ''.join(rows)), ("MASTER", esc(master_text)), ("FIRE", fire), ("HAMMER", hammer),
                       ("CUPOLA", cupola)):
        page = page.replace(key, value)
    path = OUT / "immersion_review_20260929.html"
    path.write_text(page)
    print(path)


if __name__ == "__main__":
    build()
