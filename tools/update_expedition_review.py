"""Add verified local expedition evidence to the existing private review page.
No media is moved, uploaded, re-encoded, or represented as independent human testing.
"""
from pathlib import Path
from html import escape
import json,re

ROOT=Path(__file__).resolve().parents[1]
CAP=ROOT/'captures'
PAGE=CAP/'development_review_20261030.html'

def receipt(prefix):return json.loads((CAP/(prefix+'.json')).read_text(encoding='utf-8'))
def still(name,label):
    assert (CAP/name).is_file(),name
    return f'<figure><a class="enlarge" href="{escape(name)}"><img src="{escape(name)}" alt="{escape(label)}" loading="lazy"></a><figcaption>{escape(label)}</figcaption></figure>'
def card(key,title,category,body,limits,prefix,images,video=None):
    kind='video' if video else 'stills'
    media=''
    if video:
        assert (CAP/video).is_file()
        media=f'<video controls playsinline preload="none" poster="{images[0][0]}"><source src="{video}" type="video/mp4"></video><p class="openfile"><a href="{video}">Open clip separately</a></p>'
    return f'''<article class="card {kind}" id="{key}" data-category="{category}" data-kind="{kind}">
<div class="card-head"><span class="eyebrow">30 Oct / completion pass / recorded evidence</span><span class="badge {kind}">{'PLAYABLE CLIP' if video else 'RUNTIME REVIEW'}</span><h2>{escape(title)}</h2></div>
{media}<div class="copy"><p><strong>What passed</strong>{escape(body)}</p><p class="limits"><strong>Scope and remaining work</strong>{escape(limits)}</p></div>
<div class="stills">{''.join(still(*i) for i in images)}</div><details><summary>Exact checks and original files</summary><a href="{prefix}.json">Runtime receipt</a></details></article>'''

