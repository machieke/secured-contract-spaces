import assert from "node:assert/strict";
import test from "node:test";

import {
  Blob,
} from "node:buffer";

import {
  DettaClientSdk,
  DettaSdkError,
  DEFAULT_MAX_RPC_VERIFIED_BLOB_BYTES,
  InMemoryBlobClient,
  PROFILE_ALREADY_REGISTERED_CODE,
  canonicalExternalBlobUri,
  daRecordEnvelope,
  defineApplication,
  externalBlobRetrievalVerification,
  hashCanonical,
  profileFromApplicationDefinition,
  purpleFrenzChannelCoordinate,
  purpleFrenzChatProfile,
  purpleFrenzChatProfileId,
  purpleFrenzEncryptedRecord,
  socialAvatarPublishRequest,
  socialDemoProfile,
  socialDemoProfileId,
} from "../src/index.js";

test("social demo profile hash matches the Rust fixture", async () => {
  assert.equal(
    await socialDemoProfileId(),
    "2308b7978a20f557aa664fdb623ee90a968791195bd2fcdbc5743bfac0fe9420",
  );
  assert.equal(await hashCanonical(socialDemoProfile()), await socialDemoProfileId());
});

test("external blob URI canonicalization matches DA adapter rules", () => {
  assert.equal(canonicalExternalBlobUri("Ipfs", "bafyavatar"), "ipfs://bafyavatar");
  assert.equal(canonicalExternalBlobUri("Arweave", "tx-avatar"), "ar://tx-avatar");
  assert.equal(
    canonicalExternalBlobUri("Filecoin", "deal-1/piece-1"),
    "filecoin://deal-1/piece-1",
  );
  assert.throws(
    () => canonicalExternalBlobUri("Ipfs", "ar://wrong-backend"),
    DettaSdkError,
  );
});

test("generic blob reference upload creates an external content address record", async () => {
  const sdk = new DettaClientSdk({
    rpc: new MockRpcClient(),
    blobClient: InMemoryBlobClient.arweave("arweave.local"),
  });
  const receipt = await sdk.uploadExternalBlobReference({
    recordSchema: "social.media.reference",
    contentType: "image/webp",
    blobBytes: new Uint8Array([1, 2, 3]),
    suggestedName: "avatar.webp",
  });

  assert.equal(receipt.reference.backend, "Arweave");
  assert.match(receipt.reference.uri, /^ar:\/\/sdk-arweave-1-avatar-webp$/);
  assert.equal(receipt.record.encoding, "ExternalContentAddress");
});

