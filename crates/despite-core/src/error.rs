use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("gif error: {0}")]
    Gif(#[from] gif::EncodingError),
    #[error("invalid data: {0}")]
    InvalidData(String),
    #[error("image too large: {Width}x{Height}")]
    TooLarge { Width: u32, Height: u32 },
    #[error("unsupported project version {0}")]
    UnsupportedVersion(u32),
    #[error("{0}")]
    Other(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
