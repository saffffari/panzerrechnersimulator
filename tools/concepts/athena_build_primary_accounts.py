"""Original editable voxel concepts from Pozdnyakova's 14 March 1943 testimony.
Run in a separate Blender process, never the user's interactive scene.
All geometry, materials, .vox files and GLBs are generated locally from this source.
"""
import bpy, math, random, json, struct, hashlib, sys
from pathlib import Path
from mathutils import Vector
ROOT=Path('/home/alex/panzer/voxel-runtime')
OUT=ROOT/'assets/concepts/primary-accounts-20261107/athena'
CAP=Path('/windows/shop/panzerrechnersystem/captures/screenshots')
UNIT=.1
PALETTE=[
 ('earth',(65,58,51)),('earth_edge',(43,39,35)),('dust',(130,116,95)),('brick',(131,69,49)),
 ('brick_light',(169,99,68)),('brick_dark',(96,52,43)),('mortar',(94,87,73)),('plaster',(186,173,145)),
 ('soot',(27,29,28)),('char',(49,43,35)),('wood',(115,84,51)),('ash',(131,130,119)),
 ('iron',(59,68,68)),('rust',(130,74,42)),('tank_teal',(68,99,96)),('tank_light',(94,120,111)),
 ('ceramic',(218,209,180)),('cloth',(113,112,93)),('cloth_light',(174,164,133)),('water',(57,76,77)),
 ('oil',(35,43,42)),('oil_copper',(91,75,54)),('oil_blue',(52,68,77)),('skin',(159,130,102)),
 ('coat',(56,62,57)),('bucket',(142,151,140)),('brass',(168,126,57)),('red_cloth',(113,48,38)),
]
IDX={k:i for i,(k,c)in enumerate(PALETTE)}
class Vox:
 def __init__(self,name):self.name=name;self.v={};self.parts=[];self.part='base';self.rng=random.Random(19430917)
 def group(self,n):self.part=n;self.parts.append(n)if n not in self.parts else None
 def put(self,x,y,z,c):
  if x>=0 and y>=0 and z>=0:self.v[(x,y,z)]=(IDX[c]if isinstance(c,str)else c,self.part)
 def box(self,x0,y0,z0,x1,y1,z1,c):
  for x in range(round(x0*10),round(x1*10)):
   for y in range(round(y0*10),round(y1*10)):
    for z in range(round(z0*10),round(z1*10)):self.put(x,y,z,c)
 def cut(self,x0,y0,z0,x1,y1,z1):
  for x in range(round(x0*10),round(x1*10)):
   for y in range(round(y0*10),round(y1*10)):
    for z in range(round(z0*10),round(z1*10)):self.v.pop((x,y,z),None)
 def ball(self,p,r,c,scale=(1,1,1)):
  for x in range(math.floor((p[0]-r*scale[0])*10),math.ceil((p[0]+r*scale[0])*10)):
   for y in range(math.floor((p[1]-r*scale[1])*10),math.ceil((p[1]+r*scale[1])*10)):
    for z in range(max(0,math.floor((p[2]-r*scale[2])*10)),math.ceil((p[2]+r*scale[2])*10)):
     if sum((((q+.5)/10-p[i])/scale[i])**2 for i,q in enumerate([x,y,z]))<r*r:self.put(x,y,z,c)
 def beam(self,a,b,r,c):
  dist=math.dist(a,b);n=max(1,int(dist*20))
  for i in range(n+1):self.ball(tuple(a[k]+(b[k]-a[k])*i/n for k in range(3)),r,c)
 def brickwall(self,x0,y0,z0,x1,y1,z1):
  self.box(x0,y0,z0,x1,y1,z1,'mortar')
  for x in range(round(x0*10),round(x1*10)):
   for y in range(round(y0*10),round(y1*10)):
    for z in range(round(z0*10),round(z1*10)):
     course=(z-round(z0*10))//3
     tangent=y if x1-x0<y1-y0 else x
     if z%3!=0 or (tangent+course*2)%5!=0:
      c=['brick','brick','brick_dark','brick_light'][(tangent//5+course*3)%4];self.put(x,y,z,c)
 def rubble(self,box,n):
  for _ in range(n):
   x=self.rng.uniform(box[0],box[3]);y=self.rng.uniform(box[1],box[4]);z=box[2]
   self.ball((x,y,z),self.rng.uniform(.09,.25),self.rng.choice(['brick','brick_light','ash','char']),scale=(1.4,.8,.55))
 def bucket(self,p,fallen=False):
  x,y,z=p;self.box(x-.2,y-.2,z,x+.2,y+.2,z+.4,'bucket');self.cut(x-.1,y-.1,z+.1,x+.1,y+.1,z+.4)
  self.beam((x-.2,y,z+.3),(x-.2,y,z+.6),.05,'iron');self.beam((x+.2,y,z+.3),(x+.2,y,z+.6),.05,'iron');self.beam((x-.2,y,z+.6),(x+.2,y,z+.6),.05,'iron')
  if fallen:self.cut(x-.2,y-.2,z+.1,x+.2,y,z+.6)
 def stool(self,x,y,z,small=False):
  w=.35 if small else .5;h=.45 if small else .7
  self.box(x,y,z+h,x+w,y+w,z+h+.1,'wood')
  for xx in [x,x+w-.1]:
   for yy in [y,y+w-.1]:self.box(xx,yy,z,xx+.1,yy+.1,z+h,'char')

def house():
 v=Vox('the-window-solnechnaya');v.group('01_earth_and_foundation');v.box(0,0,0,12,10,1.8,'earth');v.box(0,0,0,12,10,.2,'earth_edge')
 v.cut(1.8,3,.3,10.2,8.1,1.8);v.box(1.8,3,.2,10.2,8.1,.4,'dust')
 v.group('02_cellar_retaining_walls');v.brickwall(1.8,3,.4,2.2,8.1,2.0);v.brickwall(9.8,3,.4,10.2,8.1,2);v.brickwall(2.2,7.8,.4,9.8,8.1,2)
 v.group('03_burned_two_storey_shell');v.brickwall(2,7.5,1.8,10,7.9,7.3);v.brickwall(9.7,3.4,1.8,10.1,7.8,6.6)
 # Two levels of open window voids; geometry is an interpretation, not a measured house.
 for x in [2.9,5.4,7.8]:
  for z in [2.8,5.3]:v.cut(x,7.4,z,x+1.1,8,z+1.3)
 for y in [4.2,6.0]:v.cut(9.6,y,3,10.2,y+1,4.2)
 for x in range(20,100):
  top=66+int(6*math.sin(x*.33))+v.rng.randrange(0,4)
  v.cut(x/10,7.4,top/10,(x+1)/10,8,7.6)
 # Soot follows the open upper walls and window lintels.
 for (x,y,z),(c,p) in list(v.v.items()):
  if p=='03_burned_two_storey_shell' and z>34+7*math.sin(x*.13+y*.08):
   scar=math.sin(x*.19+y*.11)+math.sin(z*.23+x*.08)
   if scar>-.9:v.v[(x,y,z)]=(IDX['soot']if scar>-.2 else IDX['char'],p)
 v.group('04_escape_window_and_closed_door');v.brickwall(2,3,1.8,5.0,3.4,4.6);v.cut(2.8,2.9,2.6,4.1,3.5,4.0)
 for x in [2.7,4.1]:v.box(x,3,2.5,x+.1,3.4,4.1,'char')
 v.box(2.7,3,4.0,4.2,3.4,4.1,'char');v.box(2.6,2.8,2.5,4.3,3.5,2.6,'ash')
 v.box(4.1,3.1,1.9,4.9,3.2,3.8,'char');v.box(4.75,3.0,2.7,4.85,3.1,2.8,'iron')
 # The draped strip suggests hurried passage; no person or exact pose is invented.
 for z in range(21,29):v.box(3.6,2.9,z/10,3.9,3.0,(z+1)/10,'cloth_light')
 v.group('05_charred_floor_and_fallen_joists')
 for x in [2.4,4.7,8.5,9.5]:v.beam((x,3.3,2.0),(x,7.7,2.0),.1,'char')
 for x in [2.4,4.7,7.1,9.5]:v.beam((x,5.0,4.5),(x,7.7,4.5),.12,'char')
 for y in [6.1,6.4,6.7,7,7.3]:v.box(7.5,y,4.5,9.7,y+.2,4.6,'wood')
 v.beam((4.5,4.3,.6),(8.8,6.3,4.5),.15,'char');v.beam((3.2,4.5,.5),(5.2,7.6,4.6),.11,'char')
 v.group('06_cellar_bedding_and_supper');v.box(2.4,6.5,.4,4.2,7.2,.6,'cloth');v.box(2.5,6.7,.6,2.9,7.1,.8,'cloth_light')
 v.box(6.6,6.7,1.3,8.2,7.4,1.4,'char')
 for x in [6.7,7.9]:v.box(x,6.8,.4,x+.1,7.3,1.3,'char')
 v.box(6.8,6.8,1.4,7.3,7.2,1.5,'ceramic');v.box(7.5,6.9,1.4,7.7,7.1,1.6,'ceramic')
 v.stool(7.9,5.7,.4,True);v.stool(6.7,5.7,.4)
 v.box(4.9,6.8,.4,5.4,7.2,1.1,'iron');v.beam((5.2,7,1.1),(5.2,7,2.2),.09,'iron')
 v.group('07_courtyard_trench_and_absence');v.cut(1.0,.6,.6,8.4,1.9,1.9);v.cut(7.5,1.5,.6,8.4,3.2,1.9)
 v.box(1.1,.7,.6,4.3,1.7,.7,'wood');v.box(1.4,.9,.7,2.9,1.5,.8,'cloth');v.box(3.3,1.1,.7,3.7,1.4,1.0,'red_cloth')
 v.box(4.7,.8,.6,5.0,1.2,.8,'char');v.box(5.2,.8,.6,5.5,1.2,.8,'char')
 v.ball((7.4,1.8,1.25),1.05,'earth',scale=(1.1,1,.7));v.rubble((1,0,1.85,11.6,9.5,1.85),130)
 v.group('08_cellar_scattered_bricks');v.rubble((2.4,3.6,.48,9.4,7.3,.48),75)
 return v

def cistern():
 v=Vox('water-under-fire');v.group('01_exposed_ground');v.box(0,0,0,14,11,.4,'earth');v.box(0,0,0,14,11,.1,'earth_edge')
 # Buried railway ties and rails run behind an exposed collection path.
 v.group('02_rail_and_underframe')
 for x in range(5,140,7):v.box(x/10,6.8,.4,(x+2)/10,9.2,.6,'wood')
 for y in [7.1,8.8]:v.box(0,y,.6,14,y+.1,.8,'iron')
 v.box(2.1,7,.95,11.9,9,1.2,'iron')
 for x in [3.2,4.2,9.8,10.8]:
  v.beam((x,7.05,.85),(x,9,.85),.09,'iron')
  for y in [7.05,8.95]:v.ball((x,y,.85),.5,'iron',scale=(1,.28,1));v.ball((x,y-.07,.85),.19,'rust',scale=(1,.3,1))
 v.group('03_cistern_shell')
 # 7.8m cylindrical barrel, hollow above the retained internal waterline.
 for x in range(29,112):
  for y in range(68,93):
   for z in range(12,38):
    rr=math.hypot((y+.5)/10-8.,(z+.5)/10-2.5)
    if rr<1.22 and (rr>1.04 or x<31 or x>109):
     c='tank_teal'
     if z<19 or (x*17+y*7+z*3)%53<5:c='rust'
     elif (x//5+z//4)%7==0:c='tank_light'
     v.put(x,y,z,c)
 for x in [3.1,5.1,8.9,11.0]:
  for i in range(160):
   a=i/160*math.tau;v.ball((x,8+1.24*math.cos(a),2.5+1.24*math.sin(a)),.07,'iron')
 v.group('04_filler_ladder_and_empty_plate');v.box(6.7,7.7,3.6,7.5,8.3,3.9,'iron');v.box(6.8,7.8,3.9,7.4,8.2,4.0,'rust')
 for x in [6.7,7.4]:v.beam((x,6.7,1.1),(x,7.1,3.8),.05,'iron')
 for z in [1.2,1.6,2,2.4,2.8,3.2,3.6]:v.beam((6.7,6.7+(z-1.1)*.148,z),(7.4,6.7+(z-1.1)*.148,z),.05,'iron')
 v.box(8.5,6.7,2.3,9.6,6.8,2.8,'tank_light')
 v.group('05_low_water_outlet');v.beam((5.2,7.2,1.6),(5.2,6.5,1.6),.12,'rust');v.beam((5.2,6.5,1.6),(5.2,6.5,1.1),.09,'iron')
 v.beam((4.9,6.7,1.85),(5.5,6.7,1.85),.05,'iron');v.bucket((5.2,6.5,.4));v.box(5.1,6.4,.6,5.3,6.6,.7,'water')
 v.group('06_contaminated_water_and_footpath')
 for x in range(42,65):
  for y in range(49,68):
   if ((x-53)/12)**2+((y-59)/10)**2<1:
    band=((x-53)/12)**2+((y-59)/10)**2+.08*math.sin(x*.8+y*.6)
    v.put(x,y,4,'oil_copper'if .73<band<.84 else 'oil_blue'if .52<band<.61 else 'oil')
 for i in range(19):
  t=i/18;x=1.5+3.3*t;y=1.1+4.8*t
  for side in [-1,1]:v.box(x+side*.14,y,.4,x+side*.14+.12,y+.25,.5,'earth_edge')
 v.bucket((3.1,3.3,.4),True);v.bucket((1.9,1.6,.4));v.box(1.3,1.5,.4,1.7,1.8,.9,'ceramic')
 v.group('07_crouched_anonymous_collector');x,y=5.9,5.9
 for xx in [x-.2,x+.2]:v.box(xx-.1,y-.1,.4,xx+.15,y+.35,.6,'char');v.beam((xx,y,.6),(xx,y+.1,1.0),.13,'coat')
 v.ball((x,y,1.05),.35,'coat',scale=(.9,.6,1));v.ball((x,y-.04,1.53),.2,'cloth');v.box(x-.15,y-.2,1.4,x+.15,y-.1,1.6,'skin')
 v.beam((x-.25,y,1.25),(5.4,6.3,.85),.1,'coat');v.ball((5.4,6.3,.85),.1,'skin')
 v.group('08_shelter_edge_and_missing_return');v.brickwall(.3,.4,.4,.7,3.3,2.0);v.brickwall(.4,.4,.4,2.8,.8,1.6)
 v.box(.8,.8,.4,1.7,1.5,.6,'cloth');v.box(.9,1,.6,1.3,1.3,.8,'cloth_light')
 # An impact hollow intercepts a separate incomplete trail. It implies danger, not a named death.
 for x in range(82,105):
  for y in range(23,44):
   if ((x-93)/11)**2+((y-33)/10)**2<1:v.v.pop((x,y,3),None);v.put(x,y,2,'earth_edge')
 for i in range(10):v.box(7.4+i*.13,1.3+i*.21,.4,7.55+i*.13,1.5+i*.21,.5,'earth_edge')
 v.rubble((.3,.3,.45,13.5,10.5,.45),115)
 return v

# A separate background Blender process owns this disposable scene only.
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.context.preferences.filepaths.save_version=0
materials=[]
def linear(c):
 c=c/255
 return c/12.92 if c<=.04045 else ((c+.055)/1.055)**2.4
for name,color in PALETTE:
 m=bpy.data.materials.new(name);m.diffuse_color=tuple(linear(v)for v in color)+(1,);m.use_nodes=True;p=m.node_tree.nodes.get('Principled BSDF');p.inputs['Base Color'].default_value=m.diffuse_color;p.inputs['Roughness'].default_value=.88 if name not in ['water','oil','oil_blue','oil_copper']else.27
 if name in ['iron','bucket','brass','tank_teal','tank_light']:p.inputs['Metallic'].default_value=.35
 materials.append(m)
FACES=[((1,0,0),[(1,0,0),(1,1,0),(1,1,1),(1,0,1)]),((-1,0,0),[(0,1,0),(0,0,0),(0,0,1),(0,1,1)]),((0,1,0),[(1,1,0),(0,1,0),(0,1,1),(1,1,1)]),((0,-1,0),[(0,0,0),(1,0,0),(1,0,1),(0,0,1)]),((0,0,1),[(0,0,1),(1,0,1),(1,1,1),(0,1,1)]),((0,0,-1),[(0,1,0),(1,1,0),(1,0,0),(0,0,0)])]
def chunk(name,data=b'',children=b''):return name+struct.pack('<II',len(data),len(children))+data+children

def export(v):
 dest=OUT/v.name;dest.mkdir(parents=True,exist_ok=True)
 parts={p:i for i,p in enumerate(v.parts)};coords=sorted(v.v);maxs=[max(p[i]for p in coords)+1 for i in range(3)]
 sparse={'schema':'panzer.original-concept-voxels.v1','name':v.name,'voxel_size_m':UNIT,'axis':'right-handed Z-up; GLB exporter converts to Y-up','origin_m':[0,0,0],'dimensions_cells':maxs,'palette':[{'name':n,'rgba':[*c,255]}for n,c in PALETTE],'parts':v.parts,'voxels':[[*p,v.v[p][0],parts[v.v[p][1]]]for p in coords]}
 (dest/'model.voxels.json').write_text(json.dumps(sparse,separators=(',',':'))+'\n')
 xyzi=struct.pack('<I',len(coords))+b''.join(struct.pack('4B',*p,v.v[p][0]+1)for p in coords)
 pal=b''.join(bytes((*c,255))for _,c in PALETTE)+bytes(4*(256-len(PALETTE)))
 (dest/'model.vox').write_bytes(b'VOX '+struct.pack('<I',150)+chunk(b'MAIN',children=chunk(b'SIZE',struct.pack('<3I',*maxs))+chunk(b'XYZI',xyzi)+chunk(b'RGBA',pal)))
 groups={p:([],[],[])for p in v.parts}
 for pos,(mat,part) in v.v.items():
  vs,fs,ms=groups[part]
  for normal,corner in FACES:
   if tuple(pos[i]+normal[i]for i in range(3))in v.v:continue
   start=len(vs);vs.extend([tuple((pos[i]+q[i])*UNIT for i in range(3))for q in corner]);fs.append(tuple(range(start,start+4)));ms.append(mat)
 objects=[];quads=0
 for part,(vs,fs,ms) in groups.items():
  if not fs:continue
  mesh=bpy.data.meshes.new(part);mesh.from_pydata(vs,[],fs);mesh.update();obj=bpy.data.objects.new(part,mesh);bpy.context.collection.objects.link(obj)
  for m in materials:mesh.materials.append(m)
  for polygon,material in zip(mesh.polygons,ms):polygon.material_index=material
  objects.append(obj);quads+=len(fs)
 bpy.ops.object.select_all(action='DESELECT')
 for obj in objects:obj.select_set(True)
 bpy.context.view_layer.objects.active=objects[0]
 bpy.ops.export_scene.gltf(filepath=str(dest/'model.glb'),export_format='GLB',use_selection=True,export_yup=True,export_apply=True,export_materials='EXPORT')
 # Render original geometry as a studio diorama, not a simulated historical photograph.
 scene=bpy.context.scene;scene.render.engine='CYCLES';scene.cycles.samples=24;scene.cycles.use_denoising=True;scene.render.threads_mode='FIXED';scene.render.threads=8
 scene.render.resolution_x=1600;scene.render.resolution_y=1200;scene.render.resolution_percentage=100;scene.render.image_settings.file_format='PNG';scene.world=bpy.data.worlds.new('neutral_studio')if not scene.world else scene.world;scene.world.use_nodes=True;scene.world.node_tree.nodes['Background'].inputs[0].default_value=(.29,.36,.42,1);scene.world.node_tree.nodes['Background'].inputs[1].default_value=.45
 scene.view_settings.view_transform='AgX';scene.view_settings.look='AgX - Medium High Contrast'
 def area(name,loc,power,size,color):
  d=bpy.data.lights.new(name,'AREA');d.energy=power;d.shape='DISK';d.size=size;d.color=color;o=bpy.data.objects.new(name,d);scene.collection.objects.link(o);o.location=loc;o.rotation_euler=(Vector((6,5,2))-o.location).to_track_quat('-Z','Y').to_euler();return o
 lights=[area('soft overcast key',(-4,-6,16),2300,8,(.83,.89,1)),area('warm ruin rim',(12,10,12),1700,6,(1,.77,.49))]
 camera_data=bpy.data.cameras.new('concept_camera');camera=bpy.data.objects.new('concept_camera',camera_data);scene.collection.objects.link(camera);scene.camera=camera;camera_data.type='ORTHO'
 def shot(tag,eye,target,scale):
  camera.location=eye;camera.rotation_euler=(Vector(target)-camera.location).to_track_quat('-Z','Y').to_euler();camera_data.ortho_scale=scale;scene.render.filepath=str(CAP/f'voxel-runtime-concept-athena-{v.name}-{tag}-20261107-r2.png');bpy.ops.render.render(write_still=True);return scene.render.filepath
 if v.name.startswith('the-window'):
  previews=[shot('overview',(-12,-17,15),(6,4.7,2.6),17.8),shot('threshold',(7,-5,13),(5.7,4.0,1.8),12.7)]
 else:previews=[shot('overview',(-10,-15,15),(7,5.5,1.4),19),shot('outlet',(0,-4,6),(5.4,6.0,1.6),7.8)]
 bpy.ops.wm.save_as_mainfile(filepath=str(dest/'model.blend'),compress=True)
 report={'name':v.name,'voxel_size_m':UNIT,'dimensions_m':[n*UNIT for n in maxs],'occupied_voxels':len(coords),'exposed_quads':quads,'triangles':quads*2,'mesh_parts':len(objects),'material_count':len(PALETTE),'previews':previews,'editable_sources':['model.vox','model.voxels.json','model.blend'],'export':'model.glb','original_geometry':True,'runtime_import_tested':False,'performance_budget_claim':None}
 report['files']={p.name:{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}for p in dest.iterdir()if p.is_file()and p.name!='validation.json'}
 (dest/'validation.json').write_text(json.dumps(report,indent=2)+'\n');print('CONCEPT_COMPLETE',json.dumps(report),flush=True)
 for o in objects+lights+[camera]:bpy.data.objects.remove(o,do_unlink=True)
for build in [house,cistern]:export(build())
