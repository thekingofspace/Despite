#![allow(non_snake_case, non_upper_case_globals)]

mod prelude;
mod runner;
mod types;
mod validate;

pub use prelude::{
    DefaultEffectSource, EffectSignature, GlobalsData, MaxExtraInputs, MaxIdentifierLength,
    ParamField, ParamLayout, PreludeNames, build_prelude, compute_param_layout, pack_params,
    validate_identifier,
};
pub use runner::ShaderRunner;
pub use types::{
    ShaderError, ShaderImage, ShaderInput, ShaderJob, ShaderOutput, ShaderParam, ShaderValue,
};
pub use validate::{validate, validate_source};

#[cfg(test)]
mod tests;
