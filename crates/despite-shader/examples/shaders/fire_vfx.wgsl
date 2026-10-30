fn flame_palette(Heat: f32) -> vec3<f32> {
    let Ember = vec3<f32>(0.45, 0.04, 0.02);
    let Orange = vec3<f32>(1.0, 0.42, 0.05);
    let Core = vec3<f32>(1.0, 0.93, 0.6);
    return mix(mix(Ember, Orange, smoothstep(0.0, 0.5, Heat)), Core, smoothstep(0.5, 1.0, Heat));
}

fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {
    let Base = sample_input(Uv);
    let Scroll = vec2<f32>(0.0, Globals.Time * Params.Speed);
    let Turbulence = fbm(Uv * vec2<f32>(6.0, 4.0) + Scroll, 5);
    var Heat = 0.0;
    for (var Step = 1; Step <= 10; Step = Step + 1) {
        let Fraction = f32(Step) / 10.0;
        let Sway = (value_noise(vec2<f32>(Uv.y * 8.0 + Globals.Time * Params.Speed * 2.0, f32(Step))) - 0.5) * 0.06 * Fraction;
        let Below = sample_input(Uv + vec2<f32>(Sway, Fraction * Params.Height)).a;
        Heat = max(Heat, Below * (1.0 - Fraction * 0.85));
    }
    Heat = clamp(Heat * (Turbulence * 1.8 - 0.15) * Params.Intensity, 0.0, 1.0);
    let Flame = vec4<f32>(flame_palette(Heat), smoothstep(0.08, 0.4, Heat));
    let Glow = vec3<f32>(1.0, 0.5, 0.1) * Heat * 0.25 * Params.Intensity;
    let Lit = vec4<f32>(min(Base.rgb + Glow, vec3<f32>(1.0)), Base.a);
    return blend_over(Lit, Flame);
}
