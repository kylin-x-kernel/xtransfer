//! Reliable transport mechanisms.
//!
//! This module provides reliability features:
//! - Reassembler: Out-of-order fragment reassembly
//! - RetransmitManager: Timeout-based retransmission with exponential backoff

mod reassembler;
mod retransmit;

pub use reassembler::{Reassembler, ReassemblyEntry};
pub use retransmit::{RetransmitManager, RetransmitTimer, RetransmitStats};
