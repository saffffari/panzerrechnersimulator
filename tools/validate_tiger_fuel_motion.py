"""CPU-only full triangle translation-sweep admission for the private filler source.
Mirrors workshop_sweep convex-hull SAT and explicit fitted-plane contact; no GPU or build.
The native Rust test remains required because its loader/transforms are runtime authority.
"""
from pathlib import Path
import sys,json,itertools,time,numpy as np
ROOT=Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'tools'))
from fit_crew_steel_helmet import read_glb
from prepare_tiger_engine_service import source_data
from prepare_tiger_cooling import read_output
EPS=float(32*np.finfo(np.float32).eps*10)

def path(i,f):
 lift,park=(.15,np.array([1.42-1.14618853,2.0380541-2.0107101,-1.60+1.34911114])) if i==0 else(.25,np.array([1.25-1.14813453,2.0380541-1.934,-1.80+1.33648619]))
 if f<=.3:return np.array([0,lift*f/.3,0],dtype=np.float32)
 if f<=.75:return np.array([park[0]*(f-.3)/.45,lift,park[2]*(f-.3)/.45],dtype=np.float32)
 return np.array([park[0],lift+(park[1]-lift)*(f-.75)/.25,park[2]],dtype=np.float32)

def fitted(a,b,t):
 e=t[:,1]-t[:,0];f=t[:,2]-t[:,0];n=np.cross(e,f);nn=np.linalg.norm(n,axis=1);n/=np.maximum(nn[:,None],1e-30)
 aa=np.einsum('bij,bj->bi',a-t[:,None,0],n);bb=np.einsum('bij,bj->bi',b-t[:,None,0],n)
 delta=b[:,0]-a[:,0];length=np.linalg.norm(delta,axis=1);axis=delta/np.maximum(length[:,None],1e-30)
 extent=np.einsum('bij,bj->bi',t,axis).max(1)-np.einsum('bij,bj->bi',t,axis).min(1)
 departs=(length>1e-12)&((extent>2*EPS)|(np.einsum('bi,bi->b',delta,n)>=-length*(64*np.finfo(np.float32).eps)))&(np.einsum('bij,bj->bi',a,axis).min(1)>=np.einsum('bij,bj->bi',t,axis).max(1)-EPS)
 return departs|((nn>1e-14)&np.any(abs(aa)<=EPS,axis=1)&np.all(aa>=-EPS,axis=1)&np.all(bb>=-EPS,axis=1)&np.all(bb>=aa-EPS,axis=1))

def blocked(a,b,t):
 # Translate in double precision, matching runtime SAT's local subtraction.
 origin=a[:,0:1].copy();h=np.concatenate([a,b],axis=1)-origin;t=t-origin
 normal=np.cross(t[:,1]-t[:,0],t[:,2]-t[:,0]);axes=[np.broadcast_to(x,normal.shape)for x in np.eye(3)]+[normal]
 for i in range(3):axes.append(np.cross(normal,t[:,(i+1)%3]-t[:,i]))
 for x,y in itertools.combinations(range(6),2):
  edge=h[:,y]-h[:,x];axes.append(np.cross(normal,edge))
  for z in range(y+1,6):axes.append(np.cross(edge,h[:,z]-h[:,x]))
  for i in range(3):axes.append(np.cross(edge,t[:,(i+1)%3]-t[:,i]))
 axes=np.stack(axes,axis=1);length=np.linalg.norm(axes,axis=2);hh=np.einsum('baj,bvj->bav',axes,h);tt=np.einsum('baj,bvj->bav',axes,t)
 margin=(EPS+64*np.finfo(float).eps)*length
 separated=(length>=1e-14)&((hh.max(2)+margin<tt.min(2))|(tt.max(2)+margin<hh.min(2)))
 return ~np.any(separated,axis=1)

