//! Device parsing, cache, and verification modules.

pub mod datasheet_fetcher;
pub(crate) mod dfp_datasheet;
pub mod dfp_manager;
pub(crate) mod dfp_paths;
pub(crate) mod dfp_store;
pub mod edc_parser;
pub mod pack_index;
pub mod pinout_verifier;
pub mod verifier_cache;
pub mod verify_compare;
pub mod verify_openai_stream;
pub mod verify_overlay;
pub mod verify_pdf;
pub mod verify_progress;
pub mod verify_prompt;
pub mod verify_provider;
pub mod verify_provider_anthropic;
pub mod verify_provider_openai;
pub mod verify_provider_schema;
