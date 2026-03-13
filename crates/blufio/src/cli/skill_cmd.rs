// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Skill management CLI handlers for `blufio skill` subcommands.

use crate::SkillCommands;

/// Handle `blufio skill <action>` subcommands.
pub(crate) async fn handle_skill_command(
    config: &blufio_config::model::BlufioConfig,
    action: SkillCommands,
) -> Result<(), blufio_core::BlufioError> {
    match action {
        SkillCommands::Init { name } => {
            let target_dir = std::path::Path::new(".");
            blufio_skill::scaffold_skill(&name, target_dir)?;
            eprintln!("Skill project '{name}' created successfully.");
            eprintln!("  cd {name} && cargo build --target wasm32-wasip1 --release");
            Ok(())
        }
        SkillCommands::List => {
            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));
            let skills = store.list().await?;

            if skills.is_empty() {
                println!("No skills installed.");
            } else {
                println!(
                    "{:<20} {:<10} {:<12} DESCRIPTION",
                    "NAME", "VERSION", "STATUS"
                );
                println!("{}", "-".repeat(70));
                for skill in &skills {
                    println!(
                        "{:<20} {:<10} {:<12} {}",
                        skill.name, skill.version, skill.verification_status, skill.description
                    );
                }
            }
            Ok(())
        }
        SkillCommands::Install {
            wasm_path,
            manifest_path,
        } => {
            // Read and parse the manifest.
            let manifest_content = std::fs::read_to_string(&manifest_path)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;
            let manifest = blufio_skill::parse_manifest(&manifest_content)?;

            // Read the WASM file.
            let wasm_bytes = std::fs::read(&wasm_path)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;

            // Compute content hash.
            let content_hash = blufio_skill::compute_content_hash(&wasm_bytes);

            // Check for adjacent .sig file.
            let sig_path = format!("{}.sig", wasm_path);
            let (signature, publisher_id) = if std::path::Path::new(&sig_path).exists() {
                let sig_content = std::fs::read_to_string(&sig_path)
                    .map_err(blufio_core::BlufioError::skill_execution_failed)?;
                let (pub_id, sig_hash, sig_hex) = parse_sig_file(&sig_content)?;

                // Verify the hash in .sig matches our computed hash.
                if sig_hash != content_hash {
                    return Err(blufio_core::BlufioError::Security(format!(
                        "signature file hash mismatch: .sig says {} but WASM hashes to {}",
                        &sig_hash[..12],
                        &content_hash[..12]
                    )));
                }

                // Verify the signature against the WASM bytes.
                let sig = blufio_skill::signature_from_hex(&sig_hex)?;
                let pubkey_bytes = hex::decode(&pub_id).map_err(|e| {
                    blufio_core::BlufioError::Security(format!("invalid publisher_id hex: {e}"))
                })?;
                let pubkey_array: [u8; 32] = pubkey_bytes.try_into().map_err(|_| {
                    blufio_core::BlufioError::Security(
                        "publisher_id must be exactly 32 bytes".to_string(),
                    )
                })?;
                let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_array)
                    .map_err(|e| {
                        blufio_core::BlufioError::Security(format!("invalid publisher key: {e}"))
                    })?;
                blufio_skill::PublisherKeypair::verify_signature(
                    &verifying_key,
                    &wasm_bytes,
                    &sig,
                )?;

                eprintln!("  Signature verified (publisher: {}...)", &pub_id[..12]);

                (Some(sig_hex), Some(pub_id))
            } else {
                (None, None)
            };

            // Serialize capabilities to JSON for storage.
            let capabilities_json =
                serde_json::to_string(&manifest.capabilities).unwrap_or_else(|_| "{}".to_string());

            // Open DB and store the skill.
            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));

            // TOFU: store publisher key if signed.
            if let Some(ref pub_id) = publisher_id {
                store.check_or_store_publisher_key(pub_id, pub_id).await?;
            }

            store
                .install(
                    &manifest.name,
                    &manifest.version,
                    &manifest.description,
                    manifest.author.as_deref(),
                    &wasm_path,
                    &manifest_content,
                    &capabilities_json,
                    Some(&content_hash),
                    signature.as_deref(),
                    publisher_id.as_deref(),
                )
                .await?;

            let status = if signature.is_some() {
                "verified"
            } else {
                "unverified"
            };
            eprintln!(
                "Skill '{}' v{} installed successfully. [{}]",
                manifest.name, manifest.version, status
            );

            // Print capabilities summary.
            if manifest.capabilities.network.is_some() {
                eprintln!("  Capabilities: network access");
            }
            if manifest.capabilities.filesystem.is_some() {
                eprintln!("  Capabilities: filesystem access");
            }
            if !manifest.capabilities.env.is_empty() {
                eprintln!(
                    "  Capabilities: env vars ({})",
                    manifest.capabilities.env.join(", ")
                );
            }

            Ok(())
        }
        SkillCommands::Remove { name } => {
            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));
            store.remove(&name).await?;
            eprintln!("Skill '{name}' removed.");
            Ok(())
        }
        SkillCommands::Update {
            name,
            wasm_path,
            manifest_path,
        } => {
            let manifest_content = std::fs::read_to_string(&manifest_path)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;
            let manifest = blufio_skill::parse_manifest(&manifest_content)?;

            let wasm_bytes = std::fs::read(&wasm_path)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;

            let content_hash = blufio_skill::compute_content_hash(&wasm_bytes);

            // Check for adjacent .sig file.
            let sig_path = format!("{}.sig", wasm_path);
            let (signature, publisher_id) = if std::path::Path::new(&sig_path).exists() {
                let sig_content = std::fs::read_to_string(&sig_path)
                    .map_err(blufio_core::BlufioError::skill_execution_failed)?;
                let (pub_id, sig_hash, sig_hex) = parse_sig_file(&sig_content)?;
                if sig_hash != content_hash {
                    return Err(blufio_core::BlufioError::Security(format!(
                        "signature file hash mismatch: .sig says {} but WASM hashes to {}",
                        &sig_hash[..12],
                        &content_hash[..12]
                    )));
                }
                let sig = blufio_skill::signature_from_hex(&sig_hex)?;
                let pubkey_bytes = hex::decode(&pub_id).map_err(|e| {
                    blufio_core::BlufioError::Security(format!("invalid publisher_id hex: {e}"))
                })?;
                let pubkey_array: [u8; 32] = pubkey_bytes.try_into().map_err(|_| {
                    blufio_core::BlufioError::Security(
                        "publisher_id must be exactly 32 bytes".to_string(),
                    )
                })?;
                let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_array)
                    .map_err(|e| {
                        blufio_core::BlufioError::Security(format!("invalid publisher key: {e}"))
                    })?;
                blufio_skill::PublisherKeypair::verify_signature(
                    &verifying_key,
                    &wasm_bytes,
                    &sig,
                )?;
                (Some(sig_hex), Some(pub_id))
            } else {
                (None, None)
            };

            let capabilities_json =
                serde_json::to_string(&manifest.capabilities).unwrap_or_else(|_| "{}".to_string());

            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));

            if let Some(ref pub_id) = publisher_id {
                store.check_or_store_publisher_key(pub_id, pub_id).await?;
            }

            store
                .update(
                    &name,
                    &manifest.version,
                    &manifest.description,
                    manifest.author.as_deref(),
                    &wasm_path,
                    &manifest_content,
                    &capabilities_json,
                    Some(&content_hash),
                    signature.as_deref(),
                    publisher_id.as_deref(),
                )
                .await?;

            eprintln!("Skill '{}' updated to v{}.", name, manifest.version);
            Ok(())
        }
        SkillCommands::Sign {
            wasm_path,
            private_key_path,
        } => {
            let wasm_bytes = std::fs::read(&wasm_path)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;

            let keypair =
                blufio_skill::load_private_key_from_file(std::path::Path::new(&private_key_path))?;

            let content_hash = blufio_skill::compute_content_hash(&wasm_bytes);
            let signature = keypair.sign(&wasm_bytes);
            let sig_hex = blufio_skill::signature_to_hex(&signature);
            let publisher_id = keypair.public_hex();

            let sig_path = format!("{}.sig", wasm_path);
            let sig_content = format!(
                "publisher_id={}\ncontent_hash={}\nsignature={}\n",
                publisher_id, content_hash, sig_hex
            );
            std::fs::write(&sig_path, &sig_content)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;

            eprintln!("Signed: {}", wasm_path);
            eprintln!("  Publisher: {}...", &publisher_id[..16]);
            eprintln!("  Hash:      {}...", &content_hash[..16]);
            eprintln!("  Output:    {}", sig_path);
            Ok(())
        }
        SkillCommands::Keygen { output_dir } => {
            let keypair = blufio_skill::PublisherKeypair::generate();
            let dir = std::path::Path::new(&output_dir);
            let private_path = dir.join("publisher.key");
            let public_path = dir.join("publisher.pub");

            blufio_skill::save_keypair_to_file(&keypair, &private_path, &public_path)?;

            eprintln!("Publisher keypair generated:");
            eprintln!("  Private key: {}", private_path.display());
            eprintln!("  Public key:  {}", public_path.display());
            eprintln!("  Publisher ID: {}", keypair.public_hex());
            eprintln!();
            eprintln!("Keep your private key safe! Do not share it.");
            Ok(())
        }
        SkillCommands::Verify { name } => {
            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));
            let skill = store.get(&name).await?.ok_or_else(|| {
                blufio_core::BlufioError::skill_execution_msg(&format!(
                    "skill '{}' not installed",
                    name
                ))
            })?;

            // Read WASM file and verify hash.
            let wasm_bytes = std::fs::read(&skill.wasm_path)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;

            let actual_hash = blufio_skill::compute_content_hash(&wasm_bytes);

            // Hash verification.
            if let Some(ref stored_hash) = skill.content_hash {
                if actual_hash == *stored_hash {
                    eprintln!("  Hash:      PASS (SHA-256 matches)");
                } else {
                    eprintln!(
                        "  Hash:      FAIL (expected {}..., got {}...)",
                        &stored_hash[..12],
                        &actual_hash[..12]
                    );
                    return Err(blufio_core::BlufioError::Security(format!(
                        "skill '{}': content hash mismatch -- WASM may be tampered",
                        name
                    )));
                }
            } else {
                eprintln!("  Hash:      NONE (no stored hash)");
            }

            // Signature verification.
            if let Some(ref sig_hex) = skill.signature {
                let sig = blufio_skill::signature_from_hex(sig_hex)?;
                let pub_id = skill.publisher_id.as_ref().ok_or_else(|| {
                    blufio_core::BlufioError::Security(format!(
                        "skill '{}': has signature but no publisher_id",
                        name
                    ))
                })?;
                let pubkey_bytes = hex::decode(pub_id).map_err(|e| {
                    blufio_core::BlufioError::Security(format!("invalid publisher_id hex: {e}"))
                })?;
                let pubkey_array: [u8; 32] = pubkey_bytes.try_into().map_err(|_| {
                    blufio_core::BlufioError::Security(
                        "publisher_id must be exactly 32 bytes".to_string(),
                    )
                })?;
                let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_array)
                    .map_err(|e| {
                        blufio_core::BlufioError::Security(format!("invalid publisher key: {e}"))
                    })?;
                blufio_skill::PublisherKeypair::verify_signature(
                    &verifying_key,
                    &wasm_bytes,
                    &sig,
                )?;
                eprintln!("  Signature: PASS (Ed25519 verified)");
                eprintln!("  Publisher: {}...", &pub_id[..12.min(pub_id.len())]);
            } else {
                eprintln!("  Signature: NONE (unsigned skill)");
            }

            eprintln!("Skill '{}' verification complete.", name);
            Ok(())
        }
        SkillCommands::Info { name } => {
            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));
            let skill = store.get(&name).await?.ok_or_else(|| {
                blufio_core::BlufioError::skill_execution_msg(&format!(
                    "skill '{}' not installed",
                    name
                ))
            })?;

            println!("Name:         {}", skill.name);
            println!("Version:      {}", skill.version);
            println!("Description:  {}", skill.description);
            if let Some(ref author) = skill.author {
                println!("Author:       {}", author);
            }
            println!("WASM path:    {}", skill.wasm_path);
            println!("Status:       {}", skill.verification_status);
            if let Some(ref hash) = skill.content_hash {
                println!("Content hash: {}", hash);
            }
            if let Some(ref sig) = skill.signature {
                let truncated = if sig.len() > 32 {
                    format!("{}...", &sig[..32])
                } else {
                    sig.clone()
                };
                println!("Signature:    {}", truncated);
            }
            if let Some(ref pub_id) = skill.publisher_id {
                println!("Publisher ID: {}", pub_id);
            }
            println!("Installed:    {}", skill.installed_at);
            println!("Updated:      {}", skill.updated_at);
            println!("Capabilities: {}", skill.capabilities_json);
            Ok(())
        }
    }
}

/// Parse a .sig file content into (publisher_id, content_hash, signature).
pub(crate) fn parse_sig_file(
    content: &str,
) -> Result<(String, String, String), blufio_core::BlufioError> {
    let mut publisher_id = None;
    let mut content_hash = None;
    let mut signature = None;

    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("publisher_id=") {
            publisher_id = Some(val.to_string());
        } else if let Some(val) = line.strip_prefix("content_hash=") {
            content_hash = Some(val.to_string());
        } else if let Some(val) = line.strip_prefix("signature=") {
            signature = Some(val.to_string());
        }
    }

    Ok((
        publisher_id.ok_or_else(|| {
            blufio_core::BlufioError::Security("signature file missing publisher_id".to_string())
        })?,
        content_hash.ok_or_else(|| {
            blufio_core::BlufioError::Security("signature file missing content_hash".to_string())
        })?,
        signature.ok_or_else(|| {
            blufio_core::BlufioError::Security("signature file missing signature".to_string())
        })?,
    ))
}
