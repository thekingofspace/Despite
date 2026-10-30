fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {
    let Base = sample_input(Uv);
    let Texel = input_texel_size();
    var Glow = vec4<f32>(0.0);
    for (var Offset = -3; Offset <= 3; Offset = Offset + 1) {
        Glow = Glow + sample_input(Uv + vec2<f32>(f32(Offset), 0.0) * Texel * 2.0);
        Glow = Glow + sample_input(Uv + vec2<f32>(0.0, f32(Offset)) * Texel * 2.0);
    }
    Glow = Glow / 14.0;
    let Lit = Base.rgb + Glow.rgb * Params.Tint.rgb * Params.Strength * Glow.a;
    return vec4<f32>(min(Lit, vec3<f32>(1.0)), max(Base.a, Glow.a * Params.Strength * 0.5));
}
