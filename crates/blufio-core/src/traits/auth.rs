// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Authentication adapter trait for identity verification.

use async_trait::async_trait;

use crate::error::BlufioError;
use crate::traits::adapter::PluginAdapter;
use crate::types::{AuthIdentity, AuthToken};

/// Adapter for authenticating and verifying user identity.
///
/// Auth adapters validate tokens and resolve them to verified identities,
/// supporting various authentication mechanisms.
#[async_trait]
pub trait AuthAdapter: PluginAdapter {
    /// Authenticates the given token and returns the verified identity.
    async fn authenticate(&self, token: AuthToken) -> Result<AuthIdentity, BlufioError>;
}
