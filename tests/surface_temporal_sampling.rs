//! Independent numeric witnesses for surface source filtering, not rendered acceptance.
#[derive(Clone, Copy)]
struct Sample { id:u32, depth:f64, normal:[f64;3], value:f64 }
fn moments(center:Sample, neighbors:&[Sample])->(f64,f64,f64) {
    let mut values=Vec::new();
    for n in neighbors {
        let dot=n.normal.iter().zip(center.normal).map(|(a,b)|a*b).sum::<f64>();
        if n.id==center.id && (n.depth-center.depth).abs()<=0.015*center.depth+0.008 && dot>=0.985 {
            values.push(n.value);
        }
    }
    if values.is_empty(){values.push(center.value);}
    let count=values.len() as f64;
    let mean=values.iter().sum::<f64>()/count;
    let sigma=(values.iter().map(|x|x*x).sum::<f64>()/count-mean*mean).max(0.0).sqrt();
    (mean,sigma,count)
}
fn update(hist:f64,age:f64,mean:f64,sigma:f64,n:f64,source_changed:bool)->(f64,f64) {
    let scale=mean.max(hist).max(0.002);
    let change=((hist-mean).abs()-4.0*sigma/n.sqrt()).max(0.0)/scale;
    let reset=source_changed && age>0.0 && n>=4.0 && sigma/scale<=0.35 && change>0.15;
    let age=if reset{1.0}else{(age+1.0).min(64.0)};
    let weight=(1.0/age).max(0.04);
    (hist*(1.0-weight)+mean*weight,age)
}
#[test]
fn flat_noisy_source_converges_without_changing_its_mean() {
    let mut random=17u64;let mut raw_error=0.0;let mut filtered_error=0.0;let mut avg=0.0;
    let (mut history,mut age)=(0.0,0.0);
    for frame in 0..2000 {
        let mut samples=Vec::new();
        for _ in 0..9 {
            random=random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let v=0.1+1.8*((random>>32) as f64/u32::MAX as f64);
            samples.push(Sample{id:4,depth:1.2,normal:[0.0,1.0,0.0],value:v});
        }
        let (mean,sigma,n)=moments(samples[4],&samples);
        (history,age)=update(history,age,mean,sigma,n,false);
        if frame>=200 {raw_error+=(samples[4].value-1.0).powi(2);filtered_error+=(history-1.0).powi(2);avg+=history;}
    }
    assert!((avg/1800.0-1.0).abs()<0.012);
    assert!(filtered_error<raw_error*0.025,"raw={raw_error}, filtered={filtered_error}");
}
#[test]
fn different_ids_depths_and_normals_cannot_bleed_into_wall_source() {
    let wall=Sample{id:4,depth:1.2,normal:[0.0,1.0,0.0],value:0.2};
    let neighbors=[wall,Sample{id:5,value:20.0,..wall},Sample{depth:2.0,value:20.0,..wall},
        Sample{normal:[1.0,0.0,0.0],value:20.0,..wall}];
    assert_eq!(moments(wall,&neighbors),(0.2,0.0,1.0));
}
#[test]
fn coherent_flash_and_its_end_are_visible_on_first_frame() {
    let (lit,age)=update(0.2,64.0,5.0,0.002,9.0,true);
    assert_eq!(lit,5.0);assert_eq!(age,1.0);
    let (dark,age)=update(lit,age,0.2,0.0001,9.0,true);
    assert_eq!(dark,0.2);assert_eq!(age,1.0);
}
#[test]
fn missed_rare_aperture_samples_do_not_claim_the_source_switched_off() {
    // A stable emitter, previously estimated at 1, is missed by all nine current rays. The
    // resulting zero-variance set is still a noisy estimate, not a witnessed lighting change.
    let (radiance,age)=update(1.0,46.0,0.0,0.0,9.0,false);
    assert_eq!(age,47.0);
    assert!((radiance-0.96).abs()<1e-12);
    // Geometry/disocclusion rejection supplies age zero and must still replace stale history.
    assert_eq!(update(99.0,0.0,0.2,0.0,1.0,false),(0.2,1.0));
}