def main():
    world='expedition_world_20261030_b34a'
    recovery='recovery_gameplay_20261030_b31a'
    workshop='workshop_gameplay_20261030_b31a'
    theatre='expedition_theatre_20261030_b28_long'
    panel='crew_status_panel_20261030_b29a'
    w=receipt(world);rec=receipt(recovery);shop=receipt(workshop);trip=receipt(theatre)
    assert w['pass'] and w['status']=='passed' and all(c['pass'] for c in rec['checks']) and trip['pass']
    assert all(c.get('pass',c.get('passed',False)) for c in shop['checks'])
    cards=[
      card('world-save','A saved battlefield, restored exactly','world',
        'All 11 state owners match byte-for-byte after reconstruction. Twelve following simulation steps match an uninterrupted run. The actual frontend candidate constructor restores an exact second world while leaving the first unchanged. The open area map and a manual page saved midway through turning restore their exact ink and pose before play resumes. A new expedition starts on foot at camp without changing the roster, supplies or mission budget.',
        'This uses staged wounds, a turret fault, partial workshop work and a live projectile. It proves this build and its admitted sources; it is not a cross-version migration guarantee. Current audio-device sample offsets restart. Native menu clicks have not been independently human-tested.',world,
        [(world+'_before.png','Area map before saving'),(world+'_restored.png','Area map immediately restored'),(world+'_manual_before.png','Manual saved during a page turn'),(world+'_manual_restored.png','Same page and turn immediately restored')]),
      card('field-recovery','Transport, casualties and the recovery winch','vehicles',
        f"{len(rec['checks'])} checks passed: finite fuel transfer, spare-part loading, retained casualty loading, an ordinary 30 m drive and braking, exact transport-state restoration, casualty unloading, ramp deployment, source-hull cable attachment, rated pulling and remounting the same Tiger.",
        '45-second silent clip at normal cadence within labelled phases; explicit setup cuts reposition the test operator between actions. The wound and starting equipment are staged, with enemy fire suppressed. The portable anchored winch is an authored reconstruction, not a claim that this halftrack can tow a Tiger unaided. Replacement-seat boarding, transport-to-bed care and severe Tiger overhaul are separate work.',recovery,
        [(recovery+'_02_spare_loaded.png','A finite spare loaded into the transport'),(recovery+'_03_casualty_loaded.png','The same wounded person loaded aboard'),(recovery+'_04_winch_on_ground.png','The winch deployed down the ramp'),(recovery+'_05_winch_under_load.png','Rated winching consumes fuel')],recovery+'.mp4'),
      card('physical-workshop','Remove, retain, replace, secure and test','interior',
        f"{len(shop['checks'])} checks passed through actual source contacts: optic removal, retaining the old part, fitting a separate finite replacement, fastening and inspection. A battery work lamp is physically fitted, secured, tested and switched, with finite energy retained.",
        'This demonstrates the optical assembly loop, not a complete engine or transmission overhaul. Heavy source assemblies still need usable service paths. The cooling experiment is gated until its service loop is available.',workshop,
        [(workshop+'_removed.png','The optic removed from its actual mount'),(workshop+'_replacement.png','A different finite replacement fitted and verified'),(workshop+'_lamp_on.png','Installed battery work lamp on'),(workshop+'_lamp_off.png','The same installed lamp switched off')]),
      card('theatre-travel','Ten minutes on one retained battlefield','world',
        'A continuous 12 km theatre, 12 order sites, finite depot ammunition, physical map selection and 600 seconds of ordinary cruise/steering simulation. The Tiger travelled 754 m; enemy fire broke its left track and damaged the hull machine gun. Its disabled state persisted.',
        '20-second silent video: the first ten and last ten seconds of the run, at real-time cadence within each excerpt. The intervening 580 seconds are omitted. This was a scripted driver, not a human session or mission victory. The shown layout predates the latest road-clearance pass.',theatre,
        [(theatre+'_west_yard.png','The ammunition site'),(theatre+'_north_relief.png','Northern conditions')],theatre+'_travel_excerpts.mp4')]
    mapcard=card('map','Physical theatre and area maps','world',
        'Monochrome roads, source-building outlines, current position, mission selection, and a surveyed area sheet. The ordinary handheld torch illuminates the actual paper. Accepting an order spends one preparation opportunity; duplicate acceptance is rejected.',
        'The 480p review still needs larger labels and closer inspection to judge fine linework. The map is a physical interaction, but its live position marker is an intentional navigation aid.',theatre,
        [(theatre+'_physical_map.png','Theatre overview under the handheld light'),(theatre+'_area_map.png','The selected objective area')])
    text=PAGE.read_text(encoding='utf-8')
    # A still-image card and its internal thumbnail grid used the same class. Scope the
    # grid layout to the inner container so prose is not split into two narrow columns.
    text=text.replace('.stills{display:grid','.card > .stills{display:grid')
    text=text.replace('.stills{padding-left:17px','.card > .stills{padding-left:17px')
    text=re.sub(r'<!-- expedition-pass:start -->.*?<!-- expedition-pass:end -->','',text,flags=re.S)
    text=text.replace('<div class="grid">','<div class="grid"><!-- expedition-pass:start -->'+''.join(cards)+'<!-- expedition-pass:end -->',1)
    text=re.sub(r'<article\b[^>]*\bid="map".*?</article>',mapcard,text,flags=re.S)
    panel_receipt=receipt(panel)
    assert all(c['pass'] for c in panel_receipt['checks'])
    panelcard=card('panel','Physical crew lamps, pictograms and selection','crew',
        'The normal commander view shows five physical lamps at the lower left. Green means healthy, amber pulses for a wounded person, and red marks a dead occupant. The clip uses real LMB selection and a source-shadowed wire between the selected and hovered stations. Warm cabin illumination and the amber dial are visible in the wide view.',
        'Six seconds at normal cadence, silent. Wounds and death are explicitly staged through the actual casualty rules. This proves lamp state and selection, not the entire crew-transfer route. A separate gunner-eye dial close-up was obstructed and is not presented as a successful visual check.',panel,
        [(panel+'_night_healthy_wide.png','Ordinary commander view with warm cabin light'),(panel+'_night_healthy_close.png','Painted role pictograms and physical lamps'),(panel+'_night_mixed_on_close.png','Wounded amber and dead red states'),(panel+'_night_mixed_off_close.png','The wounded pulse in its dark phase')],panel+'_pulse_and_selection.mp4')
    text=re.sub(r'<article\b[^>]*\bid="panel".*?</article>',panelcard,text,flags=re.S)
    text=re.sub(r'<p class="pending-note">.*?</p>','<p class="pending-note">New footage: <a href="#panel" data-reveal="panel">physical crew lamps, pictograms and warm cabin lighting</a>. New expedition evidence appears first below.</p>',text,count=1,flags=re.S)
    total=len(re.findall(r'<article\b',text));videos=len(re.findall(r'<video\b',text));pending=len(re.findall(r'data-kind="pending"',text))
    text=re.sub(r'<div class="stats">.*?</div>',f'<div class="stats"><span><b>{videos}</b> playable clips</span><span><b>{total-videos-pending}</b> still-image reviews</span><span><b>{pending}</b> evidence gap</span></div>',text,count=1,flags=re.S)
    text=text.replace('Start with the four clips, then inspect the still-image reviews.','Start with the latest expedition evidence, then inspect the earlier clips and still-image reviews.')
    text=text.replace('the other three clips are silent','the remaining clips are silent')
    text=text.replace('Full recovery and transport, the workshop and upgrade loop, the larger theatre and mission progression, complete battlefield save/resume, and longer human playtests remain unfinished.',
        'Whole-world save/resume now has exact runtime proof. Transport, casualties, winching, source-part replacement and a ten-minute theatre simulation also have fresh evidence above. Severe Tiger overhaul, complete replacement-crew boarding, campaign-wide end-to-end verification and independent human playtests remain open.')
    text=text.replace('Whole-world save/resume now has exact runtime proof. Transport, casualties, winching, source-part replacement and a ten-minute theatre simulation also have fresh evidence above. Severe Tiger overhaul, complete replacement-crew boarding, campaign-wide end-to-end verification and independent human playtests remain open.',
        'The linked save/resume fixture passes, but expanded casualty testing exposed a retained-body restore bug now being corrected. Repeatable winch recovery works; carrier refueling and medical evacuation are still under integration test. Full heavy overhaul, replacement-crew boarding, linked campaign acceptance and independent human feedback remain open. Earlier clips below remain available with their original scope.')
    text=re.sub(r'\d+ feature reviews · newest useful evidence within each feature',f'{total} feature reviews · latest completion evidence first',text)
    PAGE.write_text(text,encoding='utf-8',newline='\n')
    print(json.dumps({'page':str(PAGE),'cards':total,'videos':videos,'pending':pending}))

if __name__=='__main__':main()
