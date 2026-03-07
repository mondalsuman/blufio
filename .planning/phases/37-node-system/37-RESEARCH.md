# Phase 37: Node System - Research

**Researched:** 2026-03-07
**Domain:** Distributed device mesh with Ed25519 mutual authentication, WebSocket connectivity, fleet management CLI, and approval routing
**Confidence:** HIGH

## Summary

Phase 37 implements a paired device mesh system ("Node System") that allows multiple Blufio instances to discover each other, authenticate mutually via Ed25519, maintain persistent WebSocket connections, and route approval requests across the fleet. The phase builds heavily on existing crate infrastructure: `blufio-auth-keypair` provides Ed25519 sign/verify primitives, `blufio-bus` provides the event system for node lifecycle events, `blufio-gateway` provides the axum WebSocket server pattern, and `blufio-config` provides the TOML config model pattern.

The core technical challenge is the **pairing flow** -- generating a time-limited, single-use token, exchanging public keys mutually, and establishing trust that persists across restarts. The secondary challenge is maintaining WebSocket connections with heartbeat monitoring and reconnection logic. The tertiary challenge is broadcasting approval requests to all connected devices and handling first-wins semantics.

**Primary recommendation:** Create a new `blufio-node` crate encapsulating pairing state machine, node connection manager, heartbeat monitor, fleet store (SQLite), and approval broadcaster. Use `tokio-tungstenite` for WebSocket client connections (connecting TO peers) and the existing axum WebSocket server for accepting connections FROM peers. Use the `qrcode` crate for terminal QR rendering. Use `sysinfo` for memory reporting and `starship-battery` (or manual platform API) for battery status.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- QR code (unicode blocks in terminal) for local/mobile devices, with CLI token fallback for headless/remote pairing
- Mutual confirmation: both devices display a fingerprint/code and both operators must confirm (prevents MITM)
- Pairing tokens expire after 15 minutes, single-use; silent expiry with clear error message, user runs `blufio nodes pair` again
- Pairings persist in SQLite/vault storage; nodes auto-reconnect on restart using stored Ed25519 keys
- Extensible capability registry: core capabilities (camera, screen, location, exec) built-in as enum variants, plus custom string capabilities for plugins/future use
- Manual declaration in TOML config (e.g., `capabilities = ["exec", "screen"]`); no auto-detection
- Capabilities enforce permissions: if a node doesn't declare a capability, it cannot receive requests for it
- "exec" capability means both shell commands (`blufio nodes exec node-2 -- ls /tmp`) and Blufio operations (skills, sessions, agent tasks) routed from other nodes
- `blufio nodes list`: table format by default (Name, Status, Capabilities, Battery, Memory), `--json` flag for machine parsing
- `blufio nodes group`: named groups (create, delete, list) -- `blufio nodes group create mobile --nodes node-1,node-3`, then target groups with `blufio nodes exec mobile -- ...`
- `blufio nodes exec`: streamed per-node output as results arrive, prefixed with node name (`[node-1] output...`)
- Node status (battery, memory, connectivity) reported via heartbeat messages over WebSocket; `nodes list` shows last-known state
- Configurable per-action type in TOML config -- operator specifies which action types require broadcast approval
- First-wins semantics: first device to approve/deny wins, other devices see "Already handled by [device]"
- Timeout then deny: after configurable timeout (default 5 min), auto-deny the pending action; user can retry
- Approval requests pushed as WebSocket messages to connected devices; devices display via their channel integration

