//! [`LargeOriginals`] for the Turso backend: not built yet.
//!
//! Pattern: Adapter — Turso behind the trait; every method keeps the trait's `Unsupported` answer until the
//! blob table exists (`DESIGN.md`, "Large originals").

use super::TursoStore;
use crate::LargeOriginals;

impl LargeOriginals for TursoStore {}
