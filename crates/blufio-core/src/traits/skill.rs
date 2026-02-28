// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Skill runtime adapter trait for executing agent skills.

use async_trait::async_trait;

use crate::error::BlufioError;
use crate::traits::adapter::PluginAdapter;
use crate::types::{SkillInvocation, SkillManifest, SkillResult};

/// Adapter for managing and executing agent skills.
///
/// Skill runtime adapters handle the lifecycle of skills (WASM modules,
/// native plugins, etc.), including discovery, invocation, and sandboxing.
#[async_trait]
pub trait SkillRuntimeAdapter: PluginAdapter {
    /// Invokes a skill with the given parameters and returns the result.
    async fn invoke(&self, invocation: SkillInvocation) -> Result<SkillResult, BlufioError>;

    /// Lists all available skill manifests.
    fn list_skills(&self) -> Vec<SkillManifest>;
}
