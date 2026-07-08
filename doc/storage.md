# STORAGE COMPONENT

## Methods
- GET `/blobs`: List of existing blobs
- PUT `/blobs/<BLOB_ID>`: Upsert a blob
- GET `/blobs/<BLOB_ID>`: Get a blob
- DELETE `/blobs/<BLOB_ID>`: Delete a blob

## Request Headers
- Required on all methods:
  - `Authorization: ApiKey <APIKEY>`
- Optional on PUT `/blobs/<BLOB_ID>`
  - `Digest: blake3=<HASH>`

## Flow
### PUT
```
[Sender] 
       │
       ├─── (API Key validation) ───> Invalid ───> HTTP 401 Unauthorized
       │
       ├─── (Concurency check)  ───> Multiple ──> HTTP 409 Conflict
       │
       └─── (Stream process) ──────> Finished ──> BLAKE3 verification if there is Digest request header 
                                                      ├─── Mismatch ──> HTTP 422 Unprocessable Content
                                                      └─── Match   ──> Atomic Rename ──> HTTP 201 Created
```

## HTTP status
Will return http status:
- 200: Success
- 201: PUT success
- 401: Invalid apikey
- 409: Conflict (multiple write using same blob id in the same time simultaneously)
- 422: Checksum mismatch
- 400: Invalid request
