"""Read back actual sparse voxel, MagicaVoxel and GLB files; no runtime assertions."""
import hashlib, json, math, struct
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
BASE=ROOT/'assets/concepts/primary-accounts-20261107/athena'

def chunk_data(blob):
    magic,version,total=struct.unpack_from('<4sII',blob)
    assert magic==b'glTF' and version==2 and total==len(blob)
    off=12;chunks={}
    while off<len(blob):
        size,kind=struct.unpack_from('<I4s',blob,off);off+=8
        chunks[kind]=blob[off:off+size];off+=size
    assert off==len(blob)
    return json.loads(chunks[b'JSON']),chunks[b'BIN\x00']

def validate(folder):
    report=json.loads((folder/'validation.json').read_text())
    vox=json.loads((folder/'model.voxels.json').read_text())
    cells=vox['voxels'];dims=vox['dimensions_cells'];unit=vox['voxel_size_m']
    assert vox['schema']=='panzer.original-concept-voxels.v1' and unit==.1
    assert vox['origin_m']==[0,0,0] and 'Z-up' in vox['axis']
    assert len({tuple(v[:3])for v in cells})==len(cells)
    assert all(len(v)==5 and all(isinstance(x,int) for x in v) for v in cells)
    assert all(all(0<=v[i]<dims[i] for i in range(3)) and 0<=v[3]<len(vox['palette']) and 0<=v[4]<len(vox['parts']) for v in cells)
    assert len(cells)==report['occupied_voxels']
    raw=(folder/'model.vox').read_bytes();assert raw[:8]==b'VOX '+struct.pack('<I',150)
    assert raw[8:12]==b'MAIN';main_size,child_size=struct.unpack_from('<II',raw,12)
    assert main_size==0 and child_size+20==len(raw)
    off=20;chunks={}
    while off<len(raw):
        name,size,child=struct.unpack_from('<4sII',raw,off);off+=12
        assert child==0;chunks[name]=raw[off:off+size];off+=size
    assert list(struct.unpack('<3I',chunks[b'SIZE']))==dims
    count=struct.unpack_from('<I',chunks[b'XYZI'])[0];assert count==len(cells)
    decoded=[tuple(chunks[b'XYZI'][4+i*4:8+i*4])for i in range(count)]
    assert decoded==[(*v[:3],v[3]+1)for v in cells]
    assert chunks[b'RGBA'][:4*len(vox['palette'])]==b''.join(bytes(p['rgba'])for p in vox['palette'])
    doc,buf=chunk_data((folder/'model.glb').read_bytes())
    types={'SCALAR':1,'VEC2':2,'VEC3':3,'VEC4':4};fmt={5126:'f',5125:'I',5123:'H',5121:'B'}
    def values(index):
        acc=doc['accessors'][index];view=doc['bufferViews'][acc['bufferView']]
        width=types[acc['type']];form='<'+fmt[acc['componentType']]*width
        step=view.get('byteStride',struct.calcsize(form));start=view.get('byteOffset',0)+acc.get('byteOffset',0)
        assert start+step*(acc['count']-1)+struct.calcsize(form)<=len(buf)
        return [struct.unpack_from(form,buf,start+i*step)for i in range(acc['count'])]
    triangles=0;positions=[]
    for mesh in doc['meshes']:
        for primitive in mesh['primitives']:
            assert primitive.get('mode',4)==4
            pos=values(primitive['attributes']['POSITION']);positions.extend(pos)
            normal=values(primitive['attributes']['NORMAL']);indices=values(primitive['indices'])
            assert len(normal)==len(pos)
            assert all(all(math.isfinite(v)for v in row)for row in pos+normal)
            assert all(abs(sum(v*v for v in n)-1)<1e-5 for n in normal)
            assert all(0<=i[0]<len(pos)for i in indices) and len(indices)%3==0
            assert 0<=primitive['material']<len(doc['materials'])
            triangles+=len(indices)//3
    assert triangles==report['triangles']
    bounds=[[min(p[i]for p in positions),max(p[i]for p in positions)]for i in range(3)]
    expected=[[0,dims[0]*unit],[0,dims[2]*unit],[-dims[1]*unit,0]]
    assert all(abs(bounds[i][j]-expected[i][j])<1e-4 for i in range(3)for j in range(2)),(bounds,expected)
    assert len(doc['meshes'])==report['mesh_parts']
    assert all((folder/p).stat().st_size<20_000_000 for p in ['model.glb','model.vox','model.voxels.json','model.blend'])
    for p,metadata in report['files'].items():
        if p in ['model.glb','model.vox','model.voxels.json','model.blend']:
            data=(folder/p).read_bytes();assert len(data)==metadata['bytes'] and hashlib.sha256(data).hexdigest()==metadata['sha256']
    return {'name':folder.name,'passed':True,'occupied_voxels':count,'source_dimensions_m':[round(v*unit,4)for v in dims],
            'glb_bounds_y_up_m':bounds,'triangles':triangles,'mesh_parts':len(doc['meshes']),
            'checks':['unique integer bounded cells','valid material and part references','VOX150 exact cells/palette roundtrip','GLB2 container and buffer/accessor bounds','finite positions and unit normals','triangle indices in range','GLB/source bounds agree after Z-up to Y-up conversion','triangles agree with exposed surface export','each editable/export file below20MB','artifact SHA256 verification'],
            'not_tested':['native engine import','runtime frame rate','collision/FEA behavior','LOD suitability']}

if __name__=='__main__':
    reports=[validate(BASE/name)for name in ['the-window-solnechnaya','water-under-fire']]
    (BASE/'readback-validation.json').write_text(json.dumps({'schema':'panzer.concept-readback.v1','assets':reports},indent=2)+'\n')
    print(json.dumps(reports,indent=2))
