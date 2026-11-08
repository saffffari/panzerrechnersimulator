//! CPU-only sparse-voxel lab. Authored erasure is not a ballistics solver or gameplay integration.
//! cargo run --release --bin voxel_chunk_probe -- --report target/chunk-probe.json
//! Optional: --obj-dir target/chunk-meshes --budget 2 --steps 32 [fixture.json ...]
#[path = "../voxel_chunks.rs"]
mod voxel_chunks;

use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use voxel_chunks::{Cell, Material, Quad, World, EDGE};

const DEFAULT_FIXTURES: [&str; 4] = [
    "assets/concepts/primary-accounts-20261107/athena/the-window-solnechnaya/model.voxels.json",
    "assets/concepts/primary-accounts-20261107/athena/water-under-fire/model.voxels.json",
    "assets/concepts/primary-accounts-20261107/hephaestus/factory-cellar-ward.voxels.json",
    "assets/concepts/primary-accounts-20261107/hephaestus/no-departure-airfield.voxels.json",
];

struct Fixture {
    cells: BTreeMap<Cell, Material>,
    palette: BTreeMap<Material, [f64; 3]>,
    cell_m: f64,
    origin_m: [f64; 3],
    min: Cell,
    max: Cell,
    schema: String,
    source_bytes: usize,
    source_fnv1a64: String,
}

impl Fixture {
    fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let source_bytes = bytes.len();
        let source_fnv1a64 = format!("{:016x}", bytes.iter().fold(0xcbf29ce484222325u64,
            |hash, byte| (hash ^ *byte as u64).wrapping_mul(0x100000001b3)));
        let data: Value = serde_json::from_slice(&bytes)?;
        let schema = data["schema"].as_str().context("fixture schema required")?;
        let zero_based = match schema {
            "panzer.original-concept-voxels.v1" => true,
            "panzer.sparse-voxel-concept.v1" => false,
            other => bail!("unsupported concept schema {other}"),
        };
        let cell_m = data[if zero_based { "voxel_size_m" } else { "cell_size_m" }]
            .as_f64().context("metric cell size required")?;
        ensure!(cell_m.is_finite() && cell_m > 0. && cell_m <= 10., "invalid metric cell size");
        let mut origin_m = [0.; 3];
        if let Some(origin) = data["origin_m"].as_array() {
            ensure!(origin.len() == 3, "origin_m needs three coordinates");
            for i in 0..3 {
                origin_m[i] = origin[i].as_f64().context("numeric origin required")?;
                ensure!(origin_m[i].is_finite(), "finite origin required");
            }
        }
        let mut palette = BTreeMap::new();
        for (index, entry) in data["palette"].as_array().context("palette required")?.iter().enumerate() {
            let id = if zero_based { u16::try_from(index + 1)? }
                else { u16::try_from(entry["id"].as_u64().context("positive palette id required")?)? };
            ensure!(id > 0, "material zero is reserved for air");
            let rgb = entry[if zero_based { "rgba" } else { "srgb" }].as_array().context("palette colour required")?;
            ensure!(rgb.len() >= 3, "RGB requires three values");
            let mut colour = [0.; 3];
            for i in 0..3 { colour[i] = rgb[i].as_f64().context("numeric colour required")? / 255.; }
            ensure!(palette.insert(id, colour).is_none(), "duplicate palette material");
        }
        let rows = data["voxels"].as_array().context("sparse voxels required")?;
        ensure!(!rows.is_empty() && rows.len() <= 2_000_000, "probe expects 1..2000000 cells");
        let mut cells = BTreeMap::new();
        let mut min = [i32::MAX; 3];
        let mut max = [i32::MIN; 3];
        for row in rows {
            let row = row.as_array().context("voxel row must be an array")?;
            ensure!(row.len() >= 4, "voxel row needs x,y,z,material");
            let mut p = [0; 3];
            for i in 0..3 {
                p[i] = i32::try_from(row[i].as_i64().context("integer cell coordinates required")?)?;
                min[i] = min[i].min(p[i]); max[i] = max[i].max(p[i]);
            }
            let raw = row[3].as_u64().context("integer material required")?;
            let material = u16::try_from(raw.checked_add(u64::from(zero_based)).context("material overflow")?)?;
            ensure!(palette.contains_key(&material), "unknown palette material {material}");
            ensure!(cells.insert(p, material).is_none(), "duplicate occupied cell {p:?}");
        }
        ensure!((0..3).all(|i| max[i] as i64 - min[i] as i64 <= 2048), "probe extents exceed 2048 cells");
        Ok(Self { cells, palette, cell_m, origin_m, min, max, schema: schema.into(), source_bytes, source_fnv1a64 })
    }
}

