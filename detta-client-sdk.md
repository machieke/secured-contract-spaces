# DeTTa Client SDKs

`crates/detta-client-sdk` is a Rust client SDK for application DA workflows that
store large bytes in external blob networks and commit only hash-bound
references in DeTTa DA.

`sdk/javascript` is the browser JavaScript SDK for the same workflow. It uses
ES modules, browser `fetch`, `crypto.subtle`, and a pluggable blob-client
interface so web applications can publish and retrieve external blob records
without depending on the Rust crate.

The first high-level workflow is social avatar publication:

1. Upload avatar bytes through a pluggable `BlobClient`.
2. Build a canonical `DaExternalBlobReference` and `social.media.reference`
   record.
3. Build a `social.demo` application DA media-manifest payload.
4. Produce the application DA batch through RPC.
5. Record a canonical external blob lifecycle record.
6. Retrieve bytes later and verify size/hash against the committed reference
   through the node retrieval-verification RPC.

## Atomicity Model

The SDK makes the DeTTa side of the workflow a single easy client operation:

- duplicate application profile registration is treated as success;
- the application payload is submitted in one `produce_application_da_batch`
  RPC call;
- lifecycle state is recorded only after the DA batch succeeds;
- retrieval verifies bytes locally and through
  `verify_application_da_external_blob_retrieval`.

The external blob upload is not consensus-atomic with DeTTa because IPFS,
Arweave, Filecoin, gateways, and pinning providers are outside the DeTTa state
machine. Production adapters should therefore use content-addressed or
idempotent upload APIs, stable operation references, multi-backend replication,
and repair jobs.

## Publishing An Avatar From Rust

```rust
use detta_client_sdk::{
    DettaClientSdk, InMemoryBlobClient, SocialAvatarPublishRequest,
};

let blob_client = InMemoryBlobClient::ipfs("ipfs.local");
let mut sdk = DettaClientSdk::new(rpc_client, blob_client);

let mut request = SocialAvatarPublishRequest::social_demo(
    "alice",
    "avatar-1",
    1,
    avatar_png_bytes,
    vec!["validator-1".into(), "validator-2".into()],
)?;
request.content_type = "image/png".into();

let receipt = sdk.publish_social_avatar(request)?;

println!("manifest: {}", receipt.production.manifest_hash);
println!("avatar reference: {}", receipt.reference.uri);
```

The returned `SocialAvatarPublishReceipt` contains the external upload receipt,
the canonical blob reference, the DA reference record, the produced application
DA payload, the production report, and the persisted lifecycle record.

## Retrieving An Avatar From Rust

```rust
use detta_client_sdk::BlobRetrievalRequest;

let retrieval = sdk.retrieve_verified_blob(
    BlobRetrievalRequest::new(receipt.reference_record.clone(), "bob", 2)
        .with_provider("ipfs.local"),
)?;

assert_eq!(retrieval.bytes, avatar_png_bytes);
assert!(retrieval.verification.available);
```

Clients normally get `receipt.reference_record` by reading the application DA
payload or the `social.media` namespace from a DeTTa node. The SDK parses the
record, fetches the external bytes through the configured `BlobClient`, checks
the committed byte length and SHA-256 hash, and asks the node to produce
retrieval-verification evidence.

## Provider Integration

`InMemoryBlobClient` is for tests and local examples. Production integrations
implement:

```rust
pub trait BlobClient {
    fn backend(&self) -> DaExternalBlobBackend;
    fn upload_blob(&mut self, request: BlobUploadRequest<'_>)
        -> Result<BlobUploadReceipt, ClientSdkError>;
    fn fetch_blob(&mut self, reference: &DaExternalBlobReference)
        -> Result<Vec<u8>, ClientSdkError>;
}
```

An IPFS adapter would call the pinning service or node API, return the CID as
`locator`, and return a pin or provider identifier as `provider_reference`. An
Arweave adapter would return the transaction id. A Filecoin adapter would
return a deal or piece locator and include deal evidence as
`availability_proof`.

The lower-level `upload_external_blob_reference` method is available for
application profiles other than `social.demo`; it uploads the bytes and returns
a hash-bound `DaRecordEnvelope` that application code can place in its own
namespaced DA payload.

## Publishing An Avatar From A Browser

```js
import {
  DettaClientSdk,
  FetchRpcClient,
  socialAvatarPublishRequest,
} from "@detta/client-sdk";

const rpc = new FetchRpcClient("https://validator.example/rpc");
const sdk = new DettaClientSdk({ rpc, blobClient });

const file = document.querySelector("input[type=file]").files[0];
const request = await socialAvatarPublishRequest({
  author: "alice",
  avatarId: "avatar-1",
  sequence: 1,
  avatarBytes: file,
  contentType: file.type || "image/png",
  certificateSigners: ["validator-1", "validator-2"],
});

const receipt = await sdk.publishSocialAvatar(request);
```

`blobClient` is supplied by the application and implements `uploadBlob` and
`fetchBlob`, returning provider locators for IPFS, Arweave, or Filecoin. See
`sdk/javascript/README.md` for a complete browser example.
