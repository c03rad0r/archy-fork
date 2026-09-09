mod async_lifecycle;
mod config;
mod dependencies;
mod install;
mod lifecycle;
mod pine_ha;
pub(crate) use install::patch_indeedhub_nostr_provider;
pub(crate) use pine_ha::wyoming_satellite_keeper;
mod progress;
mod runtime;
mod set_config;
mod stacks;
mod update;
mod validation;

// Re-export items needed by sibling modules (container.rs, security.rs, transitional.rs)
pub(in crate::api::rpc) use install::install_log;
pub(super) use validation::validate_app_id;
