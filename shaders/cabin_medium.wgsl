// Source-projected cavity columns: nearest 2 cm cells, source Y intersections, and articulated
// hull/turret frames. AABBs reject work only. No camera flag or convex hull defines cabin air.
struct CabinMedium {
    world_to_local:array<mat4x4<f32>,3>,
    grid_min_step:array<vec4<f32>,3>,
    grid_size:array<vec4<u32>,3>,
    bounds_min:array<vec4<f32>,3>,
    bounds_max:array<vec4<f32>,3>,
};
const CABIN_RAY_EPSILON:f32=0.00001;
fn cabin_contains(point:vec3<f32>)->bool {
    for(var domain=0u;domain<3u;domain+=1u){
        let grid=cabin_grid(domain);let size=cabin_size(domain);
        if(grid.w<0.5 || any(point<cabin_min(domain)) || any(point>cabin_max(domain))){continue;}
        let p=(cabin_transform(domain)*vec4(point,1.0)).xyz;
        let cell=vec2<i32>(floor((p.xz-grid.xy)/grid.z));
        if(any(cell<vec2(0)) || any(cell>=vec2<i32>(size.xy))){continue;}
        let interval=textureLoad(cabin_air_columns,cell,i32(size.z),0).xy;
        if(interval.y>interval.x && p.y>=interval.x && p.y<=interval.y){return true;}
    }
    return false;
}
fn cabin_domain_run(origin:vec3<f32>,ray:vec3<f32>,start:f32,limit:f32,domain:u32)->vec2<f32> {
    let grid=cabin_grid(domain);let size=cabin_size(domain);
    let empty=vec2(limit);
    if(grid.w<0.5){return empty;}
    var begin=max(start,0.0);var end=limit;
    // Conservative world bounds avoid all texture reads for distant weather rays.
    let box_min=cabin_min(domain);let box_max=cabin_max(domain);
    for(var axis=0u;axis<3u;axis+=1u){
        if(abs(ray[axis])<1e-8){if(origin[axis]<box_min[axis] || origin[axis]>box_max[axis]){return empty;}continue;}
        let a=(box_min[axis]-origin[axis])/ray[axis];let b=(box_max[axis]-origin[axis])/ray[axis];
        begin=max(begin,min(a,b));end=min(end,max(a,b));
    }
    if(end<=begin){return empty;}
    let transform=cabin_transform(domain);
    let p=(transform*vec4(origin,1.0)).xyz;let d=(transform*vec4(ray,0.0)).xyz;
    let grid_max=grid.xy+vec2<f32>(size.xy)*grid.z;
    for(var axis=0u;axis<2u;axis+=1u){
        let v=d.xz[axis];let o=p.xz[axis];
        if(abs(v)<1e-8){if(o<grid.xy[axis] || o>=grid_max[axis]){return empty;}continue;}
        let a=(grid.xy[axis]-o)/v;let b=(grid_max[axis]-o)/v;
        begin=max(begin,min(a,b));end=min(end,max(a,b));
    }
    if(end<=begin){return empty;}
    var cursor=begin;var run=empty;var found=false;
    // Exact DDA cell exits, followed by the column's source floor/roof intersections. The
    // epsilon selects which cell owns a shared boundary; it never expands the Y interval.
    loop {
        if(cursor>=end){break;}
        let sample=min(cursor+CABIN_RAY_EPSILON,(cursor+end)*0.5);
        let cell=clamp(vec2<i32>(floor(((p+d*sample).xz-grid.xy)/grid.z)),vec2(0),vec2<i32>(size.xy)-vec2(1));
        var next=end;
        for(var axis=0u;axis<2u;axis+=1u){
            let v=d.xz[axis];if(abs(v)<1e-8){continue;}
            let edge=grid.xy[axis]+f32(cell[axis]+select(0,1,v>0.0))*grid.z;
            let crossing=(edge-p.xz[axis])/v;
            if(crossing>cursor+CABIN_RAY_EPSILON*0.1){next=min(next,crossing);}
        }
        let column=textureLoad(cabin_air_columns,cell,i32(size.z),0).xy;
        var lo=cursor;var hi=next;
        if(column.y<=column.x){hi=lo;}
        else if(abs(d.y)<1e-8){if(p.y<column.x || p.y>column.y){hi=lo;}}
        else {let a=(column.x-p.y)/d.y;let b=(column.y-p.y)/d.y;lo=max(lo,min(a,b));hi=min(hi,max(a,b));}
        if(hi>lo){
            if(!found){run=vec2(lo,hi);found=true;}
            else if(lo<=run.y+CABIN_RAY_EPSILON){run.y=max(run.y,hi);}
            else{return run;}
        }else if(found){return run;}
        if(next<=cursor){break;}
        cursor=next;
    }
    return run;
}
fn cabin_next_span(origin:vec3<f32>,ray:vec3<f32>,start:f32,limit:f32)->vec2<f32> {
    var runs:array<vec2<f32>,3>;var result=vec2(limit);
    for(var domain=0u;domain<3u;domain+=1u){
        runs[domain]=cabin_domain_run(origin,ray,start,limit,domain);
        if(runs[domain].x<result.x){result=runs[domain];}
    }
    if(result.y<=result.x){return result;}
    // Merge the actual intervals at overlapping hull/turret interfaces, never their bounds.
    loop {
        let before=result.y;
        for(var domain=0u;domain<3u;domain+=1u){
            var span=runs[domain];
            if(span.y<result.y-CABIN_RAY_EPSILON){
                span=cabin_domain_run(origin,ray,max(start,result.y-CABIN_RAY_EPSILON),limit,domain);
                runs[domain]=span;
            }
            if(span.x<=result.y+CABIN_RAY_EPSILON && span.y>span.x){result.y=max(result.y,span.y);}
        }
        if(result.y<=before+CABIN_RAY_EPSILON){break;}
    }
    return result;
}
fn cabin_path_length(origin:vec3<f32>,ray:vec3<f32>,limit:f32)->f32 {
    var cursor=0.0;var sum=0.0;
    loop {
        if(cursor>=limit){break;}
        let interval=cabin_next_span(origin,ray,cursor,limit);
        if(interval.y<=interval.x){break;}
        sum+=interval.y-interval.x;
        cursor=interval.y+CABIN_RAY_EPSILON;
    }
    return min(sum,max(limit,0.0));
}
