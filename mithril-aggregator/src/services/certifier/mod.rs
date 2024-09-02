//! ## Certifier Service
//!
//! This service is responsible for [OpenMessage] cycle of life. It creates open
//! messages and turn them into [Certificate]. To do so, it registers
//! single signatures and deal with the multi_signer for aggregate signature
//! creation.

mod certifier_service;
mod interface;

pub use certifier_service::*;
pub use interface::*;
