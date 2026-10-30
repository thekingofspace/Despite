fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {
    let Base = sample_input(Uv);
    let NoiseValue = textureSampleLevel(Noise, RepeatSampler, Uv * Params.NoiseScale, 0.0).r;
    let Amount = select(Params.Amount, Globals.Progress, Params.Animate != 0u);
    let Cut = Amount * (1.0 + Params.EdgeWidth);
    if (NoiseValue <= Cut - Params.EdgeWidth) {
        return vec4<f32>(0.0);
    }
    let Edge = 1.0 - smoothstep(Cut - Params.EdgeWidth, Cut, NoiseValue);
    let Burned = mix(Base.rgb, Params.EdgeColor.rgb, Edge * Params.EdgeColor.a);
    return vec4<f32>(Burned, Base.a);
}