### Claude's Discretion
- WebSocket reconnection strategy and backoff parameters
- Heartbeat interval and stale detection thresholds
- QR code encoding format and terminal rendering details
- Node ID generation scheme
- Internal message serialization format for node-to-node communication
- Exact TOML config schema structure for node settings

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| NODE-01 | Node pairing via Ed25519 mutual authentication (QR or shared token) | Existing `DeviceKeypair` in `blufio-auth-keypair` provides sign/verify_strict; `qrcode` crate renders Unicode in terminal; pairing state machine handles token generation, exchange, mutual confirmation |
| NODE-02 | Node connection via WebSocket with capability declaration (camera, screen, location, exec) | Axum WebSocket server pattern from `blufio-gateway/ws.rs` for accepting connections; `tokio-tungstenite` for client-side outbound connections; capability enum with custom string extension |
| NODE-03 | Node heartbeat monitoring (battery, memory, connectivity, stale detection) | `sysinfo` crate for memory; `starship-battery` for battery level; heartbeat interval pattern from `blufio-agent/heartbeat.rs`; stale detection via timestamp comparison |
| NODE-04 | Node fleet management CLI (blufio nodes list/group/exec) | Clap subcommand pattern from existing Skill/Plugin/Config commands; `println!` format padding for table output; `--json` flag via serde_json serialization |
| NODE-05 | Approval routing broadcasts to all connected operator devices | Event bus subscription for approval events; WebSocket broadcast to all connected node senders; first-wins via atomic state (DashMap or tokio Mutex); timeout via `tokio::time::timeout` |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ed25519-dalek | 2.1 | Ed25519 mutual authentication | Already in workspace; provides sign/verify_strict for node keypairs |
| axum | 0.8 | WebSocket server (accepting node connections) | Already in workspace with `ws` feature; handles upgrade, split, send/recv |
| tokio-tungstenite | 0.28 | WebSocket client (connecting to peer nodes) | Standard Rust async WebSocket client; pairs with axum server-side |
| qrcode | 0.14 | QR code generation for terminal pairing | Standard Rust QR encoder; renders to Unicode `Dense1x2` half-blocks |
| sysinfo | 0.33+ | Memory and system info for heartbeat status | Cross-platform memory (total/used); no external dependencies needed |
| tokio-rusqlite | 0.7 | Async SQLite for node/pairing storage | Already in workspace; matches existing store patterns |
| serde / serde_json | 1.x | Node message serialization | Already in workspace; JSON for WebSocket message payloads |
| dashmap | 6 | Concurrent map for connected node senders | Already in workspace; used in `GatewayState` for ws_senders |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| starship-battery | 0.9+ | Battery level/state reporting | For nodes that are laptops/mobile; optional behind feature flag |
| base64 | 0.22 | Encoding pairing tokens for QR payloads | Only if pairing token includes binary data in QR |
| futures | 0.3 | Stream combinators for WebSocket message handling | Already in workspace |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| tokio-tungstenite (client) | axum client (none exists) | Axum is server-only; tokio-tungstenite is the standard client |
| sysinfo | sys-info | sysinfo has broader cross-platform support, more actively maintained |
| starship-battery | battery (original) | starship-battery is the maintained fork; original is abandoned |
| Custom QR | qr_code crate | qrcode crate is more mature with 8.9M downloads |

**Installation:**
```bash
# Add to workspace Cargo.toml [workspace.dependencies]
tokio-tungstenite = "0.28"
qrcode = { version = "0.14", default-features = false }
sysinfo = { version = "0.33", default-features = false }
starship-battery = "0.9"
```

## Architecture Patterns

### Recommended Project Structure
```
crates/blufio-node/
├── Cargo.toml
└── src/
    ├── lib.rs              # Public API: NodeManager, NodeStore, types
    ├── pairing.rs          # Pairing state machine, token generation, mutual auth
    ├── connection.rs       # WebSocket connection manager (client + server handlers)
    ├── heartbeat.rs        # Heartbeat send/receive, stale detection, system metrics
    ├── fleet.rs            # Fleet operations: list, group, exec
    ├── approval.rs         # Approval routing, broadcast, first-wins resolution
    ├── store.rs            # SQLite storage for pairings, groups, node state
    ├── types.rs            # NodeId, NodeInfo, Capability, NodeMessage, etc.
    └── config.rs           # TOML config structs (NodeConfig, ApprovalConfig)
```

### Pattern 1: Pairing State Machine
**What:** A state machine that drives the pairing flow from token generation through mutual authentication to persisted trust.
**When to use:** Every time `blufio nodes pair` is invoked.
**Example:**
```rust
// Pairing flow states
enum PairingState {
    /// Token generated, waiting for peer to connect
    AwaitingPeer {
        token: PairingToken,
        our_keypair: DeviceKeypair,
        expires_at: Instant,
    },
    /// Peer connected, exchanging public keys
    KeyExchange {
        our_keypair: DeviceKeypair,
        peer_public: VerifyingKey,
    },
    /// Both sides display fingerprint for mutual confirmation
    AwaitingConfirmation {
        our_keypair: DeviceKeypair,
        peer_public: VerifyingKey,
        fingerprint: String,
    },
    /// Pairing complete, stored in DB
    Complete {
        node_id: NodeId,
        peer_public: VerifyingKey,
    },
    /// Pairing failed or expired
    Failed { reason: String },
}

struct PairingToken {
    /// Random token (32 bytes, hex-encoded)
    value: String,
    /// When this token becomes invalid
    expires_at: Instant,
    /// Whether this token has been used
    used: bool,
}

impl PairingToken {
    fn generate() -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        Self {
            value: hex::encode(bytes),
            expires_at: Instant::now() + Duration::from_secs(15 * 60),
            used: false,
        }
    }

    fn is_valid(&self) -> bool {
        !self.used && Instant::now() < self.expires_at
    }
}
```