fn timings(samples: &[f64]) -> Value {
    if samples.is_empty() { return json!({"count":0}); }
    let mut sorted = samples.to_vec(); sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| sorted[((sorted.len()-1) as f64*p).ceil() as usize];
    json!({"count":samples.len(),"total_ms":samples.iter().sum::<f64>(),
        "p50_ms":percentile(0.5),"p95_ms":percentile(0.95),"max_ms":sorted.last()})
}

fn elapsed_ms(start: Instant) -> f64 { start.elapsed().as_secs_f64()*1000. }

fn allowed_dirty(p: Cell, allowed: &mut BTreeSet<Cell>) {
    let chunk = p.map(|v| v.div_euclid(EDGE)); allowed.insert(chunk);
    for axis in 0..3 {
        let local = p[axis].rem_euclid(EDGE);
        let offset = if local == 0 { -1 } else if local == EDGE-1 { 1 } else { 0 };
        if offset != 0 { let mut neighbor=chunk; neighbor[axis]+=offset; allowed.insert(neighbor); }
    }
}

fn publish(world: &mut World, cache: &mut BTreeMap<Cell, Vec<Quad>>, budget: usize,
    allowed: Option<&BTreeSet<Cell>>, touched: &mut BTreeSet<Cell>, times: &mut Vec<f64>) -> Result<()> {
    let start = Instant::now();
    let pages = world.publish(budget);
    times.push(elapsed_ms(start));
    ensure!(pages.len() <= budget, "publication exceeded chunk budget");
    for (key, mesh) in pages {
        if let Some(allowed) = allowed { ensure!(allowed.contains(&key), "unrelated chunk remeshed: {key:?}"); }
        touched.insert(key); cache.insert(key, mesh);
    }
    Ok(())
}

fn mesh_counts(meshes: &BTreeMap<Cell, Vec<Quad>>, cell_m: f64) -> Value {
    let quads = meshes.values().map(Vec::len).sum::<usize>();
    let area_cells = meshes.values().flatten().map(|q| q.width as u64*q.height as u64).sum::<u64>();
    json!({"quads":quads,"triangles":quads*2,"surface_area_m2":area_cells as f64*cell_m*cell_m})
}

fn write_obj(path: &Path, meshes: &BTreeMap<Cell, Vec<Quad>>, fixture: &Fixture) -> Result<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let mtl_path = path.with_extension("mtl");
    let mut mtl = BufWriter::new(std::fs::File::create(&mtl_path)?);
    for (id, rgb) in &fixture.palette {
        writeln!(mtl,"newmtl material_{id}\nKd {} {} {}\nd 1\n",rgb[0],rgb[1],rgb[2])?;
    }
    mtl.flush()?;
    let mut out = BufWriter::new(std::fs::File::create(path)?);
    writeln!(out,"# Authored local erasure demonstration; metres; Z up; not simulated ballistics")?;
    writeln!(out,"mtllib {}",mtl_path.file_name().unwrap().to_string_lossy())?;
    let mut index = 1;
    for (key, quads) in meshes {
        writeln!(out,"o chunk_{}_{}_{}",key[0],key[1],key[2])?;
        for q in quads {
            let u=(q.axis+1)%3;let v=(q.axis+2)%3;
            let corners=[(0,0),(q.width as i32,0),(q.width as i32,q.height as i32),(0,q.height as i32)];
            for (du,dv) in corners {
                let mut p=q.origin;p[u]+=du;p[v]+=dv;
                let pos=std::array::from_fn::<_,3,_>(|i|fixture.origin_m[i]+p[i] as f64*fixture.cell_m);
                writeln!(out,"v {:.6} {:.6} {:.6}",pos[0],pos[1],pos[2])?;
            }
            writeln!(out,"usemtl material_{}",q.material)?;
            if q.positive {writeln!(out,"f {} {} {} {}",index,index+1,index+2,index+3)?;}
            else {writeln!(out,"f {} {} {} {}",index+3,index+2,index+1,index)?;}
            index+=4;
        }
    }
    out.flush()?; Ok(())
}

