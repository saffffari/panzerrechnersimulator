#!/usr/bin/env python3
"""Build a local native-pixel review from explicit, receipt-checked media entries."""
import argparse,html,json
from pathlib import Path

def main():
    parser=argparse.ArgumentParser();parser.add_argument("manifest",type=Path);parser.add_argument("output",type=Path)
    args=parser.parse_args();data=json.loads(args.manifest.read_text());cards=[]
    for item in data["clips"]:
        movie=Path(item["video"]);poster=Path(item["poster"])
        assert movie.is_file() and poster.is_file(),item["title"]
        assert movie.parent.resolve()==poster.parent.resolve()==args.output.parent.resolve(),"serve clips and posters beside this page"
        receipt=Path(item["receipt"]);assert receipt.is_file()
        assert receipt.parent.resolve()==args.output.parent.resolve(),"serve receipts beside this page"
        cards.append(f'<article><h2>{html.escape(item["title"])}</h2><p>{html.escape(item["description"])}</p>'
            f'<video controls preload="metadata" playsinline poster="{html.escape(poster.name)}" src="{html.escape(movie.name)}"></video>'
            f'<p class="evidence"><a href="{html.escape(receipt.name)}">Capture receipt</a> · <a href="{html.escape(poster.name)}">Native still</a> · {html.escape(item["scope"])}</p></article>')
    page='<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Winter playtest — release review</title><style>\nbody{margin:0;background:#111416;color:#e9e9e4;font:16px/1.55 system-ui,sans-serif}main{max-width:1660px;margin:auto;padding:30px}h1{font-size:28px;font-weight:600;margin-bottom:6px}p{max-width:900px;color:#bac0bf}h2{font-size:20px;margin:0 0 8px}article{padding:24px 0;border-top:1px solid #353b3c}video{display:block;width:800px;height:450px;max-width:100%;object-fit:contain;image-rendering:pixelated;background:#000}a{color:#e3b987}.evidence{font-size:13px}button{background:#282e30;color:white;border:1px solid #4a5154;padding:7px 12px;margin:4px 5px 16px 0;cursor:pointer}.note{padding:12px 16px;border-left:3px solid #c99955;background:#1b2022}video.large{width:1600px;height:900px;max-width:none}@media(max-width:850px){main{padding:16px}video{height:auto}}\n</style><main><h1>Winter playtest</h1><p>{{summary}}</p><p class="note">Native 800 × 450 captures. Play with sound. These are deterministic simulation recordings; their 30 fps encoding is not a live performance measurement.</p><button id="scale">Toggle native / 2× pixels</button>{{cards}}</main><script>document.querySelector(\'#scale\').onclick=()=>document.querySelectorAll(\'video\').forEach(v=>v.classList.toggle(\'large\'));document.querySelectorAll(\'video\').forEach(v=>v.addEventListener(\'play\',()=>document.querySelectorAll(\'video\').forEach(other=>{if(other!==v)other.pause()})));</script></html>'
    page=page.replace("{{summary}}",html.escape(data["summary"])).replace("{{cards}}","".join(cards))
    args.output.write_text(page)
if __name__=="__main__":main()