### Pattern 2: Node WebSocket Message Protocol
**What:** JSON-serialized messages exchanged over WebSocket between paired nodes.
**When to use:** All node-to-node communication after pairing.
**Example:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NodeMessage {
    // --- Pairing ---
    PairRequest {
        token: String,
        public_key: String,  // hex-encoded Ed25519 public key
    },
    PairResponse {
        public_key: String,
        signature: String,   // signature over shared challenge
    },
    PairConfirm {
        fingerprint: String,
        confirmed: bool,
    },

    // --- Connection lifecycle ---
    Hello {
        node_id: String,
        capabilities: Vec<String>,
        version: String,
    },
    Heartbeat {
        node_id: String,
        battery_percent: Option<u8>,
        memory_used_mb: u64,
        memory_total_mb: u64,
        uptime_secs: u64,
    },

    // --- Approval routing ---
    ApprovalRequest {
        request_id: String,
        action_type: String,
        description: String,
        timeout_secs: u64,
    },
    ApprovalResponse {
        request_id: String,
        approved: bool,
        responder_node: String,
    },
    ApprovalHandled {
        request_id: String,
        handled_by: String,
    },

    // --- Exec routing ---
    ExecRequest {
        request_id: String,
        command: String,
        args: Vec<String>,
    },
    ExecOutput {
        request_id: String,
        node_id: String,
        stream: String,  // "stdout" or "stderr"
        data: String,
    },
    ExecComplete {
        request_id: String,
        node_id: String,
        exit_code: i32,
    },
}
```

### Pattern 3: Connection Manager with Reconnection
**What:** Manages outbound WebSocket connections to paired nodes with exponential backoff reconnection.
**When to use:** On startup (reconnect to all known peers) and when connections drop.
**Example:**
```rust
struct ConnectionManager {
    /// Active connections: node_id -> sender channel
    connections: Arc<DashMap<String, mpsc::Sender<NodeMessage>>>,
    /// Known peers from the store
    store: Arc<NodeStore>,
    /// Event bus for publishing node events
    event_bus: Arc<EventBus>,
}

impl ConnectionManager {
    async fn reconnect_with_backoff(&self, peer: &PairedNode) {
        let mut delay = Duration::from_secs(1);
        let max_delay = Duration::from_secs(60);
        let mut attempts = 0;

        loop {
            match self.connect_to_peer(peer).await {
                Ok(_) => {
                    tracing::info!(node_id = %peer.node_id, "reconnected to peer");
                    break;
                }
                Err(e) => {
                    attempts += 1;
                    tracing::warn!(
                        node_id = %peer.node_id,
                        attempt = attempts,
                        delay_secs = delay.as_secs(),
                        "reconnection failed: {e}"
                    );
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(max_delay);
                }
            }
        }
    }
}
```

### Pattern 4: Capability Enum with Custom Extension
**What:** Core capabilities as enum variants with a Custom(String) escape hatch for plugins.
**When to use:** Capability declaration in config and capability checks at request routing time.
**Example:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
enum NodeCapability {
    Camera,
    Screen,
    Location,
    Exec,
    #[serde(untagged)]
    Custom(String),
}

impl std::fmt::Display for NodeCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Camera => write!(f, "camera"),
            Self::Screen => write!(f, "screen"),
            Self::Location => write!(f, "location"),
            Self::Exec => write!(f, "exec"),
            Self::Custom(s) => write!(f, "{s}"),
        }
    }
}
```

