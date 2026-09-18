# DeTTa Browser JavaScript SDK

This package provides a browser-oriented JavaScript SDK for DeTTa application
DA workflows that store large bytes in external blob networks and commit
hash-bound references in DeTTa.

It is dependency-free ESM and uses browser `fetch` plus `crypto.subtle`.
The core API is application-neutral: application support is added by passing a
declarative `ApplicationDefinition`, not by changing SDK code.

## Install From This Repository

```json
{
  "dependencies": {
    "@detta/client-sdk": "file:./sdk/javascript"
  }
}
```

## Define An Application

```js
import { defineApplication, DettaClientSdk, FetchRpcClient } from "@detta/client-sdk";

const forumDefinition = defineApplication({
  applicationId: "forum.chat",
  profileName: "Forum chat DA v1",
  privacyMode: "Encrypted",
  rootBindings: [{ name: "forum.chat.event.log.root", required: true }],
  coordinateTemplates: {
    thread: "base:{chainId}:thread:{threadId}",
  },
  namespaces: [
    {
      namespace: "forum.feed",
      requirement: "Required",
      records: ["forum.message"],
      minRecords: 1,
      maxRecords: 10000,
      retentionClass: "Warm",
    },
    {
      namespace: "forum.media",
      requirement: "Optional",
      records: ["forum.media.reference"],
      minRecords: 0,
      maxRecords: 10000,
      retentionClass: "Cold",
    },
  ],
  records: [
    {
      schema: "forum.message",
      namespaces: ["forum.feed"],
      encodings: ["EncryptedBytes"],
      maxBytes: 262144,
      requireContentHash: true,
      requireSigner: true,
    },
    {
      schema: "forum.media.reference",
      namespaces: ["forum.media"],
      encodings: ["ExternalContentAddress"],
      maxBytes: 16384,
      requireContentHash: true,
      requireSigner: true,
    },
  ],
  retention: {
    defaultClass: "Warm",
    namespaceOverrides: { "forum.media": "Cold" },
    payloadKindOverrides: { Batch: "Warm" },
  },
});
```

## Publish A Generic Batch

```js
const sdk = new DettaClientSdk({ rpc, blobClient });
const app = sdk.application(forumDefinition);

const record = await app.record("forum.message", {
  bytes: encryptedMessageBytes,
  signer: smartWalletAddress,
  signature: walletSignature,
});

const receipt = await app.publishBatch({
  coordinate: app.coordinate({
    template: "thread",
    values: { chainId: "8453", threadId: "42" },
    sequence: 1,
    epoch: 1,
  }),
  records: [record],
  certificateSigners: ["validator-1", "validator-2"],
});
```

`defineApplication` builds the canonical DeTTa application profile,
`app.record` builds hash-bound record envelopes from the definition's record
policies, and `app.publishBatch` registers the profile when needed and submits
one canonical application DA batch.

## Publish An Avatar Recipe

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

## Retrieve A Blob

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

## Data-Driven Recipes

The SDK still ships convenience recipes for tested examples:

- `socialDemoDefinition()` and `socialAvatarPublishRequest()` for the checked
  `social.demo` fixture;
- `purpleFrenzChatDefinition()` for encrypted message, reaction, edit, and
  tombstone records plus external encrypted-media references;
- `purpleFrenzChannelCoordinate()` and `purpleFrenzEncryptedRecord()` as thin
  wrappers over the generic definition methods.

New applications should usually export only a definition object plus optional
small convenience wrappers. They should not need SDK changes.

DeTTa validates signer and signature presence for generic application
profiles. An application gateway must cryptographically verify wallet
signatures before publishing records.

`retrieveVerifiedBlob()` always verifies fetched bytes locally. By default it
also requests DeTTa RPC verification for blobs up to 512 KiB. Larger blobs are
returned with `verificationScope: "local"` because the current RPC request
bound rejects larger verification bodies. Set `maxRpcVerifiedBlobBytes` in the
SDK constructor to make this behavior stricter.
