# GLOSSARY

## Logical data abstraction

Bucket => Object (path + version) => Part(s) => Segment(s) => Shards

- Bucket: will have separate distribution policy and ACL.
- Object: single object entity, has compound unique index of path (string) with version (uuidv7) id (primary key: uuidv7).
- Part: part from S3 multipart upload.
- Segment: part bigger than max size will be splitted to multiple segments before entering EC computation.
- Shard: EC result to be put on storage nodes.

## Physical infrastucture topology

Region => Zone => Machine => Instance

- Machine: one physical server
- Instance: one running binary
