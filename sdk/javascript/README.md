# DeTTa Browser JavaScript SDK

This package provides a browser-oriented JavaScript SDK for DeTTa application
DA workflows that store large bytes in external blob networks and commit
hash-bound references in DeTTa.

It is dependency-free ESM and uses browser `fetch` plus `crypto.subtle`.

## Install From This Repository

```json
{
  "dependencies": {
    "@detta/client-sdk": "file:./sdk/javascript"
  }
}
```

## Publish An Avatar

```js
import {
  DettaClientSdk,
  FetchRpcClient,
  socialAvatarPublishRequest,
} from "@detta/client-sdk";

const rpc = new FetchRpcClient("https://validator.example/rpc");
const blobClient = {
  backend: "Ipfs",
  async uploadBlob({ bytes, suggestedName }) {
    const cid = await uploadToYourPinningProvider(bytes, suggestedName);
    return {
      backend: "Ipfs",
      locator: cid,
      provider: "pinning.example",
      provider_reference: cid,
      operation_reference: cid,
      availability_proof: null,
    };
  },
  async fetchBlob(reference) {
    const response = await fetch(`https://ipfs.io/ipfs/${reference.uri.slice("ipfs://".length)}`);
    return new Uint8Array(await response.arrayBuffer());
  },
};

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
console.log(receipt.production.manifest_hash);
console.log(receipt.reference.uri);
```

## Retrieve An Avatar

```js
const retrieval = await sdk.retrieveVerifiedBlob({
  record: receipt.referenceRecord,
  verifier: "bob",
  provider: "pinning.example",
  verifiedAtHeight: 2,
});

const url = URL.createObjectURL(new Blob([retrieval.bytes], {
  type: retrieval.reference.content_type,
}));
document.querySelector("img").src = url;
```

In a real app, `record` normally comes from
`get_application_da_payload` or `get_application_da_namespace` rather than from
the original publish receipt.

## Atomicity Model

`publishSocialAvatar` makes the client-side sequence easy and retry-friendly:

- it treats duplicate `social.demo` profile registration as success;
- it submits the application DA payload in one RPC call;
- it records lifecycle state only after DA production succeeds;
- `retrieveVerifiedBlob` checks the fetched bytes locally and through DeTTa RPC.

The external blob upload is outside DeTTa consensus. Production blob clients
should use content-addressed or idempotent uploads, stable operation
references, replication policies, health checks, and repair jobs.

## PurpleFrenZ Encrypted Chat

Version 0.2 adds the generic application helpers used by PurpleFrenZ:

- `purpleFrenzChatProfile()` defines encrypted message, reaction, edit, and
  tombstone records plus external encrypted-media references;
- `purpleFrenzChannelCoordinate()` creates Base channel stream coordinates;
- `purpleFrenzEncryptedRecord()` creates signed encrypted record envelopes;
- `publishApplicationBatch()` commits a canonical multi-namespace batch;
- `getApplicationDaPayload()`, `getApplicationDaNamespace()`, and
  `getApplicationDaManifestIndexByApplicationId()` expose the read path.

DeTTa validates signer and signature presence for generic application
profiles. An application gateway must cryptographically verify wallet
signatures before publishing records. PurpleFrenZ signs a SHA-256 commitment
to each encrypted event with its Privy smart wallet.

`retrieveVerifiedBlob()` always verifies fetched bytes locally. By default it
also requests DeTTa RPC verification for blobs up to 512 KiB. Larger blobs are
returned with `verificationScope: "local"` because the current RPC request
bound rejects larger verification bodies. Set `maxRpcVerifiedBlobBytes` in the
SDK constructor to make this behavior stricter.
