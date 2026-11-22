# ADR-001: Centralized Coordinator with Planned HA Evolution

## Status

Accepted

## Context

RustyStore needs a single source of truth for cluster metadata: node registrations,
leases, file manifests, chunk locations, and replica placement. The initial design
uses a centralized Coordinator process backed by PostgreSQL.

## Decision

Use a centralized Coordinator (single process) for Phase 1, with metadata persisted
in PostgreSQL. The Coordinator is designed to be stateless-ish (all durable state in
Postgres) so it can be evolved toward a highly available coordinator architecture
(e.g., leader election via Patroni / Raft over the same Postgres, or a distributed
consensus layer) in a later phase.

## Rationale

- Simplicity: one coordinator avoids split-brain, consensus complexity, and
  distributed transactions in Phase 1.
- Correctness first: metadata mutations run in Postgres transactions; incomplete
  uploads are never marked committed.
- Evolvability: because metadata lives in Postgres (not coordinator RAM), a future
  HA coordinator can be another process reading the same DB, with leader election
  layered on top.
- Single Coordinator is an acceptable SPOF for Phase 1; new uploads require it, but
  existing chunks remain readable from storage nodes during a coordinator outage.

## Alternatives considered

1. **Fully distributed metadata (no coordinator)** — Rejected: placement and
   replication-factor enforcement need a global view; hard to guarantee N distinct
   replicas without coordination.
2. **Coordinator + etcd/Consul** — Rejected for Phase 1: adds an operational
   dependency; Postgres already satisfies durability needs.
3. **Leader election from day one** — Rejected: overengineering before the core
   data path is proven.

## Consequences

- Single point of failure for new writes until HA coordinator is implemented.
- Storage nodes can serve existing chunks independently of the coordinator.
- Recovery scheduling runs in the coordinator process; bounded concurrency prevents
  recovery storms.

## Related

- Phase 10: Production Hardening (HA coordinator, TLS, auth).