### Anti-Patterns to Avoid
- **Polling for pairing completion:** Use channel-based notification (tokio oneshot/mpsc) instead of sleep loops checking pairing state
- **Shared mutable state without DashMap:** Do not use `Arc<Mutex<HashMap>>` for the connection registry; DashMap provides better concurrent performance and matches the existing gateway pattern
- **Blocking system info calls on the async runtime:** The `sysinfo` crate's refresh methods can block; run them in `tokio::task::spawn_blocking` or a dedicated thread
- **Storing raw private keys in SQLite:** Private keys for paired nodes should be stored in the vault (encrypted), not plaintext in SQLite; SQLite stores the public key, node_id, name, capabilities, and metadata

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| QR code generation | Custom QR encoder | `qrcode` crate with `Dense1x2` renderer | QR encoding has complex error correction; 8.9M downloads proves reliability |
| WebSocket client | Raw TCP + HTTP upgrade | `tokio-tungstenite` | Handles masking, framing, ping/pong, close handshake correctly |
| System memory info | Read `/proc/meminfo` manually | `sysinfo` crate | Cross-platform (Linux/macOS/Windows); handles OS-specific APIs |
| Battery level | Platform-specific `ioctl`/`IOKit` | `starship-battery` | Abstracts Linux, macOS, Windows, FreeBSD battery APIs |
| Exponential backoff | Manual delay doubling | Simple inline impl (3 lines) | Too simple to warrant a dependency; just `delay = (delay * 2).min(max)` |
| Ed25519 key exchange | Custom Diffie-Hellman | Existing `DeviceKeypair` sign/verify_strict | Already proven in the codebase; mutual signing proves identity |

**Key insight:** The pairing protocol is the most security-critical component. Use Ed25519 mutual signing (not DH key exchange) because the goal is **identity verification** not **key agreement** -- both nodes already have their own keypairs and need to prove they hold the corresponding private key.

## Common Pitfalls

### Pitfall 1: Pairing Token Replay
**What goes wrong:** An attacker captures a pairing token and replays it after the legitimate pairing completes.
**Why it happens:** Token validation only checks expiry, not single-use status.
**How to avoid:** Mark the token as `used = true` immediately upon first connection attempt. Store consumed tokens until expiry so replayed tokens are rejected. Clear expired tokens periodically.
**Warning signs:** Multiple pairing connections from different IPs using the same token.

### Pitfall 2: MITM During Pairing
**What goes wrong:** Attacker intercepts the pairing flow and inserts their own public key.
**Why it happens:** No out-of-band verification of the peer's identity.
**How to avoid:** The mutual confirmation step (both operators see and confirm a fingerprint derived from both public keys) prevents MITM. The fingerprint should be a hash of `sort([our_pubkey, their_pubkey])` so both sides compute the same value.
**Warning signs:** Fingerprints don't match between the two devices.

### Pitfall 3: WebSocket Reconnection Storm
**What goes wrong:** All nodes try to reconnect simultaneously after a network partition heals, overwhelming the server.
**Why it happens:** Fixed reconnection delays without jitter.
**How to avoid:** Add random jitter to the backoff delay: `delay = (delay * 2).min(max) + random(0..delay/4)`. This spreads reconnection attempts over time.
**Warning signs:** CPU/connection spikes after network recovery.

