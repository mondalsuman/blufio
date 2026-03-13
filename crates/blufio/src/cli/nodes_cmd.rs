// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Node management CLI handlers for `blufio nodes` subcommands.

#[cfg(feature = "node")]
use crate::{NodeGroupCommands, NodesCommands};

/// Handle `blufio nodes <action>` subcommands.
#[cfg(feature = "node")]
pub(crate) async fn handle_nodes_command(
    config: &blufio_config::model::BlufioConfig,
    action: NodesCommands,
) -> Result<(), blufio_core::BlufioError> {
    use std::sync::Arc;

    let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
    let store = Arc::new(blufio_node::NodeStore::new(conn));
    let event_bus = Arc::new(blufio_bus::EventBus::new(128));
    let conn_manager =
        blufio_node::ConnectionManager::new(store.clone(), event_bus.clone(), config.node.clone());

    match action {
        NodesCommands::List { json } => {
            let nodes = conn_manager
                .list_nodes_with_state()
                .await
                .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?;
            if json {
                println!(
                    "{}",
                    blufio_node::format_nodes_json(&nodes)
                        .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?
                );
            } else {
                print!("{}", blufio_node::format_nodes_table(&nodes));
            }
        }
        NodesCommands::Pair { token: token_mode } => {
            let keypair = Arc::new(blufio_auth_keypair::DeviceKeypair::generate());
            let pairing_mgr =
                blufio_node::PairingManager::new(keypair, store.clone(), event_bus.clone());
            let host = &config.gateway.host;
            let port = config.node.listen_port;
            let (pairing_token, qr_display) = pairing_mgr.initiate_pairing(host, port);
            if token_mode {
                println!("Pairing token: {}", pairing_token.value);
                println!("Connect to: ws://{}:{}/nodes/pair", host, port);
            } else {
                println!("{qr_display}");
            }
            println!("\nToken expires in 15 minutes. Waiting for peer connection...");
            // Note: Full interactive pairing requires a running serve instance.
            // This command displays the token/QR for use with a running server.
        }
        NodesCommands::Remove { node_id } => {
            let removed = store
                .remove_pairing(&node_id)
                .await
                .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?;
            if removed {
                println!("Node '{node_id}' removed.");
            } else {
                eprintln!("Node '{node_id}' not found.");
                std::process::exit(1);
            }
        }
        NodesCommands::Group {
            action: group_action,
        } => match group_action {
            NodeGroupCommands::Create { name, nodes } => {
                blufio_node::create_group(&store, &name, &nodes)
                    .await
                    .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?;
                println!("Group '{}' created with {} node(s).", name, nodes.len());
            }
            NodeGroupCommands::Delete { name } => {
                let deleted = blufio_node::delete_group(&store, &name)
                    .await
                    .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?;
                if deleted {
                    println!("Group '{name}' deleted.");
                } else {
                    eprintln!("Group '{name}' not found.");
                    std::process::exit(1);
                }
            }
            NodeGroupCommands::List => {
                let groups = blufio_node::list_groups(&store)
                    .await
                    .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?;
                print!("{}", blufio_node::format_groups_table(&groups));
            }
        },
        NodesCommands::Exec { targets, command } => {
            if command.is_empty() {
                return Err(blufio_core::BlufioError::Internal(
                    "no command specified for exec".to_string(),
                ));
            }
            let cmd = &command[0];
            let args: Vec<String> = command[1..].to_vec();
            blufio_node::exec_on_nodes(&conn_manager, &store, &targets, cmd, &args)
                .await
                .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?;
            println!("Exec request sent to {} target(s).", targets.len());
        }
    }

    Ok(())
}
