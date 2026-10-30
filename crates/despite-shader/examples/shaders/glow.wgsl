fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {
    let Base = sample_input(Uv);
    let Texel = input_texel_size();
    var Accumulated = 0.0;
    var TotalWeight = 0.0;
    for (var Ring = 1; Ring <= 4; Ring = Ring + 1) {
        let Distance = Params.Radius * f32(Ring) / 4.0;
        let Weight = 1.0 - f32(Ring - 1) / 4.0;
        for (var Step = 0; Step < 12; Step = Step + 1) {
            let Angle = f32(Step) * 0.5235988 + f32(Ring) * 0.37;
            let Offset = vec2<f32>(cos(Angle), sin(Angle)) * Distance * Texel;
            Accumulated = Accumulated + sample_input(Uv + Offset).a * Weight;
            TotalWeight = TotalWeight + Weight;
        }
    }
    let Wave = select(1.0, 0.8 + 0.2 * sin(Globals.LoopProgress * 6.2831853), Params.Pulse != 0u);
    let GlowAlpha = clamp(Accumulated / TotalWeight * Params.Intensity * Wave, 0.0, 1.0);
    let Glow = vec4<f32>(Params.GlowColor.rgb, GlowAlpha * Params.GlowColor.a);
    return blend_over(Base, Glow);
}
