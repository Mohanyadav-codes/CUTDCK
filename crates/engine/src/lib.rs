//! Cutdeck engine: long video (file or link) in, vertical shorts out.

pub mod ai;
pub mod captions;
pub mod deps;
pub mod download;
pub mod edit;
pub mod faces;
pub mod frame;
pub mod media;
pub mod pick;
pub mod pipeline;
pub mod proc;
pub mod project;
pub mod render;
pub mod source;
pub mod transcribe;
pub mod types;

pub use pipeline::{Emit, Engine, Event, JobCtx};
pub use tokio_util::sync::CancellationToken;