def main():
 start=time.time();D,B,_=read_glb(str(ROOT/'assets/tanks/tiger/baked/tiger_hull.glb'));fixed=[];roots=[]
 for n in D['nodes']:
  if 'mesh'not in n or n.get('name') in ['Hull_0875','Hull_0902','Hull_0708']:continue
  d,_=source_data(D,B,n['name']);a=d['vertices'][d['triangles'],:3].astype(np.float32);lo=a.min(1);hi=a.max(1)
  use=np.all(lo<np.array([1.8,2.6,-.9]),axis=1)&np.all(hi>np.array([.9,1.8,-2.0]),axis=1)
  fixed.extend(a[use]);roots.extend([n['name']]*int(use.sum()))
 tank=read_output(ROOT/'assets/tanks/tiger/workshop_fuel/left_tank_open_neck.glb');fixed.extend(tank['vertices'][tank['triangles'],:3].astype(np.float32));roots.extend(['derived_tank']*len(tank['triangles']))
 deck=read_output(ROOT/'assets/tanks/tiger/workshop_fuel/deck_filler_clearance_v3.glb');fixed.extend(deck['vertices'][deck['triangles'],:3].astype(np.float32));roots.extend(['derived_deck']*len(deck['triangles']))
 fixed=np.array(fixed);local=(fixed.min(1)<[1.70,2.6,-1.0]).all(1)&(fixed.max(1)>[.95,1.8,-2.0]).all(1);fixed=fixed[local];roots=[name for name,keep in zip(roots,local)if keep];caps=[]
 for name in ['left_outer_plug','left_inner_cap']:
  d=read_output(ROOT/('assets/tanks/tiger/workshop_fuel/'+name+'.glb'));caps.append(d['vertices'][d['triangles'],:3].astype(np.float32))
 state=[0.,0.];report=dict(schema='tiger-fuel-motion-source.v1',source_error_m=EPS,status='running',checks=[],first_block=None)
 for index,opening in [(0,True),(1,True),(1,False),(0,False)]:
  for step in range(1,51):
   next=step/50 if opening else 1-step/50;before=state[index];times=[before]+[x for x in ([.3,.75]if opening else[.75,.3])if min(before,next)<x<max(before,next)]+[next]
   other=1-index;obstacles=np.concatenate([fixed,caps[other]+path(other,state[other])]);omin=obstacles.min(1);omax=obstacles.max(1)
   for f,g in zip(times,times[1:]):
    aa=caps[index]+path(index,f);bb=caps[index]+path(index,g);lo=np.minimum(aa.min(1),bb.min(1))-EPS;hi=np.maximum(aa.max(1),bb.max(1))+EPS
    candidate=np.all(lo[:,None]<=omax[None],axis=2)&np.all(hi[:,None]>=omin[None],axis=2);mi,oi=np.where(candidate)
    if not len(mi):continue
    a,b,t=aa[mi].astype(float),bb[mi].astype(float),obstacles[oi].astype(float);bad=blocked(a,b,t)&~fitted(a,b,t)
    if next in(0.,1.):bad&=~fitted(b,a,t)
    if np.any(bad):
     j=int(np.where(bad)[0][0]);report['first_block']=dict(cap=index,opening=opening,step=step,from_fraction=before,to_fraction=next,triangle=int(mi[j]),obstacle=int(oi[j]),root=roots[oi[j]]if oi[j]<len(roots)else'other_cap',start=a[j].tolist(),end=b[j].tolist(),obstacle_vertices=t[j].tolist());report['status']='failed';break
   if report['first_block']:break
   state[index]=next
  report['checks'].append(dict(cap=index,opening=opening,reached_fraction=state[index],pass_=report['first_block']is None))
  if report['first_block']:break
 report['elapsed_seconds']=time.time()-start;report['status']='passed'if report['first_block']is None else'failed'
 out=ROOT/'artifacts/tiger-fuel-source-20261030/full-face-motion.json';out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report));return 0 if report['status']=='passed'else 1
if __name__=='__main__':raise SystemExit(main())
