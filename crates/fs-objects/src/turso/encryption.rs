//! Maps a [`KeyProvider`](crate::KeyProvider) onto Turso's page encryption.
//!
//! Pattern: Strategy — the key provider is the strategy for obtaining the key; this module is
//! the adapter that applies the result to the Turso builder.

use super::options::OpenOptions;
use crate::Result;

/// Configure `b` for encryption if the options carry a key provider.
/// Returns the builder and whether encryption is on.
pub(super) fn configure(b: turso::Builder, opts: &OpenOptions) -> Result<(turso::Builder, bool)> {
    let Some(kp) = &opts.key_provider else {
        return Ok((b, false));
    };
    let key = kp.key_for(&opts.store_name)?;
    key.validate()?;
    let b = b
        .experimental_encryption(true)
        .with_encryption(turso::EncryptionOpts {
            cipher: key.cipher.turso_name().into(),
            hexkey: key.hex(),
        });
    Ok((b, true))
}
