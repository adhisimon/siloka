# SILOKA 

**WORK IN PROGRESS**

This application is still on very early development.

## Main Goal
To create an object storage platform that perform Erasure Coding (EC) but with flexibility of adding storage nodes.

MinIO and RustFS use EC, but they don't allow you to add new storage easily, not without creating a new pool. SeaweedFS and Garage allow you to add new storage easily, but they don't perform EC.

## Features
- No single point of failure (multiple master and storage nodes).
- Erasure Coding (EC) on the fly.
- Automatic zstd compression based on configuration or content type detection.
- Location (region, data center, rack, machine) awareness.
- Background maintenance and healing.

## Components
- master nodes (raft cluster): write using rust
- storage nodes: write using NodeJS at first, will be porting to rust next
- worker nodes: will do computation intensive task like EC encoding/decoding
- gateway:
  - s3 gateway
  - simple http GET/POST/DELETE

## Metadata
Metadata stored on a DBMS. First consideration is to use MariaDB. Why MariaDB? Why not PostgreSQL? Because it is so much easier to create MariaDB cluster using MariaDB Galera. There is no easy method to create a PostgreSQL cluster.

Why cluster is a consideration? Because I don't want a single point of failure.

I don't have plan to implement multiple DBMS type.

Redis will not use for metadata storage because I don't want storage capacity limited by RAM. Another reason is I can not get reverse-index on redis. Maybe I will use redis on this project, but just for caching (although I think memcache will be more suitable)

## Challenges
- I need to learn rust because I don't have experience on it. Master component need to be write on rust.
  As I'm still learning, contributions are highly welcome! Don't hesitate to open an issue or PR if you spot any non-idiomatic rust code.
- A lot of S3 API need to be implemented.

## License
Copyright 2026 Adhidarma Hadiwinoto <adhisimon@tektrans.id>.

This project is licensed under the Apache License, Version 2.0 - see the [LICENSE](LICENSE) file for details.
