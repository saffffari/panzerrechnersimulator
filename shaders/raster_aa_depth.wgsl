// Conservative native-pixel depth for existing single-sample medium/particle consumers.
// Only mixed-coverage edge pixels lose depth detail. Color resolves all four real samples;
// the nearest source surface prevents fog/particles behind armor leaking across its edge.
@group(0) @binding(0) var source_depth: texture_depth_multisampled_2d;

@vertex fn vs(@builtin(vertex_index) index:u32)->@builtin(position) vec4<f32> {
    let p=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    return vec4(p[index],0.0,1.0);
}
@fragment fn fs(@builtin(position) pixel:vec4<f32>)->@builtin(frag_depth) f32 {
    let xy=vec2<i32>(pixel.xy);
    var nearest=1.0;
    for(var sample=0u;sample<textureNumSamples(source_depth);sample+=1u) {
        nearest=min(nearest,textureLoad(source_depth,xy,i32(sample)));
    }
    return nearest;
}
