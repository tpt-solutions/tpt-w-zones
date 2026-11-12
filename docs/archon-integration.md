# Archon integration: study notes

Written for: maintainers planning Phase 6 (`zones-archon-bridge`).

## What was read

Source: `tpt-solutions/tpt-archon` at its default branch, read on 2026-11-12: `spec.txt`,
`TODO.md`, and `crates/tpt-archon-bridge/src`.

## Findings

1. **No wire protocol yet.** The bridge crate (`tpt-archon-bridge`) is a Rust API: `Capability`,
   `CapabilityIssuer`, `Right`, `Resource`, `UnifiedPageCache`, `CapabilityGrant`. These are
   in-process types for memory and page access. `TODO.md` lists the wire protocol as Phase 8
   work, and says it is not done.
2. **IPC is capability-typed.** The spec describes messages that carry capabilities. Rights
   are unforgeable and revocable. Services run in user space and are isolated.
3. **Service model exists in principle.** "Filesystems, network stacks and drivers run in
   user space as isolated services." Nothing in the spec or code describes a window-management
   or compositor service.
4. **The network example is a transport precedent only.** `out-archon-pgwire` speaks the
   PostgreSQL wire protocol over TCP with no authentication. That is a database protocol, not
   a service-IPC endpoint for us.

Conclusion: there is no Archon endpoint to connect to today. The `tpt-archon-bridge` client
crate (Phase 6 TODO item) depends on a transport that Archon has not built.

## What is built now

`zones-archon-bridge` maps every `zones_core::ipc::Request` to an access level:

| Request                        | Access   |
| ------------------------------ | -------- |
| `hello`                        | none     |
| `list_windows`, `list_monitors`, `subscribe` | `Observe` |
| `snap_window`, `reload_config` | `Manage` |

`Grant` then answers whether a client holding a given grant may make a request. `Manage`
implies `Observe`. Tests cover each case.

## Planned mapping to Archon capabilities

Once Archon's transport exists:

- `Observe` and `Manage` become capability rights on a `Resource` that represents the
  tpt-zones service. The names are placeholders until Archon defines its resource types.
- The service grants a client capabilities at handshake, based on who it is. Capabilities
  are revocable, so a reload can narrow access without restarting the service.
- The same `zones_core::ipc` messages travel over Archon IPC. The JSON framing is replaced
  by Archon's typed messages. The mapping is one-to-one, so the Windows and Linux clients keep
  working.

## Blockers and what would unblock them

- A stable wire or IPC specification for service endpoints in Archon (currently Phase 8).
- A decision on whether the tpt-zones service runs as an Archon user-space service, or as a
  normal process that connects to Archon. Both are plausible; the spec leans toward the former.
- Archon's current Linux user-space path (io_uring, mmap) is the realistic first target. The
  spec's bare-metal goals are not relevant to this integration.

## Immediate next steps

1. Ask the Archon maintainers for the planned service-endpoint design, or follow the Phase 8 wire
   protocol work in `TODO.md` and revisit when it lands.
2. Keep `access_for` as the single place that decides request access, so the capability mapping
   changes in one file.
3. Do not implement a client against Archon's in-process API. That would couple tpt-zones to
   internal types that may change.