### Pitfall 4: Stale Heartbeat False Positives
**What goes wrong:** A node is marked stale (disconnected) even though it's alive, just temporarily slow.
**Why it happens:** Heartbeat timeout too aggressive, or clock skew between nodes.
**How to avoid:** Use a generous stale threshold (3x the heartbeat interval). Track last-seen timestamp locally (don't compare clocks across nodes). Default heartbeat interval of 30 seconds with stale threshold of 90 seconds.
**Warning signs:** Nodes flapping between online/offline status.

### Pitfall 5: Approval Race Condition
**What goes wrong:** Two devices approve the same request simultaneously, both think they won.
**Why it happens:** No atomic compare-and-swap on the approval state.
**How to avoid:** Use an atomic state transition: `DashMap::entry(request_id).or_insert(Pending)` then `compare_exchange` to `Approved`. The node that successfully transitions the state is the winner; others receive `ApprovalHandled` messages.
**Warning signs:** Duplicate approval side-effects (action executed twice).

### Pitfall 6: sysinfo Blocking the Async Runtime
**What goes wrong:** Calling `System::refresh_memory()` on the tokio runtime blocks the event loop.
**Why it happens:** sysinfo reads from `/proc` or platform APIs synchronously.
**How to avoid:** Collect system metrics in a `tokio::task::spawn_blocking` closure, or on a dedicated metrics thread that runs on a fixed interval and stores results in an `Arc<RwLock<SystemMetrics>>`.
**Warning signs:** Heartbeat send delays, WebSocket message latency spikes.

## Code Examples

### QR Code Terminal Rendering for Pairing
```rust
// Source: Context7 /kennytm/qrcode-rust
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;

fn render_pairing_qr(token: &str, host: &str, port: u16) -> String {
    // Encode pairing URI: blufio-pair://<host>:<port>?token=<token>
    let uri = format!("blufio-pair://{}:{}?token={}", host, port, token);
    let code = QrCode::new(uri.as_bytes()).expect("QR encoding failed");

    // Render as compact Unicode (half-block characters)
    // Invert colors for dark terminals
    code.render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .quiet_zone(true)
        .build()
}
```

### WebSocket Client Connection (tokio-tungstenite)
```rust
// Source: Context7 /snapview/tokio-tungstenite
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use futures_util::{StreamExt, SinkExt};

async fn connect_to_node(
    url: &str,
    our_public_key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut request = url.into_client_request()?;
    let headers = request.headers_mut();
    headers.insert("X-Node-PublicKey", HeaderValue::from_str(our_public_key)?);

    let (ws_stream, _response) = connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();

    // Send Hello with capabilities
    let hello = serde_json::json!({
        "type": "hello",
        "node_id": "node-abc",
        "capabilities": ["exec", "screen"],
        "version": env!("CARGO_PKG_VERSION"),
    });
    write.send(tokio_tungstenite::tungstenite::Message::Text(
        serde_json::to_string(&hello)?
    )).await?;

    // Message receive loop
    while let Some(msg) = read.next().await {
        match msg? {
            tokio_tungstenite::tungstenite::Message::Text(text) => {
                let node_msg: serde_json::Value = serde_json::from_str(&text)?;
                // Handle message...
            }
            tokio_tungstenite::tungstenite::Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}
```

### Mutual Fingerprint for MITM Prevention
```rust
use sha2::{Sha256, Digest};

/// Compute a deterministic fingerprint from two public keys.
/// Both sides compute the same fingerprint regardless of who initiated.
fn compute_pairing_fingerprint(key_a: &[u8; 32], key_b: &[u8; 32]) -> String {
    // Sort keys so the fingerprint is order-independent
    let (first, second) = if key_a < key_b {
        (key_a, key_b)
    } else {
        (key_b, key_a)
    };

    let mut hasher = Sha256::new();
    hasher.update(first);
    hasher.update(second);
    let hash = hasher.finalize();

    // Display as 4 groups of 4 hex chars: ABCD-EFGH-IJKL-MNOP
    let hex = hex::encode(&hash[..8]);
    format!(
        "{}-{}-{}-{}",
        &hex[0..4], &hex[4..8], &hex[8..12], &hex[12..16]
    )
}
```

### System Metrics Collection
```rust
// Source: Context7 /guillaumegomez/sysinfo
use sysinfo::System;

struct SystemMetrics {
    memory_used_mb: u64,
    memory_total_mb: u64,
    battery_percent: Option<u8>,
    uptime_secs: u64,
}

async fn collect_metrics() -> SystemMetrics {
    // Run in blocking context -- sysinfo reads /proc synchronously
    tokio::task::spawn_blocking(|| {
        let mut sys = System::new();
        sys.refresh_memory();

        let battery_percent = get_battery_percent();

        SystemMetrics {
            memory_used_mb: sys.used_memory() / (1024 * 1024),
            memory_total_mb: sys.total_memory() / (1024 * 1024),
            battery_percent,
            uptime_secs: System::uptime(),
        }
    })
    .await
    .unwrap_or(SystemMetrics {
        memory_used_mb: 0,
        memory_total_mb: 0,
        battery_percent: None,
        uptime_secs: 0,
    })
}

fn get_battery_percent() -> Option<u8> {
    // starship-battery for cross-platform battery info
    #[cfg(feature = "battery")]
    {
        use starship_battery::Manager;
        let manager = Manager::new().ok()?;
        let battery = manager.batteries().ok()?.next()?.ok()?;
        Some((battery.state_of_charge().value * 100.0) as u8)
    }
    #[cfg(not(feature = "battery"))]
    { None }
}
```

### SQLite Migration for Node Storage
```sql
-- V9__node_system.sql
CREATE TABLE IF NOT EXISTS node_pairings (
    node_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    public_key_hex TEXT NOT NULL,
    capabilities TEXT NOT NULL DEFAULT '[]',  -- JSON array
    paired_at TEXT NOT NULL,
    last_seen TEXT,
    endpoint TEXT  -- ws://host:port for reconnection
);

CREATE TABLE IF NOT EXISTS node_groups (
    group_name TEXT NOT NULL,
    node_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (group_name, node_id),
    FOREIGN KEY (node_id) REFERENCES node_pairings(node_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS pending_approvals (
    request_id TEXT PRIMARY KEY,
    action_type TEXT NOT NULL,
    description TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, approved, denied, expired
    handled_by TEXT,  -- node_id that handled it
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    resolved_at TEXT
);
```

### Node TOML Config Pattern
```toml
[node]
enabled = true
node_id = "my-laptop"              # User-friendly name
listen_port = 9877                  # WebSocket listener for incoming node connections
capabilities = ["exec", "screen"]  # Declared capabilities

[node.heartbeat]
interval_secs = 30                 # Heartbeat send interval
stale_threshold_secs = 90          # Mark node stale after 3 missed heartbeats

[node.reconnect]
initial_delay_secs = 1
max_delay_secs = 60
jitter = true

[node.approval]
broadcast_actions = ["shell_exec", "file_write", "skill_install"]
timeout_secs = 300                 # 5 minutes
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Custom WebSocket from scratch | axum (server) + tokio-tungstenite (client) | axum 0.7+ (2023) | Standard split; axum handles HTTP upgrade, tungstenite handles client |
| `battery` crate (abandoned) | `starship-battery` (maintained fork) | 2023 | Original `battery` crate unmaintained; starship fork actively maintained |
| sysinfo `new_all()` | sysinfo selective refresh (`refresh_memory()`) | sysinfo 0.30+ | Avoids expensive full system scan when only memory needed |

**Deprecated/outdated:**
- `battery` crate: Abandoned upstream; use `starship-battery` (maintained Starship fork)
- `sysinfo::new_all()`: Overly broad for our use case; use `System::new()` + selective `refresh_memory()`

## Open Questions

1. **Battery crate Rust version requirement**
   - What we know: starship-battery documentation mentions Rustc 1.89 minimum
   - What's unclear: Whether this is accurate or outdated (our workspace requires Rust 1.85)
   - Recommendation: Make battery reporting optional behind a feature flag. If the MSRV conflict is real, use a simpler platform-specific fallback or skip battery on incompatible targets. The core node system should work without battery info.

2. **Node-to-node endpoint discovery**
   - What we know: Paired nodes store endpoint URLs (ws://host:port) for reconnection
   - What's unclear: What happens when a node's IP changes (dynamic IP, laptop moves to different network)
   - Recommendation: On reconnection failure, fall back to event bus notification if both nodes are on the same Blufio instance group. For truly remote nodes, the user re-pairs. This is acceptable for v1.3.

3. **Private key storage for paired nodes**
   - What we know: The vault stores the device's own keypair; paired node public keys go in SQLite
   - What's unclear: Whether each pairing should generate a dedicated keypair or reuse the device keypair
   - Recommendation: Reuse the device's existing `DeviceKeypair`. The node proves its identity by signing challenges with its device key. No need for per-pairing keys -- the device keypair IS the node identity.

## Sources

### Primary (HIGH confidence)
- Context7 `/kennytm/qrcode-rust` - QR code generation API, Unicode rendering, Dense1x2 format
- Context7 `/snapview/tokio-tungstenite` - WebSocket client API, connect_async, message handling, error types
- Context7 `/tokio-rs/axum` - WebSocket server handler pattern, split, concurrent send/receive
- Context7 `/guillaumegomez/sysinfo` - System memory API, refresh patterns, cross-platform support
- Existing codebase: `blufio-auth-keypair` (Ed25519), `blufio-bus` (events), `blufio-gateway/ws.rs` (WebSocket pattern), `blufio-config/model.rs` (TOML config)

### Secondary (MEDIUM confidence)
- crates.io: tokio-tungstenite 0.28, qrcode 0.14.1, sysinfo 0.33+, starship-battery 0.9+
- WebSearch: battery crate MSRV requirements, starship-battery as maintained fork

### Tertiary (LOW confidence)
- starship-battery MSRV claim (1.89) -- needs validation against actual Cargo.toml; may be outdated documentation

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All core libraries verified via Context7 and existing codebase patterns
- Architecture: HIGH - Follows established patterns (DashMap, axum WS, SQLite stores, event bus, clap CLI)
- Pitfalls: HIGH - Security pitfalls (MITM, replay) well-understood; runtime pitfalls (sysinfo blocking) documented

**Research date:** 2026-03-07
**Valid until:** 2026-04-07 (stable domain; libraries are mature)
