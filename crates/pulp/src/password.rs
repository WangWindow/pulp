use std::fmt;

use crate::error::ArchiveResult;

/// The reason a provider was asked for a password.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PasswordReason {
    /// The archive header is encrypted.
    Header,
    /// Entry data is encrypted.
    Data,
    /// The previous password was rejected.
    Retry,
}

/// Context passed to a password provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PasswordRequest {
    /// Why a password is needed.
    pub reason: PasswordReason,
    /// One-based attempt number.
    pub attempt: u32,
}

/// A short-lived password buffer that clears its bytes on drop.
pub struct Password(Vec<u8>);

impl Password {
    /// Creates a password from UTF-8 text.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().into_bytes())
    }

    /// Returns the password bytes for the duration of a callback.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Returns the password as UTF-8 when it is valid text.
    pub fn as_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.0)
    }
}

impl Clone for Password {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Password(REDACTED)")
    }
}

impl Drop for Password {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// Supplies passwords to an archive operation.
pub trait PasswordProvider: Send + Sync {
    /// Returns a password for the request, or `None` to decline.
    fn request(&self, request: PasswordRequest) -> ArchiveResult<Option<Password>>;
}

/// A provider that always declines password requests.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoPasswordProvider;

impl PasswordProvider for NoPasswordProvider {
    fn request(&self, _request: PasswordRequest) -> ArchiveResult<Option<Password>> {
        Ok(None)
    }
}