test("data-driven application definitions build profiles records and batches", async () => {
  const definition = defineApplication({
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
        maxRecords: 1000,
        retentionClass: "Warm",
      },
      {
        namespace: "forum.media",
        requirement: "Optional",
        records: ["forum.media.reference"],
        minRecords: 0,
        maxRecords: 1000,
        retentionClass: "Cold",
      },
    ],
    records: [
      {
        schema: "forum.media.reference",
        namespaces: ["forum.media"],
        encodings: ["ExternalContentAddress"],
        maxBytes: 16 * 1024,
        requireContentHash: true,
        requireSigner: true,
      },
      {
        schema: "forum.message",
        namespaces: ["forum.feed"],
        encodings: ["EncryptedBytes"],
        maxBytes: 128 * 1024,
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
  const rpc = new MockRpcClient({ profileAlreadyRegistered: true });
  const sdk = new DettaClientSdk({
    rpc,
    blobClient: InMemoryBlobClient.ipfs("ipfs.local"),
  });
  const app = sdk.application(definition);
  const record = await app.record("forum.message", {
    bytes: new Uint8Array([4, 5, 6]),
    signer: "0x1111111111111111111111111111111111111111",
    signature: "0xsigned",
  });

  const receipt = await app.publishBatch({
    coordinate: app.coordinate({
      template: "thread",
      values: { chainId: "8453", threadId: "42" },
      sequence: 1,
      epoch: 1,
    }),
    records: [record],
    certificateSigners: ["validator-1"],
  });

  assert.deepEqual(profileFromApplicationDefinition(definition), app.profile());
  assert.equal(receipt.payload.application_id, "forum.chat");
  assert.equal(receipt.payload.coordinate.stream_id, "base:8453:thread:42");
  assert.equal(receipt.payload.application_roots[0].name, "forum.chat.event.log.root");
  assert.equal(receipt.payload.namespaces[0].namespace, "forum.feed");
  assert.equal(receipt.production.profile_id, await app.profileId());
});

test("one-call social avatar publish and verified retrieval use RPC and blob providers", async () => {
  const rpc = new MockRpcClient({ profileAlreadyRegistered: true });
  const blobClient = InMemoryBlobClient.ipfs("ipfs.local");
  const sdk = new DettaClientSdk({ rpc, blobClient });
  const request = await socialAvatarPublishRequest({
    author: "alice",
    avatarId: "avatar-1",
    sequence: 1,
    avatarBytes: new Blob(["small-avatar-icon-webp"]),
    contentType: "image/webp",
    certificateSigners: ["validator-1", "validator-2"],
  });

  const publish = await sdk.publishSocialAvatar(request);
  assert.equal(publish.reference.backend, "Ipfs");
  assert.equal(publish.reference.content_type, "image/webp");
  assert.equal(publish.production.original_share_count, 4);
  assert.equal(publish.production.encoded_share_count, 6);
  assert.equal(publish.lifecycleRecord.reference_hash, await hashCanonical(publish.reference));
  assert.equal(blobClient.containsUri(publish.reference.uri), true);

  const retrieval = await sdk.retrieveVerifiedBlob({
    record: publish.referenceRecord,
    verifier: "bob",
    provider: "ipfs.local",
    verifiedAtHeight: 2,
  });
  assert.equal(new TextDecoder().decode(retrieval.bytes), "small-avatar-icon-webp");
  assert.equal(retrieval.verification.available, true);

  assert.deepEqual(
    rpc.requests.map((request) => request.method),
    [
      "register_application_da_profile",
      "produce_application_da_batch",
      "record_application_da_external_blob_lifecycle",
      "verify_application_da_external_blob_retrieval",
    ],
  );
});

test("verified retrieval rejects tampered blob bytes before accepting RPC evidence", async () => {
  const referenceRecord = await daRecordEnvelope({
    schema: "social.media.reference",
    contentType: "application/json",
    encoding: "ExternalContentAddress",
    bytes: JSON.stringify({
      schema: "detta.external-blob-reference.v1",
      schema_version: 1,
      backend: "Ipfs",
      uri: "ipfs://avatar",
      content_hash: "00".repeat(32),
      content_type: "image/png",
      size_bytes: 4,
      provider_reference: null,
      availability_proof: null,
    }),
  });
  const blobClient = InMemoryBlobClient.ipfs("ipfs.local");
  blobClient.objects.set("ipfs://avatar", new Uint8Array([1, 2, 3, 4]));
  const sdk = new DettaClientSdk({ rpc: new MockRpcClient(), blobClient });

  await assert.rejects(
    () =>
      sdk.retrieveVerifiedBlob({
        record: referenceRecord,
        verifier: "bob",
        provider: "ipfs.local",
        verifiedAtHeight: 2,
      }),
    /external blob retrieval verification failed locally/,
  );
});

test("large verified retrieval stays local instead of exceeding the RPC byte cap", async () => {
  const rpc = new MockRpcClient();
  const blobClient = InMemoryBlobClient.ipfs("ipfs.local");
  const sdk = new DettaClientSdk({ rpc, blobClient });
  const bytes = new Uint8Array(DEFAULT_MAX_RPC_VERIFIED_BLOB_BYTES + 1).fill(7);
  const uploaded = await sdk.uploadExternalBlobReference({
    recordSchema: "purplefrenz.media.reference",
    contentType: "application/octet-stream",
    blobBytes: bytes,
    suggestedName: "encrypted-image",
    signer: "0x1111111111111111111111111111111111111111",
    signature: "0xsigned",
  });

  const retrieval = await sdk.retrieveVerifiedBlob({
    record: uploaded.record,
    verifier: "gateway",
    provider: "ipfs.local",
    verifiedAtHeight: 1,
  });

  assert.equal(retrieval.verification.available, true);
  assert.equal(retrieval.rpcVerification, null);
  assert.equal(retrieval.verificationScope, "local");
  assert.equal(rpc.requests.length, 0);
});

test("PurpleFrenZ profile publishes encrypted channel batches", async () => {
  const rpc = new MockRpcClient({ profileAlreadyRegistered: true });
  const sdk = new DettaClientSdk({
    rpc,
    blobClient: InMemoryBlobClient.ipfs("ipfs.local"),
  });
  const profile = purpleFrenzChatProfile();
  const record = await purpleFrenzEncryptedRecord({
    encryptedBytes: new Uint8Array([1, 2, 3]),
    signer: "0x1111111111111111111111111111111111111111",
    signature: "0xsigned",
  });

  const receipt = await sdk.publishApplicationBatch({
    profile,
    coordinate: purpleFrenzChannelCoordinate({ channelId: "7", sequence: 1, epoch: 1 }),
    namespaces: [{ namespace: "purplefrenz.feed", records: [record] }],
    rootName: "purplefrenz.chat.event.log.root",
  });

  assert.equal(await purpleFrenzChatProfileId(), await hashCanonical(profile));
  assert.equal(receipt.payload.application_id, "purplefrenz.chat");
  assert.equal(receipt.payload.coordinate.stream_id, "base:8453:channel:7");
  assert.equal(receipt.payload.namespaces[0].records[0].encoding, "EncryptedBytes");
  assert.equal(receipt.production.manifest_hash, "11".repeat(32));
});

test("PurpleFrenZ profile is canonicalized for Rust validator registration", () => {
  const profile = purpleFrenzChatProfile();
  const sorted = (values) => [...values].sort((left, right) => left.localeCompare(right));

  assert.deepEqual(
    profile.namespace_policies.map((policy) => policy.namespace),
    sorted(profile.namespace_policies.map((policy) => policy.namespace)),
  );
  for (const policy of profile.namespace_policies) {
    assert.deepEqual(policy.allowed_record_schemas, sorted(policy.allowed_record_schemas));
  }
  assert.deepEqual(
    profile.record_policies.map((policy) => policy.schema),
    sorted(profile.record_policies.map((policy) => policy.schema)),
  );
});

class MockRpcClient {
  constructor(options = {}) {
    this.profileAlreadyRegistered = options.profileAlreadyRegistered ?? false;
    this.requests = [];
  }

  async request(request) {
    this.requests.push(request);
    if (request.method === "register_application_da_profile") {
      if (this.profileAlreadyRegistered) {
        return {
          status: "error",
          body: {
            code: PROFILE_ALREADY_REGISTERED_CODE,
            message: "already registered",
          },
        };
      }
      return {
        status: "ok",
        body: {
          result: "application_da_profile",
          data: {
            profile_id: await hashCanonical(request.params.profile),
            profile: request.params.profile,
          },
        },
      };
    }
    if (request.method === "produce_application_da_batch") {
      const payload = request.params.payload;
      return {
        status: "ok",
        body: {
          result: "application_da_production",
          data: {
            application_id: payload.application_id,
            profile_id: payload.profile_id,
            coordinate: payload.coordinate,
            manifest_hash: "11".repeat(32),
            certificate_hash: "22".repeat(32),
            payload_hash: await hashCanonical(payload),
            namespace_root: "33".repeat(32),
            share_root: "44".repeat(32),
            payload_bytes: JSON.stringify(payload).length,
            original_share_count: request.params.data_share_count,
            encoded_share_count:
              request.params.data_share_count + request.params.parity_share_count,
            reconstruction_threshold: request.params.data_share_count,
            certificate_signers: request.params.certificate_signers,
          },
        },
      };
    }
    if (request.method === "record_application_da_external_blob_lifecycle") {
      return {
        status: "ok",
        body: {
          result: "application_da_external_blob_lifecycle_record",
          data: request.params.record,
        },
      };
    }
    if (request.method === "verify_application_da_external_blob_retrieval") {
      return {
        status: "ok",
        body: {
          result: "application_da_external_blob_retrieval_verification",
          data: await externalBlobRetrievalVerification({
            reference: JSON.parse(
              new TextDecoder().decode(Uint8Array.from(request.params.record.bytes)),
            ),
            blobBytes: request.params.blob_bytes,
            verifier: request.params.verifier,
            provider: request.params.provider,
            verifiedAtHeight: request.params.verified_at_height,
          }),
        },
      };
    }
    return {
      status: "error",
      body: { code: "mock.unsupported", message: request.method },
    };
  }
}