#[test]
fn rare_bright_source_keeps_its_mean_and_history_through_dark_sample_sets() {
    // Independent importance-sampling witness: each ray reaches a small opening with probability
    // .02 and returns radiance40, above a .2 steady base. The true expectation is therefore1.0.
    // No actual source or geometry changes occur during this sequence.
    let mut random=901u64;
    let (mut history,mut age)=(0.0,0.0);
    let (mut old_history,mut old_age)=(0.0,0.0);
    let (mut total,mut old_total,mut resets)=(0.0,0.0,0usize);
    let (mut misses,mut retained_misses)=(0usize,0usize);
    for frame in 0..12_000 {
        let mut samples=Vec::new();
        for _ in 0..9 {
            random=random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let hits=((random>>32) as f64 / u32::MAX as f64) < 0.02;
            samples.push(Sample{id:4,depth:1.2,normal:[0.0,1.0,0.0],value:0.2+if hits{40.0}else{0.0}});
        }
        let (mean,sigma,n)=moments(samples[4],&samples);
        (history,age)=update(history,age,mean,sigma,n,false);
        // Control: the prior rule inferred source changes from each random sample set alone.
        (old_history,old_age)=update(old_history,old_age,mean,sigma,n,true);
        if frame>=1_000 {
            total+=history;old_total+=old_history;
            resets+=usize::from(age==1.0);
            if mean<0.201 {misses+=1;retained_misses+=usize::from(age==64.0);}
        }
    }
    let mean=total/11_000.0;
    let old_mean=old_total/11_000.0;
    assert!((mean-1.0).abs()<0.06,"stable rare-source mean was biased: {mean}");
    assert!((old_mean-1.0).abs()>0.15,"witness does not detect the old false-reset failure: {old_mean}");
    assert_eq!(resets,0);
    assert!(misses>7_000 && retained_misses==misses,
        "all-missed sets shortened valid source history");
}

#[test]
fn tiny_source_jitter_does_not_turn_into_a_coherent_radiance_cut() {
    // Even if a conservative host marks source input changes, the existing significant/coherent
    // radiance guard must reject tiny variations rather than repeatedly returning to age1.
    let (radiance,age)=update(0.2,46.0,0.2001,0.0001,9.0,true);
    assert_eq!(age,47.0);
    assert!((radiance-0.200004).abs()<1e-12);
}

#[test]
fn stationary_reflections_retain_history_when_a_narrow_source_is_missed() {
    // The fixed-eye glossy case used to unconditionally permit a source cut. A dark, uniform
    // current neighborhood then repeatedly discarded a real bright reflection's history.
    let fixed_eye = true;
    let fixed_surface = true;
    let source_changed = false;
    let allow_reflection_cut = source_changed || !fixed_eye || !fixed_surface;
    let (retained, age) = update(2.0, 46.0, 0.0, 0.0, 9.0, allow_reflection_cut);
    assert_eq!(age, 47.0);
    assert!((retained - 1.92).abs() < 1e-12);
    assert_eq!(update(2.0, 46.0, 0.0, 0.0, 9.0, true), (0.0, 1.0));
    // A changed light, or changed eye/surface orientation, still permits a coherent response.
    for (fixed_eye, fixed_surface, source_changed) in
        [(true, true, true), (false, true, false), (true, false, false)] {
        let allow_cut = source_changed || !fixed_eye || !fixed_surface;
        assert_eq!(update(0.2, 46.0, 5.0, 0.0001, 9.0, allow_cut), (5.0, 1.0));
    }
}

#[test]
fn surface_shader_validates_and_retains_geometry_and_reflection_guards() {
    let source=include_str!("../shaders/accum.wgsl");
    let m=wgpu::naga::front::wgsl::parse_str(source).expect("surface WGSL");
    wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all()).validate(&m).expect("valid surface WGSL");
    for required in ["textureLoad(id_tex, q, 0).r != id","dot(qn.xyz, center_normal.xyz)",
        "let spatial_filter = is_diffuse || carrier >= 0.55", "identity_delta(id)",
        "expected_depth = clamp(pndc.z, 0.0, 1.0)",
        "let reflection_view_changed = !is_diffuse",
        "u.surface_history.x < 0.5 || !identity_delta(id)",
        "let source_change_allowed = u.surface_history.y > 0.5 || reflection_view_changed",
        "history_age > 0.0 && support >= 4.0 && source_change_allowed"] {assert!(source.contains(required));}
    assert!(!source.contains("hist = clamp(hist"));
}
