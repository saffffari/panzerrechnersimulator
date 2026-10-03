@group(0) @binding(0) var mask: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    let p=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    return vec4(p[i],0.0,1.0);
}
@fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {
    let size=vec2<i32>(textureDimensions(mask));let pixel=vec2<i32>(p.xy);
    let center=textureLoad(mask,pixel,0).r;
    var edge=0.0;
    for(var y=-1;y<=1;y+=1) {for(var x=-1;x<=1;x+=1) {
        edge=max(edge,textureLoad(mask,clamp(pixel+vec2(x,y),vec2(0),size-vec2(1)),0).r);
    }}
    // Resolved coverage is an edge mask, not fractional world emission. Use a stable
    // binary selected silhouette while preserving the actual occluded sample coverage.
    let alpha=select(0.0,1.0,edge>0.0 && center<=0.0);
    // One internal pixel around the visible silhouette, no filled overlay or X-ray selection.
    return vec4(vec3(1.0,0.78,0.24)*alpha,alpha);
}
