use std::collections::HashSet;

use crate::prelude::{
    EffectSignature, MaxExtraInputs, build_prelude, line_count, validate_identifier,
};
use crate::types::{ShaderError, ShaderImage, ShaderJob, ShaderParam};

pub(crate) struct AssembledSource {
    pub Text: String,
    pub PreludeLines: u32,
    pub UserLines: u32,
}

fn user_line_count(Source: &str) -> u32 {
    Source.lines().count().max(1) as u32
}

pub(crate) fn assemble_source(Job: &ShaderJob) -> AssembledSource {
    let Prelude = build_prelude(&Job.Params, &Job.input_names());
    let PreludeLines = line_count(&Prelude);
    let mut Text = Prelude;
    Text.push_str(&Job.Source);
    if !Job.Source.ends_with('\n') {
        Text.push('\n');
    }
    AssembledSource {
        Text,
        PreludeLines,
        UserLines: user_line_count(&Job.Source),
    }
}

pub fn validate(Job: &ShaderJob) -> Result<(), ShaderError> {
    validate_settings(Job)?;
    let Assembled = assemble_source(Job);
    check_wgsl(&Assembled)?;
    Ok(())
}

pub fn validate_source(
    Source: &str,
    Params: &[ShaderParam],
    InputNames: &[String],
) -> Result<(), ShaderError> {
    let NameRefs: Vec<&str> = InputNames.iter().map(String::as_str).collect();
    validate_names(Params, &NameRefs)?;
    let Prelude = build_prelude(Params, InputNames);
    let PreludeLines = line_count(&Prelude);
    let mut Text = Prelude;
    Text.push_str(Source);
    if !Source.ends_with('\n') {
        Text.push('\n');
    }
    check_wgsl(&AssembledSource {
        Text,
        PreludeLines,
        UserLines: user_line_count(Source),
    })?;
    Ok(())
}

fn check_image(Label: &str, Image: &ShaderImage) -> Result<(), ShaderError> {
    if Image.Width == 0 || Image.Height == 0 {
        return Err(ShaderError::new(format!(
            "{Label} has an empty size ({}x{})",
            Image.Width, Image.Height
        )));
    }
    if Image.Rgba.len() != Image.expected_len() {
        return Err(ShaderError::new(format!(
            "{Label} is {}x{} and needs {} RGBA bytes, but has {}",
            Image.Width,
            Image.Height,
            Image.expected_len(),
            Image.Rgba.len()
        )));
    }
    Ok(())
}

pub(crate) fn validate_settings(Job: &ShaderJob) -> Result<(), ShaderError> {
    if Job.FrameCount == 0 {
        return Err(ShaderError::new("FrameCount must be at least 1"));
    }
    if Job.FrameCount > 1 && !(Job.Fps.is_finite() && Job.Fps > 0.0) {
        return Err(ShaderError::new(format!(
            "Fps must be a positive number when rendering more than one frame (got {})",
            Job.Fps
        )));
    }
    check_image("The main image", &Job.Main)?;
    let (OutputWidth, OutputHeight) = Job.output_size();
    if OutputWidth == 0 || OutputHeight == 0 {
        return Err(ShaderError::new(format!(
            "Output size must not be empty (got {OutputWidth}x{OutputHeight})"
        )));
    }
    let NameRefs: Vec<&str> = Job.Inputs.iter().map(|Input| Input.Name.as_str()).collect();
    validate_names(&Job.Params, &NameRefs)?;
    for Input in &Job.Inputs {
        check_image(&format!("Input texture `{}`", Input.Name), &Input.Image)?;
    }
    Ok(())
}

fn validate_names(Params: &[ShaderParam], InputNames: &[&str]) -> Result<(), ShaderError> {
    if InputNames.len() > MaxExtraInputs {
        return Err(ShaderError::new(format!(
            "Too many extra input textures: {} (at most {MaxExtraInputs})",
            InputNames.len()
        )));
    }
    let mut SeenInputs = HashSet::new();
    for Name in InputNames {
        validate_identifier(Name)
            .map_err(|Reason| ShaderError::new(format!("Input texture: {Reason}")))?;
        if !SeenInputs.insert(*Name) {
            return Err(ShaderError::new(format!(
                "Input texture name `{Name}` is used more than once"
            )));
        }
    }
    let mut SeenParams = HashSet::new();
    for Param in Params {
        validate_identifier(&Param.Name)
            .map_err(|Reason| ShaderError::new(format!("Parameter: {Reason}")))?;
        if !SeenParams.insert(Param.Name.as_str()) {
            return Err(ShaderError::new(format!(
                "Parameter name `{}` is used more than once",
                Param.Name
            )));
        }
    }
    Ok(())
}

