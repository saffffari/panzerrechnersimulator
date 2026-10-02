#!/usr/bin/env python3
"""Build a self-contained local review page from explicitly selected capture receipts.

Does not run a game, alter media, select successful outcomes, or infer visual approval.
Paths stay relative to the generated page so the page and captures can travel together.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def source_path(receipt, value):
    path = Path(value)
    if path.is_absolute():
        return path
    # Native game receipts record paths relative to the checkout, not the receipt.
    return receipt.parent.parent / path


def inspect_video(path):
    result = subprocess.run(
        ["ffprobe", "-v", "error", "-count_frames", "-show_streams", "-show_format",
         "-of", "json", str(path)], check=True, capture_output=True, text=True)
    data = json.loads(result.stdout)
    video = next(s for s in data["streams"] if s["codec_type"] == "video")
    audio = next(s for s in data["streams"] if s["codec_type"] == "audio")
    subprocess.run(["ffmpeg", "-v", "error", "-xerror", "-nostdin", "-i", str(path),
                    "-f", "null", "-"], check=True, capture_output=True)
    return {"width": video["width"], "height": video["height"],
            "fps": video["avg_frame_rate"], "frames": int(video["nb_read_frames"]),
            "duration_s": float(data["format"]["duration"]),
            "audio_codec": audio["codec_name"], "sample_rate": int(audio["sample_rate"]),
            "full_decode_passed": True, "bytes": path.stat().st_size,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("receipts", type=Path, nargs="+")
    args = parser.parse_args()
    output = args.output.resolve()
    evidence = output.with_suffix(".json")
    if output.exists() or evidence.exists():
        parser.error("Choose a fresh output filename; existing reviews are preserved")
    rows = []
    for receipt in args.receipts:
        receipt = receipt.resolve()
        record = json.loads(receipt.read_text())
        if record.get("schema") != "panzer.combat-scene-case.v1" or not record.get("media_recorded"):
            parser.error(f"Not a completed combat capture receipt: {receipt}")
        path = source_path(receipt, record["video"])
        probe = inspect_video(path)
        assert [probe["width"], probe["height"]] == record["output_resolution"]
        assert probe["frames"] == round(record["duration_s"] * record["fps"])
        row = {"receipt": os.path.relpath(receipt, output.parent),
               "video": os.path.relpath(path, output.parent), "probe": probe,
               "scene": record["scene"], "passed": record["passed"],
               "checks": record["checks"], "tiger_shots": record["tiger_shots"],
               "enemy_shots": record["enemy_shots"], "incoming_hits": record["incoming_hits"],
               "outgoing_hits": record["outgoing_hits"],
               "listener_kia_at_s": record["listener_kia_at_s"],
               "maximum_travel_m": record["maximum_travel_m"],
               "renderer_changes": record.get("renderer_changes"),
               "pairs": [], "snapshots": []}
        pair = record.get("dither_video") or {}
        if pair.get("requested"):
            assert pair["same_simulation_and_audio"] and pair["same_hdr_eye_camera_and_particles"]
            for item in pair["outputs"]:
                paired_path = source_path(receipt, item["video"])
                paired_probe = inspect_video(paired_path)
                assert [paired_probe["width"], paired_probe["height"]] == item["native_resolution"]
                assert paired_probe["frames"] == item["frames"] == probe["frames"]
                row["pairs"].append({"pattern": item["pattern"],
                                     "video": os.path.relpath(paired_path, output.parent),
                                     "probe": paired_probe})
            for snapshot in pair["snapshots"]:
                row["snapshots"].append({"at_s": snapshot["at_s"], "paths": [
                    os.path.relpath(source_path(receipt, p), output.parent) for p in snapshot["paths"]]})
            row["pair_evidence"] = {k: pair[k] for k in ["frames", "frames_with_differences",
                "different_native_pixels_total", "same_simulation_and_audio",
                "same_hdr_eye_camera_and_particles", "scene_rendered_once_per_pair", "codec_limit"]}
        rows.append(row)
    payload = {"schema": "panzer.combat-review.v1", "cases": rows,
               "scope": "Old raster renderer with bounded audio/display fixes. Automated mortal crew in actual game maps; not a real-time performance test. Full video decoding checked; visual and listening judgments remain separate."}
    data = json.dumps(rows).replace("<", "\\u003c")
    page = TEMPLATE.replace("/*REVIEW_DATA*/[]", data)
    if all(r["scene"]["key"].startswith("winter_canyon") for r in rows):
        size = rows[0]["probe"]
        page = page.replace("Combat, weather & sound", "Winter · snow, particles & sound")
        page = page.replace("Actual game assets and maps, captured at 960 × 540. These films use the old raster renderer with the current sound mix and selected display fixes. The experimental visibility renderer is still under investigation.",
            f"The authored Canyon winter map, rendered at {size['width']} × {size['height']} with ordered dithering. Snow coverage and surviving vegetation come from the map. These clips use the old raster renderer with the current sound and VFX changes.")
        page = page.replace("No dither default has been changed.", "Ordered dithering is the selected look.")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(page)
    evidence.write_text(json.dumps(payload, indent=2) + "\n")
    print(json.dumps({"review": str(output), "evidence": str(evidence), "cases": len(rows),
                      "videos_decoded": sum(1 + len(r["pairs"]) for r in rows)}))


TEMPLATE = r'''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Panzerrechnersystem · Combat review</title>
<style>
:root{color-scheme:dark;font-family:system-ui,sans-serif;background:#101513;color:#e6eadc}*{box-sizing:border-box}
body{margin:0 auto;padding:40px 24px 80px;max-width:1400px}h1{font-size:clamp(28px,4vw,46px);font-weight:600;letter-spacing:-.04em;margin:12px 0}
h2{font-size:24px;font-weight:550;margin:0 0 10px}p{max-width:820px;line-height:1.6;color:#bbc5b9}.eyebrow{font-size:12px;letter-spacing:.16em;color:#cad998}
section{margin-top:40px;border-top:1px solid #3b463a;padding-top:30px}button,a,input{accent-color:#d4e99a}a{color:#d4e99a}button{font:inherit;background:#283326;border:1px solid #596450;color:#e8f2dc;border-radius:4px;padding:9px 14px;cursor:pointer}button[aria-pressed=true]{background:#d4e99a;color:#172010;border-color:#d4e99a}button:focus-visible,a:focus-visible,input:focus-visible{outline:3px solid #f2bd72;outline-offset:3px}
.controls{display:flex;align-items:center;flex-wrap:wrap;gap:10px;margin:16px 0}.controls input{min-width:140px;flex:1}.stage{position:relative;background:#020403;max-width:var(--native-width,960px);aspect-ratio:16/9;overflow:hidden}.stage video{position:absolute;width:100%;height:100%;object-fit:contain}.stage #blue{visibility:hidden}.stage.is-blue #blue{visibility:visible}.stage.zoom{max-width:none;width:var(--double-width,1920px);height:var(--double-height,1080px);aspect-ratio:auto}.scroll{overflow:auto;border:1px solid #3b463a;background:#050806}.pixel{image-rendering:pixelated}#badge{position:absolute;bottom:12px;left:12px;padding:6px 10px;background:#0c160de6;border-radius:3px;font-size:13px}.subtle{font-size:13px;color:#a3afa0}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,440px),1fr));gap:30px;margin-top:35px}.card video{image-rendering:pixelated;display:block;width:100%;background:#000;aspect-ratio:16/9}.card h3{font-size:21px;font-weight:500;margin-bottom:8px}.card p{font-size:14px}.tag{display:inline-block;border:1px solid #515c47;padding:4px 8px;border-radius:3px;margin:0 8px 6px 0;font-size:12px;color:#d3dbc6}details{margin:15px 0;font-size:13px;line-height:1.7;color:#aebbaa}summary{cursor:pointer}code{overflow-wrap:anywhere}.still{display:flex;gap:14px;flex-wrap:wrap}footer{margin-top:40px;font-size:13px;color:#99a594}output{font-variant-numeric:tabular-nums}
</style>
<div class="eyebrow">PANZERRECHNERSYSTEM / 01 OCT 2026</div><h1>Combat, weather & sound</h1>
<p>Actual game assets and maps, captured at 960 × 540. These films use the old raster renderer with the current sound mix and selected display fixes. The experimental visibility renderer is still under investigation.</p>
<div id="comparison"></div><div class="grid" id="scenes"></div>
<footer>Automated crew, live tank AI and ordinary damage. These are recorded simulation runs, not a real-time performance measurement. Source receipts and lossless dither stills are linked for inspection. No dither default has been changed.</footer>
<script>
'use strict';
const cases=/*REVIEW_DATA*/[];
const names={winter_canyon:'Winter canyon · cupola',winter_canyon_inside:'Winter canyon · inside the Tiger',distant_blizzard:'Blizzard · cupola',distant_blizzard_inside:'Blizzard · inside the Tiger',drive_day:'Daylight · tracks and suspension',urban_shadows:'City · cover and impacts',dusk_country:'Oderbruch · dusk',crew_machinery:'Halbe · crew machinery',night_firelight:'Night · firelight'};
const esc=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const pair=cases.find(c=>c.pairs.length===2);
function facts(c){const travel=c.maximum_travel_m>=1?` · ${c.maximum_travel_m.toFixed(1)} m travelled`:'';return `${c.tiger_shots} Tiger shots · ${c.enemy_shots} T-34 shots${travel}`;}
function evidence(c){const failed=Object.entries(c.checks).filter(([k,v])=>!v).map(([k])=>k);return `<details><summary>Capture evidence</summary><a href="${esc(c.receipt)}">Simulation receipt</a> · ${c.probe.frames} decoded frames · ${c.probe.width} × ${c.probe.height}. ${failed.length?'Missing observations: '+esc(failed.join(', ')):'All recorded observation checks passed.'}<br>${c.listener_kia_at_s===null?'Listener survived the recording.':`Listener killed at ${c.listener_kia_at_s.toFixed(2)} s; the ordinary hearing fade follows.`}<br>${esc(c.renderer_changes||'Baseline renderer')}</details>`;}
if(pair){const a=pair.pairs.find(p=>p.pattern==='bayer4'),b=pair.pairs.find(p=>p.pattern!=='bayer4');
const sphere=b.pattern==='sphere_bayer8';
const alternateLabel=sphere?'Obra Dinn-inspired · sphere Bayer 8':'Unordered · fixed blue noise';
const comparisonTitle=sphere?'Screen ordered vs camera-stable ordered':'Ordered or blue noise?';
const comparisonText=sphere?'Watch the slow head turn, then switch patterns on the same frame. The sphere pattern follows camera rotation and resolves four threshold samples per pixel. Colours, native scene resolution, exposure and sound stay matched. This is our implementation inspired by Obra Dinn, not its original shader.':'Switch patterns during the same blizzard firefight. Both use the identical scene frame, exposure and soundtrack. Pause and use 2× pixels to inspect the pattern.';
document.querySelector('#comparison').innerHTML=`<section><h2>${comparisonTitle}</h2><p>${comparisonText}</p><div class="controls"><button id="ordered" aria-pressed="true">Ordered · Bayer 4</button><button id="unordered" aria-pressed="false">${alternateLabel}</button><button id="zoom" aria-pressed="false">2× pixels</button></div><div class="scroll"><div class="stage pixel" id="stage" style="--native-width:${pair.probe.width}px;--double-width:${pair.probe.width*2}px;--double-height:${pair.probe.height*2}px"><video id="orderedVideo" preload="auto" playsinline src="${esc(a.video)}"></video><video id="blue" preload="auto" playsinline muted src="${esc(b.video)}"></video><span id="badge">Ordered · Bayer 4</span></div></div><div class="controls"><button id="play">Play</button><input id="seek" aria-label="Video position" type="range" min="0" max="${pair.probe.frames-1}" step="1" value="0"><output id="time">0.00 s</output><button id="sound" aria-pressed="true">Sound on</button></div><p class="subtle">${esc(facts(pair))}. Playback drift is corrected to the shared timeline; use the native PNGs for exact uncompressed pixel comparisons.</p><div class="still">${pair.snapshots.map(s=>s.paths.map((p,i)=>`<a href="${esc(p)}">${i?(sphere?'Sphere Bayer 8':'Blue noise'):'Screen Bayer 4'} at ${s.at_s.toFixed(1)} s</a>`).join(' ')).join(' ')}</div>${evidence(pair)}</section>`;
const master=document.querySelector('#orderedVideo'),secondary=document.querySelector('#blue'),play=document.querySelector('#play'),seek=document.querySelector('#seek'),stage=document.querySelector('#stage');let scrubbing=false;
const fps=pair.probe.frames/pair.probe.duration_s;
const sync=()=>{if(secondary.readyState>=1&&Math.abs(secondary.currentTime-master.currentTime)>.035)secondary.currentTime=master.currentTime;};
const update=()=>{if(!scrubbing)seek.value=Math.min(pair.probe.frames-1,Math.floor(master.currentTime*fps));document.querySelector('#time').value=`${master.currentTime.toFixed(2)} s`;sync();};
master.addEventListener('timeupdate',update);master.addEventListener('seeked',()=>{secondary.currentTime=master.currentTime;update();});
master.addEventListener('ended',()=>{secondary.pause();play.textContent='Replay';});
play.onclick=async()=>{if(!master.paused){master.pause();secondary.pause();play.textContent='Play';return;}document.querySelectorAll('.card video').forEach(v=>v.pause());if(master.ended){master.currentTime=0;secondary.currentTime=0;}sync();try{await Promise.all([master.play(),secondary.play()]);play.textContent='Pause';}catch(e){master.pause();secondary.pause();play.textContent='Play';document.querySelector('#time').value='Playback unavailable; open a video link below.';}};
seek.oninput=()=>{scrubbing=true;master.currentTime=Number(seek.value)/fps;secondary.currentTime=Number(seek.value)/fps;document.querySelector('#time').value=`${(Number(seek.value)/fps).toFixed(2)} s`;};seek.onchange=()=>{scrubbing=false;};
function pattern(blue){sync();stage.classList.toggle('is-blue',blue);document.querySelector('#ordered').setAttribute('aria-pressed',String(!blue));document.querySelector('#unordered').setAttribute('aria-pressed',String(blue));document.querySelector('#badge').textContent=blue?alternateLabel:'Ordered · Bayer 4';}
document.querySelector('#ordered').onclick=()=>pattern(false);document.querySelector('#unordered').onclick=()=>pattern(true);
document.querySelector('#zoom').onclick=e=>{const enabled=stage.classList.toggle('zoom');e.target.setAttribute('aria-pressed',String(enabled));};
document.querySelector('#sound').onclick=e=>{master.muted=!master.muted;e.target.textContent=master.muted?'Sound off':'Sound on';e.target.setAttribute('aria-pressed',String(!master.muted));};
document.querySelector('#comparison').insertAdjacentHTML('beforeend',`<p class="subtle">Direct videos: <a href="${esc(a.video)}">Bayer 4</a> · <a href="${esc(b.video)}">${alternateLabel}</a></p>`);
}
for(const c of cases.filter(c=>c!==pair)){const card=document.createElement('article');card.className='card';card.innerHTML=`<h3>${esc(names[c.scene.key]||c.scene.key)}</h3><p>${esc(c.scene.purpose)}</p><video controls preload="metadata" playsinline src="${esc(c.video)}"></video><p>${esc(facts(c))}</p><span class="tag">${c.probe.width} × ${c.probe.height}</span><span class="tag">${c.probe.fps==='30/1'?'30 fps':esc(c.probe.fps)+' fps'}</span><a href="${esc(c.video)}">Open video</a>${evidence(c)}`;document.querySelector('#scenes').append(card);}
document.addEventListener('play',event=>{if(event.target.closest('.card')){document.querySelectorAll('video').forEach(v=>{if(v!==event.target)v.pause();});const button=document.querySelector('#play');if(button)button.textContent='Play';}},true);
</script></html>'''


if __name__ == "__main__":
    main()