fn probe(path: &Path, budget: usize, steps: usize, obj_dir: Option<&Path>) -> Result<Value> {
    let start=Instant::now();let fixture=Fixture::load(path).with_context(||format!("{}",path.display()))?;
    let parse_ms=elapsed_ms(start);
    let mut world=World::default();let start=Instant::now();
    for (&cell,&material) in &fixture.cells {world.set(cell,material);}
    let load_ms=elapsed_ms(start);
    let initial_occupied=world.occupied();let initial_chunks=world.chunks();let initial_bytes=world.cell_bytes();
    ensure!(initial_occupied==fixture.cells.len(),"fixture occupancy mismatch");
    let mut cache=BTreeMap::new();let mut initial_touched=BTreeSet::new();let mut initial_times=Vec::new();
    while world.pending()>0 {publish(&mut world,&mut cache,budget,None,&mut initial_touched,&mut initial_times)?;}
    let initial_mesh=mesh_counts(&cache,fixture.cell_m);let before=cache.clone();
    let centre=std::array::from_fn::<_,3,_>(|i|(fixture.min[i] as f64+fixture.max[i] as f64)*0.5);
    // Select an occupied anchor near the scene centre; nine parallel x sweeps cut a narrow
    // channel. Every operation only removes cells reached by the current elapsed segment.
    let anchor=*fixture.cells.keys().min_by(|a,b| {
        let score=|p:&&Cell|(0..3).map(|i|(p[i] as f64-centre[i]).powi(2)).sum::<f64>();
        score(a).total_cmp(&score(b))
    }).unwrap();
    let x0=fixture.min[0] as f64-1.;let x1=fixture.max[0] as f64+2.;
    let mut removed=BTreeSet::new();let mut allowed=BTreeSet::new();let mut touched=BTreeSet::new();
    let mut edit_times=Vec::new();let mut publish_times=Vec::new();let mut max_pending=0;
    for step in 0..steps {
        let start=Instant::now();
        for dy in -1..=1 {for dz in -1..=1 {
            let from=[x0+(x1-x0)*step as f64/steps as f64,anchor[1] as f64+dy as f64+0.5,anchor[2] as f64+dz as f64+0.5];
            let to=[x0+(x1-x0)*(step+1) as f64/steps as f64,from[1],from[2]];
            while let Some(hit)=world.first_solid(from,to) {
                ensure!((0.0..=1.0).contains(&hit.fraction),"nonfinite/out-of-segment hit");
                ensure!(fixture.cells.contains_key(&hit.cell),"collision invented a cell");
                let centre=hit.cell.map(|v|v as f64+0.5);
                // A radius below half a cell removes exactly this contacted cell, never a future
                // cell farther along the path. It deliberately makes no penetration prediction.
                ensure!(world.erase_sphere(centre,0.49)==1,"eraser must remove only contacted cell");
                ensure!(removed.insert(hit.cell),"cell erased twice");
                allowed_dirty(hit.cell,&mut allowed);
                ensure!(world.first_solid(centre,centre).is_none(),"collision waited for mesh publication");
                ensure!(world.get(hit.cell)==0,"authoritative edit not visible");
            }
        }}
        edit_times.push(elapsed_ms(start));max_pending=max_pending.max(world.pending());
        publish(&mut world,&mut cache,budget,Some(&allowed),&mut touched,&mut publish_times)?;
    }
    let drain_start=publish_times.len();
    while world.pending()>0 {publish(&mut world,&mut cache,budget,Some(&allowed),&mut touched,&mut publish_times)?;}
    ensure!(!removed.is_empty(),"authored path missed all occupied cells");
    ensure!(world.occupied()==initial_occupied-removed.len(),"final occupied count mismatch");
    for (&p,&material) in &fixture.cells {
        ensure!(world.get(p)==if removed.contains(&p){0}else{material},"unreached cell changed: {p:?}");
    }
    let mut unchanged=0;
    for (key,mesh) in &before {
        if !allowed.contains(key) {ensure!(cache.get(key)==Some(mesh),"unchanged chunk geometry changed");unchanged+=1;}
    }
    let obj=if let Some(dir)=obj_dir {
        let stem=if path.file_stem().is_some_and(|s|s=="model.voxels") {
            path.parent().and_then(Path::file_name).unwrap().to_string_lossy().into_owned()
        } else {path.file_stem().unwrap().to_string_lossy().into_owned()};
        let output=dir.join(format!("{stem}-after.obj"));write_obj(&output,&cache,&fixture)?;
        Some(output)
    }else{None};
    Ok(json!({"fixture":path,"source_bytes":fixture.source_bytes,"source_fnv1a64":fixture.source_fnv1a64,
        "schema":fixture.schema,"cell_size_m":fixture.cell_m,"axis_convention":"Z up (core is axis agnostic)",
        "bounds_cells":{"min":fixture.min,"max_inclusive":fixture.max},"parse_ms":parse_ms,"load_world_ms":load_ms,
        "initial":{"occupied":initial_occupied,"chunks":initial_chunks,"chunk_cell_bytes":initial_bytes,"mesh":initial_mesh},
        "final":{"occupied":world.occupied(),"chunks":world.chunks(),"chunk_cell_bytes":world.cell_bytes(),"mesh":mesh_counts(&cache,fixture.cell_m)},
        "tool":{"kind":"nine elapsed-segment-local single-cell erasure sweeps","anchor":anchor,"steps":steps,"removed_cells":removed.len()},
        "publication":{"budget_chunks_per_update":budget,"initial":timings(&initial_times),"during_and_after_edits":timings(&publish_times),
            "drain_updates":publish_times.len()-drain_start,"max_pending_chunks":max_pending,"published_distinct_chunks":touched.len()},
        "edit_timings":timings(&edit_times),"edit_timing_scope":"Per update: nine DDA segments, reached-cell erasure and immediate collision assertions; excludes mesh publication",
        "locality":{"unchanged_chunks_verified":unchanged,"unrelated_chunks_published":0},
        "checks":{"immediate_collision":true,"final_occupancy":true,"unreached_cells_unchanged":true,"publication_budget":true},"after_obj":obj}))
}