pub(crate) fn check_wgsl(Assembled: &AssembledSource) -> Result<naga::Module, ShaderError> {
    let Module = naga::front::wgsl::parse_str(&Assembled.Text).map_err(|Error| {
        let mut Message = Error.message().to_string();
        for Note in Error.notes() {
            Message.push_str("\nnote: ");
            Message.push_str(Note);
        }
        let Spans: Vec<naga::Span> = Error.labels().map(|(Span, _)| Span).collect();
        located_error(Message, &Spans, Assembled)
    })?;
    check_effect_signature(&Module, Assembled)?;
    let mut Validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    );
    Validator.validate(&Module).map_err(|Error| {
        let Message = error_chain(Error.as_inner());
        let Spans: Vec<naga::Span> = Error.spans().map(|(Span, _)| *Span).collect();
        located_error(Message, &Spans, Assembled)
    })?;
    Ok(Module)
}

fn check_effect_signature(
    Module: &naga::Module,
    Assembled: &AssembledSource,
) -> Result<(), ShaderError> {
    let Found = Module
        .functions
        .iter()
        .find(|(_, Function)| Function.name.as_deref() == Some("effect"));
    let Some((Handle, Function)) = Found else {
        return Err(ShaderError::new(format!(
            "Your source must define `{EffectSignature}`"
        )));
    };
    let IsFloatVector = |TypeHandle: naga::Handle<naga::Type>, Size: naga::VectorSize| {
        matches!(
            Module.types[TypeHandle].inner,
            naga::TypeInner::Vector { size: FoundSize, scalar: FoundScalar } if FoundSize == Size && FoundScalar == naga::Scalar::F32
        )
    };
    let ArgumentsMatch = Function.arguments.len() == 2
        && Function
            .arguments
            .iter()
            .all(|Argument| IsFloatVector(Argument.ty, naga::VectorSize::Bi));
    let ResultMatches = Function
        .result
        .as_ref()
        .is_some_and(|Output| IsFloatVector(Output.ty, naga::VectorSize::Quad));
    if ArgumentsMatch && ResultMatches {
        return Ok(());
    }
    let Span = Module.functions.get_span(Handle);
    Err(located_error(
        format!("`effect` must have the signature `{EffectSignature}`"),
        &[Span],
        Assembled,
    ))
}

pub(crate) fn error_chain(Error: &dyn std::error::Error) -> String {
    let mut Message = Error.to_string();
    let mut Current = Error.source();
    while let Some(Inner) = Current {
        let Text = Inner.to_string();
        if !Message.contains(&Text) {
            Message.push_str(": ");
            Message.push_str(&Text);
        }
        Current = Inner.source();
    }
    Message
}

fn located_error(
    Message: String,
    Spans: &[naga::Span],
    Assembled: &AssembledSource,
) -> ShaderError {
    let mut SawGenerated = false;
    for Span in Spans {
        if !Span.is_defined() {
            continue;
        }
        let Location = Span.location(&Assembled.Text);
        if Location.line_number > Assembled.PreludeLines {
            let Line = Location.line_number - Assembled.PreludeLines;
            if Line > Assembled.UserLines {
                let LastLine = Assembled
                    .Text
                    .lines()
                    .nth((Assembled.PreludeLines + Assembled.UserLines - 1) as usize)
                    .unwrap_or("");
                let Column = LastLine.chars().count() as u32 + 1;
                return ShaderError::at(Message, Some(Assembled.UserLines), Some(Column));
            }
            let Column = char_column(
                &Assembled.Text,
                Location.offset as usize,
                Location.line_position,
            );
            return ShaderError::at(Message, Some(Line), Some(Column));
        }
        SawGenerated = true;
    }
    if SawGenerated {
        return ShaderError::new(format!(
            "{Message}\n(reported in the generated prelude: make sure your source defines `{EffectSignature}` and does not redeclare prelude names)"
        ));
    }
    ShaderError::new(Message)
}

fn char_column(Text: &str, Offset: usize, BytePosition: u32) -> u32 {
    let LineStart = Offset.saturating_sub(BytePosition.saturating_sub(1) as usize);
    match Text.get(LineStart..Offset) {
        Some(Prefix) => Prefix.chars().count() as u32 + 1,
        None => BytePosition,
    }
}
