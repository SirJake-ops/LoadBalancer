# Rust Load Balancer

TCP load balancer written in Rust with Tokio. The project currently supports a transparent single-backend proxy path, TOML configuration, per-connection Tokio tasks, and graceful shutdown handling.

The longer-term goal is to grow this into a configurable load balancer with backend pool state, health checks, and pluggable balancing strategies such as round-robin and least-connections.

## Current Features

- Loads runtime configuration from TOML.
- Binds a TCP listener to the configured address.
- Proxies client TCP connections to the first configured backend.
- Copies bytes in both directions with `tokio::io::copy_bidirectional`.
- Handles each accepted connection in its own Tokio task.
- Tracks spawned connection tasks during shutdown with `JoinSet`.
- Stops accepting new connections on `Ctrl-C`.
- Includes unit tests for config parsing, proxy behavior, concurrent connections, and shutdown.

## Requirements

- Rust toolchain with edition 2024 support.
- Local backend TCP server if you want to manually exercise proxying.

## Configuration

The default config path is `config.toml`.

```toml
listener_address = "127.0.0.1:8080"
strategy = "round_robin"

[[backend_list]]
backend_address = "127.0.0.1:8081"
backend_id = "1"
weight = 1

[[backend_list]]
backend_address = "127.0.0.1:8082"
backend_id = "2"
weight = 2

[health_check_interval]
interval_seconds = 10
timeout_seconds = 5
```

At the moment, the server proxies to the first backend in `backend_list`. Multi-backend selection is tracked in the backlog.

## Running

```bash
cargo run
```

Use a custom config file:

```bash
cargo run -- --config path/to/config.toml
```

Show CLI options:

```bash
cargo run -- --help
```

Current options:

```text
-c, --config <CONFIG>    [default: config.toml]
-d, --daemon
-v, --verbose <VERBOSE>  [default: 1]
```

`--daemon` and `--verbose` are parsed but not fully wired into runtime behavior yet.

## Testing

Run the full test suite:

```bash
cargo test
```

Run focused tests:

```bash
cargo test config
cargo test proxy
cargo test server
```

## Project Layout

```text
src/
  main.rs              CLI entry point
  lib.rs               Library module wiring
  cli.rs               clap argument parsing
  config.rs            TOML config types and loading
  server.rs            TCP listener, task spawning, shutdown
  proxy.rs             Client-to-backend TCP proxying
  pool.rs              Early connection pool state
  health.rs            Health-check placeholder
  telemetry.rs         Telemetry placeholder
  balancing/           Balancing strategy modules

docs/
  backlog.md           Project backlog and milestone status
  system_design.md     Design notes and target architecture
```

## Key Upcoming Features

**Backend Pool State**

Track each configured backend at runtime, including its health status and active connection count. This will give routing, health checks, and observability a shared view of backend availability.

**Balancing Strategy Trait**

Introduce a small strategy interface for selecting a backend from the available candidates. This keeps algorithms such as round-robin and least-connections testable and swappable.

**Round-Robin Routing**

Distribute new connections across healthy backends in order instead of always proxying to the first configured backend.

**Backend Health Checks**

Periodically verify backend availability and remove unhealthy backends from the candidate set until they recover.

**Structured Telemetry**

Replace ad hoc `println!` logging with structured logs for accepted connections, selected backends, proxy errors, shutdown, and later metrics.

## Development Notes

The project is still evolving. Some CLI flags and modules are present ahead of their full implementation, and the current build may report warnings for code that is being introduced for upcoming backlog items.