fn main() -> Result<()> {
    let mut report=None;let mut obj_dir=None;let mut budget=2;let mut steps=32;let mut fixtures=Vec::new();
    let mut args=std::env::args().skip(1);
    while let Some(arg)=args.next() {
        match arg.as_str() {
            "--report"=>report=Some(PathBuf::from(args.next().context("--report needs a path")?)),
            "--obj-dir"=>obj_dir=Some(PathBuf::from(args.next().context("--obj-dir needs a path")?)),
            "--budget"=>budget=args.next().context("--budget needs a count")?.parse::<usize>()?,
            "--steps"=>steps=args.next().context("--steps needs a count")?.parse::<usize>()?,
            "--help"|"-h"=>{println!("voxel_chunk_probe [--report FILE] [--obj-dir DIR] [--budget 2] [--steps 32] [fixture.json ...]\nCPU-only lab; no gameplay or ballistics claims. Defaults to four 20261107 concept fixtures.");return Ok(());},
            s if s.starts_with('-')=>bail!("unknown option {s}"),
            _=>fixtures.push(PathBuf::from(arg)),
        }
    }
    ensure!((1..=32).contains(&budget),"budget must be 1..32 chunks");
    ensure!((1..=128).contains(&steps),"steps must be 1..128");
    if fixtures.is_empty(){fixtures=DEFAULT_FIXTURES.into_iter().map(PathBuf::from).collect();}
    let mut results=Vec::new();
    for path in fixtures {eprintln!("[chunk-probe] {}",path.display());results.push(probe(&path,budget,steps,obj_dir.as_deref())?);}
    let result=json!({"schema":"panzer.voxel-chunk-probe.v1","scope":"Experimental CPU storage/collision/meshing lab, not integrated gameplay or a ballistics solver",
        "timing_limit":"Exploratory wall timings while other workloads may be active. Count budget is not a wall-time guarantee or hardware FPS proof.",
        "memory_limit":"chunk_cell_bytes excludes allocator, map, queue, fixture JSON, mesh and renderer allocations",
        "fixtures":results});
    let bytes=serde_json::to_vec_pretty(&result)?;
    if let Some(path)=report {
        if let Some(parent)=path.parent(){std::fs::create_dir_all(parent)?;}
        std::fs::write(&path,&bytes)?;println!("{}",path.display());
    }else{std::io::stdout().write_all(&bytes)?;println!();}
    Ok(())
}
