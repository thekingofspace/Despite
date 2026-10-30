fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {
    let Coord = vec2<i32>(floor(Uv * Globals.InputResolution));
    let Base = load_input(Coord);
    if (Base.a > 0.5) {
        return Base;
    }
    let Reach = clamp(Params.Thickness, 1, 8);
    let Size = vec2<i32>(Globals.InputResolution);
    for (var Y = -Reach; Y <= Reach; Y = Y + 1) {
        for (var X = -Reach; X <= Reach; X = X + 1) {
            let Neighbor = Coord + vec2<i32>(X, Y);
            let Inside = all(Neighbor >= vec2<i32>(0)) && all(Neighbor < Size);
            let Square = max(abs(X), abs(Y)) <= Reach;
            let Diamond = abs(X) + abs(Y) <= Reach;
            let Shape = select(Diamond, Square, Params.Diagonal != 0u);
            if (Inside && Shape && load_input(Neighbor).a > 0.5) {
                return Params.OutlineColor;
            }
        }
    }
    return Base;
}
