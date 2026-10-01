#!/usr/bin/env python3
"""Draw an editable, north-up field map from the actual Gaea runtime terrain and placements.
This is measured cartography, not AI terrain invention. No randomized enemy intelligence is drawn.
"""
import argparse, hashlib, json, math
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Polygon, Circle

ROOT=Path(__file__).resolve().parents[2]
DEFAULT=ROOT/'assets/worldgen/workshops/gaea-canyon-run'

def draw(source, output):
    edits=json.loads((source/'manifest_edits.json').read_text())
    manifest=json.loads((source/'manifest.v1.json').read_text())
    height=np.load(source/'heights.npy').astype(float)
    course=edits['canyon_course'];route=np.asarray(course['route'])
    # Keep identical world scale in X and Z, with room around the whole playable route.
    bounds=[-1150.0,2000.0,-920.0,1120.0]
    xmin,xmax,zmin,zmax=bounds
    plt.rcParams.update({'svg.fonttype':'path','font.family':'DejaVu Sans','path.simplify':True,'path.simplify_threshold':0.15})
    fig=plt.figure(figsize=(16,9.6),facecolor='#d7c9a6')
    ax=fig.add_axes([.035,.045,.93,.91],facecolor='#ddd0ae')
    ax.set_xlim(xmin,xmax);ax.set_ylim(zmax,zmin);ax.set_aspect('equal');ax.axis('off')
    x=np.linspace(-2500,2500,len(height));datum=json.loads((source/'canyon-course.receipt.json').read_text())['runtime_datum_m']
    H=height+datum
    # Subtle bands and precise survey contours: 20 m interval, 100 m index lines.
    ax.contourf(x,x,H,levels=np.arange(0,481,20),cmap=matplotlib.colors.LinearSegmentedColormap.from_list('survey',['#e5d9b8','#d4c29c','#ad9777']),alpha=.34,zorder=0)
    small=ax.contour(x,x,H,levels=np.arange(0,481,20),colors='#8c7251',linewidths=.38,alpha=.60,zorder=2)
    major=ax.contour(x,x,H,levels=np.arange(0,501,100),colors='#715b40',linewidths=.78,alpha=.80,zorder=3)
    ax.clabel(major,inline=True,fontsize=6,fmt='%d',colors='#715b40',inline_spacing=7)
    for v in range(-1000,2001,500):ax.plot([v,v],[zmin,zmax],color='#7a8171',alpha=.21,lw=.45,zorder=1)
    for v in range(-500,1001,500):ax.plot([xmin,xmax],[v,v],color='#7a8171',alpha=.21,lw=.45,zorder=1)
    # A reconnaissance pencil course, following the actual traversable route, not a road claimed
    # to exist. Parallel wheel traces are deliberately omitted: the ground is a snow corridor.
    ax.plot(route[:,0],route[:,1],color='#e7ddc3',lw=3.8,alpha=.85,zorder=4)
    ax.plot(route[:,0],route[:,1],color='#994c39',lw=1.15,linestyle=(0,(8,5)),alpha=.9,zorder=6)
    for b in edits['building_instances']:
        a=math.radians(b['yaw_deg']);c,s=math.cos(a),math.sin(a)
        pts=np.array([[-1,-1],[1,-1],[1,1],[-1,1]])*[b['width_m']/2,b['depth_m']/2]
        # Engine yaw rotates world +Z toward +X.
        pts=pts@np.array([[c,-s],[s,c]])+np.array([b['x'],b['z']])
        ax.add_patch(Polygon(pts,closed=True,facecolor='#554b3d',edgecolor='#3e382f',lw=.30,zorder=5))
    # Sparse woodland marks come only from placed trees (asset names in this manifest).
    assets=manifest['assets']
    for p in manifest['placements']:
        asset=assets[p[0]]
        name=str(asset).lower()
        if not any(v in name for v in ('tree','pine','fir','birch')):continue
        px,pz=p[1],p[3]
        if xmin<px<xmax and zmin<pz<zmax:
            ax.plot([px-6,px,px+6],[pz+5,pz-8,pz+5],color='#69745b',lw=.48,zorder=4)
            ax.plot([px,px],[pz-7,pz+10],color='#69745b',lw=.4,zorder=4)
    # Paper handling, kept restrained so the ink remains readable.
    for f in (1/3,2/3):
        v=xmin+(xmax-xmin)*f
        ax.axvline(v,color='#fff2d1',lw=2.5,alpha=.24,zorder=7)
        ax.axvline(v+3,color='#67563e',lw=.65,alpha=.13,zorder=7)
    v=(zmin+zmax)/2
    ax.axhline(v,color='#fff2d1',lw=2.0,alpha=.24,zorder=7)
    ax.axhline(v+3,color='#67563e',lw=.6,alpha=.13,zorder=7)
    # Four-point north rose: north is -Z, exactly matching the game compass.
    cx,cz=1835,-700
    for dx,dz in [(0,-125),(0,72),(-62,0),(62,0)]:
        side=np.array([-dz,dx]);side=side/np.linalg.norm(side)*10
        ax.add_patch(Polygon(np.array([[cx,cz],[cx+dx,cz+dz],[cx+side[0],cz+side[1]]]),
            fc='#514a3d',ec='#514a3d',lw=.5,zorder=8))
    ax.add_patch(Circle((cx,cz),21,fill=False,edgecolor='#514a3d',lw=.65,zorder=8))
    # Endpoints copied from the actual course, not from guessed route bounds.
    start=np.array([course['start']['x'],course['start']['z']]);goal=np.array([course['goal']['x'],course['goal']['z']])
    ax.add_patch(Circle(start,27,fill=False,edgecolor='#994c39',lw=1.15,zorder=8))
    ax.plot([goal[0]-24,goal[0]+24],[goal[1]-24,goal[1]+24],color='#994c39',lw=1.3,zorder=8)
    ax.plot([goal[0]-24,goal[0]+24],[goal[1]+24,goal[1]-24],color='#994c39',lw=1.3,zorder=8)
    x0,z0=1350,960
    for j in range(5):
        ax.add_patch(Polygon([[x0+j*100,z0],[x0+(j+1)*100,z0],[x0+(j+1)*100,z0+10],[x0+j*100,z0+10]],
            fc='#514a3d' if j%2==0 else '#e4d8b7',ec='#514a3d',lw=.5,zorder=8))
    for dx,label in [(0,'0'),(500,'500')]:ax.text(x0+dx,z0+48,label,ha='center',va='center',fontsize=8,color='#514a3d')
    # Border uses geographic coordinates; an inset margin leaves the compass clear.
    ax.add_patch(Polygon([[xmin,zmin],[xmax,zmin],[xmax,zmax],[xmin,zmax]],fill=False,ec='#756348',lw=1.0,zorder=8))
    output.parent.mkdir(parents=True,exist_ok=True)
    svg=output.with_suffix('.svg')
    fig.savefig(svg,facecolor=fig.get_facecolor())
    # Matplotlib leaves spaces before path newlines; keep the editable source whitespace-clean.
    svg.write_text('\n'.join(line.rstrip() for line in svg.read_text().splitlines())+'\n')
    fig.savefig(output.with_suffix('.png'),dpi=150,facecolor=fig.get_facecolor())
    # Runtime samples this exact finished artwork; annotations remain physical-paper UVs.
    metadata={'schema':'panzerrechnersystem.canyon-paper-map','version':1,'bounds_xz':bounds,
        'route':course['route'],'image':str(output.with_suffix('.png').relative_to(ROOT)),
        'source_manifest':str(source/'manifest.v1.json'),'height_sha256':hashlib.sha256((source/'heights.npy').read_bytes()).hexdigest(),
        'contour_interval_m':20,'index_contour_interval_m':100,'north':'-Z',
        'scope':'Gaea mechanics-course survey; not a historical map. Red pencil is the planned course. No enemy locations or live player marker.',
        'method':'Deterministic vector cartography from runtime terrain and placed buildings; no AI image generation.'}
    output.with_suffix('.json').write_text(json.dumps(metadata,indent=2)+'\n')
    plt.close(fig)
    print(output.with_suffix('.png'))

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--source',type=Path,default=DEFAULT)
    p.add_argument('--output',type=Path,default=ROOT/'catalog/maps/canyon_survey_20260930')
    a=p.parse_args();draw(a.source,a.output)
