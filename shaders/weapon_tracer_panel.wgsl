// Appended to the source-entrance panel vertex mapping. Separate HDR layer is composited only
// after main bloom; ordinary port imagery continues through its existing source-color bloom.
@fragment fn fs_tracer_panel(in:Out)->@location(0) vec4<f32> {
    if(in.source.w<=0.0){discard;}
    let uv=in.source.xy/in.source.w*vec2(0.5,-0.5)+vec2(0.5);
    if(any(uv<vec2(0.0)) || any(uv>vec2(1.0))){discard;}
    let extent=vec2<f32>(panel.layer.zw);let allocated=vec2<f32>(textureDimensions(exterior));
    let sample_uv=clamp(uv*extent,vec2(0.5),extent-vec2(0.5))/allocated;
    return vec4(textureSampleLevel(exterior,nearest_sampler,sample_uv,i32(panel.layer.x),0.0).rgb,0.0);
}
