//! Focused harness compiling the production publishing and interface sources.
//! Run only with ARCHY_TEST_PACKAGE=archipelago-publishing-tests through the
//! isolated backend runner. This crate is never included in shipped artifacts.
#[path = "../../archipelago/src/fips/iface.rs"]
pub mod fips_iface;
pub mod fips {
    pub use crate::fips_iface as iface;
}
#[path = "../../archipelago/src/publishing/mod.rs"]
pub mod publishing;
