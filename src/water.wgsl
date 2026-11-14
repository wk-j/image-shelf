// Rune backdrop refraction: damped radial water waves, not painted circles.
struct Uniforms { viewport: vec4f, taps: array<vec4f, 5>, clips: array<vec4f, 5> }
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var backdrop: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@vertex fn vs(@builtin(vertex_index) id: u32) -> @builtin(position) vec4f {
    let p = array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));
    return vec4f(p[id],0.,1.);
}
@fragment fn fs(@builtin(position) pixel: vec4f) -> @location(0) vec4f {
    let p = pixel.xy / u.viewport.z;
    let uv = pixel.xy / u.viewport.xy;
    let original = textureSampleLevel(backdrop, linear_sampler, uv, 0.);
    if original.a < 0.001 { return original; }
    var bend = vec2f(0.);
    var light = 0.;
    var clip_min = vec2f(0.);
    var clip_max = u.viewport.xy / u.viewport.z;
    var affected = false;
    for(var i=0u; i<5u; i++) {
        let tap = u.taps[i];
        let clip = u.clips[i];
        if p.x < clip.x || p.y < clip.y || p.x >= clip.x+clip.z || p.y >= clip.y+clip.w { continue; }
        // Spread the same crest motion and damping over 0.8 seconds for a softer wave.
        let age = tap.z * (1.1 / 0.8);
        if tap.w == 0. || age < 0. || age > 1.1 { continue; }
        affected = true;
        clip_min = clip.xy + vec2f(0.5 / u.viewport.z);
        clip_max = clip.xy + clip.zw - vec2f(0.5 / u.viewport.z);
        let delta = p - tap.xy;
        let distance = length(delta);
        let front = distance - age * 145.;
        let envelope = exp(-front*front/750.) * exp(-age*2.8) * smoothstep(0.,0.055,age);
        // Several crests travel outward together. Their slope refracts the actual UI.
        let slope = cos(front * 0.38) * envelope;
        bend += delta / max(distance,1.) * slope * 5.5;
        light += slope * 0.025;
    }
    if !affected { return original; }
    let shifted = textureSampleLevel(backdrop, linear_sampler,
        clamp(p + bend, clip_min, clip_max) * u.viewport.z / u.viewport.xy, 0.);
    let color = shifted.rgb / max(shifted.a, 0.001);
    return vec4f(max(color + light,vec3f(0.)) * original.a, original.a);
}
