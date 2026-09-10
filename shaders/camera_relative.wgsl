// Shared by actual primary raster shaders and their bounded GPU precision regression.
// Subtract translations BEFORE adding the transformed local vertex. This avoids two large
// nearly equal absolute positions entering the short-near-plane perspective multiply.
fn camera_relative_position(model: mat4x4<f32>, position: vec3<f32>, camera: vec3<f32>) -> vec3<f32> {
    let linear = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    return linear * position + (model[3].xyz - camera);
}
