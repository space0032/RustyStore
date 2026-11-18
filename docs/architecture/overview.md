# Architecture

## Overall

```mermaid
flowchart TD
    Client[Client / curl] -->|HTTP multipart / GET| GW[Gateway / Axum]
    GW -->|gRPC| C[Coordinator]
    C -->|SQL| DB[(PostgreSQL metadata)]
    GW -->|gRPC streaming| SN1[Storage Node 1]
    GW -->|gRPC streaming| SN2[Storage Node 2]
    GW -->|gRPC streaming| SN3[Storage Node 3]
    SN1 -->|local disk| Disk1[(/data/chunks)]
    SN2 -->|local disk| Disk2[(/data/chunks)]
    SN3 -->|local disk| Disk3[(/data/chunks)]
    SN1 -.->|heartbeat / lease| C
    SN2 -.->|heartbeat / lease| C
    SN3 -.->|heartbeat / lease| C
    C -.->|re-replication / recovery| SN1
    C -.->|re-replication / recovery| SN2
    C -.->|re-replication / recovery| SN3
```

## Upload workflow

```mermaid
sequenceDiagram
    participant CL as Client
    participant GW as Gateway
    participant CRD as Coordinator
    participant DB as PostgreSQL
    participant SN1 as Storage Node 1
    participant SN2 as Storage Node 2
    participant SN3 as Storage Node 3

    CL->>GW: POST /api/v1/files (multipart stream)
    GW->>CRD: CreateUploadSession(size, chunk_size, rf)
    CRD-->>GW: upload_id, chunk_plan[] {chunk_id, node_ids[]}
    loop for each chunk
        GW->>GW: hash chunk (SHA-256), bounded buffer
        GW->>SN1: StoreChunk(stream, chunk_id, hash)
        GW->>SN2: StoreChunk(stream, chunk_id, hash)
        GW->>SN3: StoreChunk(stream, chunk_id, hash)
        SN1-->>GW: Ack(durable=true)
        SN2-->>GW: Ack(durable=true)
        SN3-->>GW: Ack(durable=true)
    end
    GW->>CRD: CompleteUpload(upload_id, manifest)
    CRD->>DB: INSERT file, chunks, replicas (txn, status=committed)
    CRD-->>GW: file_id
    GW-->>CL: 201 {file_id}
```

## Download workflow

```mermaid
sequenceDiagram
    participant CL as Client
    participant GW as Gateway
    participant CRD as Coordinator
    participant SN1 as Storage Node 1
    participant SN2 as Storage Node 2

    CL->>GW: GET /api/v1/files/{file_id}/download
    GW->>CRD: GetManifest(file_id)
    CRD-->>GW: chunks[] ordered with locations + sha256
    loop per chunk (bounded concurrency)
        GW->>SNx: RetrieveChunk(chunk_id, expected_sha256)
        SNx-->>GW: stream chunk frames
        GW->>GW: verify SHA-256
    end
    GW-->>CL: stream ordered reconstruction
```

## Node registration & heartbeat

```mermaid
sequenceDiagram
    participant SN as Storage Node
    participant CRD as Coordinator
    participant DB as PostgreSQL

    SN->>CRD: Register(addr, grpc_addr, capacity)
    CRD->>DB: INSERT storage_nodes (lease_expiry)
    CRD-->>SN: node_id, lease_ttl_ms
    loop every lease_ttl/2
        SN->>CRD: Heartbeat(node_id, disk_space)
        CRD->>DB: UPDATE lease_expiry, last_seen
        CRD-->>SN: Ack(healthy)
    end
    alt lease expires
        CRD->>DB: UPDATE healthy=false
        CRD->>CRD: detect under-replicated chunks
        CRD->>CRD: schedule recovery tasks
    end
```

## Replication workflow

```mermaid
sequenceDiagram
    participant CRD as Coordinator
    participant SRC as Storage Node (source)
    participant DST as Storage Node (target)
    participant DB as PostgreSQL

    CRD->>DB: claim under-replicated task
    CRD->>SRC: RetrieveChunk(chunk_id)
    SRC-->>CRD: stream chunk frames
    CRD->>DST: StoreChunk(chunk_id, expected_sha256)
    DST->>DST: atomic write (temp + fsync + rename) + verify hash
    DST-->>CRD: Ack(durable=true)
    CRD->>DB: mark task done + insert chunk_replica (chunk_id, node_id=DST, durable=true)
```

## Node failure & recovery

```mermaid
flowchart TD
    HB[Heartbeat missed]
    LEASE[Lease expires?]
    UNHEALTHY[Mark node unhealthy]
    DETECT[Detect under-replicated chunks]
    SCHED[Schedule recovery tasks (bounded concurrency)]
    COPY[Copy replica from healthy source node]
    VERIFY[Verify SHA-256 + insert durable replica]
    RF[Replication factor restored]

    HB --> LEASE
    LEASE -- no --> H[Node stays healthy]
    LEASE -- yes --> UNHEALTHY
    UNHEALTHY --> DETECT
    DETECT --> SCHED
    SCHED --> COPY
    COPY --> VERIFY
    VERIFY --> RF
```
