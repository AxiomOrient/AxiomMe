mod input;
mod resolve;
mod validate;

pub const DEFAULT_OBSERVER_MESSAGE_TOKENS: u32 = 30_000;
pub const DEFAULT_REFLECTOR_OBSERVATION_TOKENS: u32 = 40_000;
pub const DEFAULT_OBSERVER_BUFFER_TOKENS_RATIO: f64 = 0.2;
pub const DEFAULT_OBSERVER_BUFFER_ACTIVATION: f32 = 0.8;
pub const DEFAULT_REFLECTOR_BUFFER_ACTIVATION: f32 = 0.5;
pub const DEFAULT_BLOCK_AFTER_MULTIPLIER: f32 = 1.2;
pub const DEFAULT_OBSERVER_MAX_TOKENS_PER_BATCH: u32 = 10_000;

pub use input::{BufferTokensInput, ObservationConfigInput, OmConfigInput, ReflectionConfigInput};
pub use resolve::{
    ResolvedObservationConfig, ResolvedOmConfig, ResolvedReflectionConfig, resolve_om_config,
};
pub use validate::OmConfigError;

#[cfg(test)]
mod tests;
