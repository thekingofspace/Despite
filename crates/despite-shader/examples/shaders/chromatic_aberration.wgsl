fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {
    let Heading = Params.Direction + vec2<f32>(0.00001, 0.0);
    let Wave = 1.0 + Params.Wobble * sin(Globals.LoopProgress * 6.2831853);
    let Shift = normalize(Heading) * Params.Amount * Wave * input_texel_size();
    let Red = sample_input(Uv + Shift);
    let Center = sample_input(Uv);
    let Blue = sample_input(Uv - Shift);
    let Alpha = max(max(Red.a, Center.a), Blue.a);
    return vec4<f32>(Red.r, Center.g, Blue.b, Alpha);
}
