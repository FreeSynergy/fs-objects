//! Fault-injection hooks used by the transaction tests.
//!
//! Pattern: none (test seam) — lets tests prove all-or-nothing behaviour.

use std::sync::atomic::Ordering;

use super::TursoStore;
use crate::{Error, Result};

impl TursoStore {
    /// Test hook: make the next state-changing operation fail after the object
    /// write but before the activity write, to prove the transaction rolls back.
    #[doc(hidden)]
    pub fn inject_fault_before_activity(&self, on: bool) {
        self.fail_before_activity.store(on, Ordering::SeqCst);
    }

    /// Test hook: make the next `put_and_log` fail after the `Create`/`Update` entry is written
    /// but before the extra entries, to prove the whole call rolls back.
    #[doc(hidden)]
    pub fn inject_fault_before_extra(&self, on: bool) {
        self.fail_before_extra.store(on, Ordering::SeqCst);
    }

    /// Test hook: make the next purge fail after it removed the entries but before it commits.
    #[doc(hidden)]
    pub fn inject_fault_in_purge(&self, on: bool) {
        self.fail_in_purge.store(on, Ordering::SeqCst);
    }

    pub(super) fn check_purge_fault(&self) -> Result<()> {
        if self.fail_in_purge.swap(false, Ordering::SeqCst) {
            return Err(Error::Backend("injected fault".into()));
        }
        Ok(())
    }

    pub(super) fn check_fault(&self) -> Result<()> {
        if self.fail_before_activity.swap(false, Ordering::SeqCst) {
            return Err(Error::Backend("injected fault".into()));
        }
        Ok(())
    }

    pub(super) fn check_extra_fault(&self) -> Result<()> {
        if self.fail_before_extra.swap(false, Ordering::SeqCst) {
            return Err(Error::Backend("injected fault".into()));
        }
        Ok(())
    }
}
