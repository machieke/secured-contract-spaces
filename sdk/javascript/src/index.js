const TEXT_ENCODER = new TextEncoder();
const PROFILE_ALREADY_REGISTERED_CODE = "rpc.application_da_profile_already_registered";
const DEFAULT_MAX_EXTERNAL_BLOB_BYTES = 128 * 1024 * 1024;
const DEFAULT_MAX_RPC_VERIFIED_BLOB_BYTES = 512 * 1024;

export {
  PROFILE_ALREADY_REGISTERED_CODE,
  DEFAULT_MAX_EXTERNAL_BLOB_BYTES,
  DEFAULT_MAX_RPC_VERIFIED_BLOB_BYTES,
};

export class DettaSdkError extends Error {
  constructor(message, options = {}) {
    super(message);
    this.name = "DettaSdkError";
    this.code = options.code ?? null;
    this.phase = options.phase ?? null;
    this.cause = options.cause;
    this.details = options.details ?? null;
  }
}

export class RpcError extends DettaSdkError {
  constructor(body) {
    super(`RPC error ${body.code}: ${body.message}`, {
      code: body.code,
      phase: "rpc",
      details: body,
    });
    this.body = body;
  }
}

export class FetchRpcClient {
  constructor(endpoint, options = {}) {
    this.endpoint = endpoint;
    this.fetch = options.fetch ?? globalThis.fetch;
    this.headers = options.headers ?? {};
    this.bearerToken = options.bearerToken ?? null;
    if (typeof this.fetch !== "function") {
      throw new DettaSdkError("FetchRpcClient requires a fetch implementation", {
        phase: "rpc",
      });
    }
  }

  async request(request) {
    const headers = {
      "content-type": "application/json",
      ...this.headers,
    };
    if (this.bearerToken !== null) {
      headers.authorization = `Bearer ${this.bearerToken}`;
    }

    const response = await this.fetch(this.endpoint, {
      method: "POST",
      headers,
      body: JSON.stringify(request),
    });
    const text = await response.text();
    let body;
    try {
      body = text.length === 0 ? null : JSON.parse(text);
    } catch (error) {
      throw new DettaSdkError("DeTTa RPC returned non-JSON response", {
        phase: "rpc",
        cause: error,
        details: { status: response.status, text },
      });
    }
    if (!body || (body.status !== "ok" && body.status !== "error")) {
      throw new DettaSdkError("DeTTa RPC returned malformed response", {
        phase: "rpc",
        details: { status: response.status, body },
      });
    }
    return body;
  }
}

export class InMemoryBlobClient {
  constructor(backend = "Ipfs", options = {}) {
    this.backend = backend;
    this.provider = options.provider ?? "in-memory";
    this.nextId = 0;
    this.objects = new Map();
  }

  static ipfs(provider = "in-memory-ipfs") {
    return new InMemoryBlobClient("Ipfs", { provider });
  }

  static arweave(provider = "in-memory-arweave") {
    return new InMemoryBlobClient("Arweave", { provider });
  }

  static filecoin(provider = "in-memory-filecoin") {
    return new InMemoryBlobClient("Filecoin", { provider });
  }

  async uploadBlob(request) {
    const bytes = await toBytes(request.bytes);
    if (bytes.length === 0) {
      throw new DettaSdkError("blob upload requires nonempty bytes", {
        phase: "blob_upload",
      });
    }
    this.nextId += 1;
    const fragment = sanitizedLocatorFragment(request.suggestedName);
    const locator =
      this.backend === "Filecoin"
        ? `sdk-filecoin-deal-${this.nextId}/piece-${fragment}`
        : `sdk-${this.backend.toLowerCase()}-${this.nextId}-${fragment}`;
    const uri = canonicalExternalBlobUri(this.backend, locator);
    this.objects.set(uri, new Uint8Array(bytes));
    const providerReference = `${this.provider}:${locator}`;
    return {
      backend: this.backend,
      locator,
      provider: this.provider,
      provider_reference: providerReference,
      operation_reference: providerReference,
      availability_proof: `in-memory-proof-${this.nextId}`,
    };
  }

  async fetchBlob(reference) {
    if (reference.backend !== this.backend) {
      throw new DettaSdkError(
        `reference backend ${reference.backend} does not match blob client backend ${this.backend}`,
        { phase: "blob_fetch", details: { reference } },
      );
    }
    const bytes = this.objects.get(reference.uri);
    if (!bytes) {
      throw new DettaSdkError(`missing blob ${reference.uri}`, {
        phase: "blob_fetch",
        details: { reference },
      });
    }
    return new Uint8Array(bytes);
  }

  containsUri(uri) {
    return this.objects.has(uri);
  }
}

export class DettaClientSdk {
  constructor({
    rpc,
    blobClient,
    maxRpcVerifiedBlobBytes = DEFAULT_MAX_RPC_VERIFIED_BLOB_BYTES,
  }) {
    if (!rpc || typeof rpc.request !== "function") {
      throw new DettaSdkError("DettaClientSdk requires an rpc client with request()", {
        phase: "init",
      });
    }
    if (
      !blobClient ||
      typeof blobClient.uploadBlob !== "function" ||
      typeof blobClient.fetchBlob !== "function"
    ) {
      throw new DettaSdkError(
        "DettaClientSdk requires a blob client with uploadBlob() and fetchBlob()",
        { phase: "init" },
      );
    }
    this.rpc = rpc;
    this.blobClient = blobClient;
    this.maxRpcVerifiedBlobBytes = maxRpcVerifiedBlobBytes;
  }

  async uploadExternalBlobReference(request) {
    const bytes = await toBytes(request.blobBytes ?? request.bytes);
    validateContentType(request.contentType);
    const upload = await this.blobClient.uploadBlob({
      contentType: request.contentType,
      bytes,
      suggestedName: request.suggestedName ?? null,
    });
    const expectedBackend = resolveBlobClientBackend(this.blobClient);
    if (upload.backend !== expectedBackend) {
      throw new DettaSdkError(
        `upload backend ${upload.backend} did not match blob client backend ${expectedBackend}`,
        { phase: "blob_upload", details: { upload } },
      );
    }
    const reference = await externalBlobReferenceFromUpload({
      upload,
      contentType: request.contentType,
      blobBytes: bytes,
    });
    const record = await externalBlobReferenceRecord({
      recordSchema: request.recordSchema,
      reference,
      signer: request.signer ?? null,
      signature: request.signature ?? null,
    });
    return { upload, reference, record };
  }

  async publishSocialAvatar(request) {
    const normalized = await normalizeSocialAvatarPublishRequest(request);
    if (normalized.ensureProfileRegistered) {
      await this.ensureApplicationDaProfileRegistered(normalized.profile);
    }

    const external = await this.uploadExternalBlobReference({
      recordSchema: "social.media.reference",
      contentType: normalized.contentType,
      blobBytes: normalized.avatarBytes,
      suggestedName: normalized.avatarId,
    });
    const mediaReferenceHash = await hashCanonical(external.reference);
    const postRecord = await daRecordEnvelope({
      schema: "social.post",
      contentType: "application/json",
      encoding: "CanonicalJson",
      bytes: utf8Bytes(
        JSON.stringify({
          author: normalized.author,
          post_id: normalized.avatarId,
          text: normalized.statusText,
          media_reference_hash: mediaReferenceHash,
        }),
      ),
      signer: normalized.author,
      signature: `sig-${normalized.author}`,
    });
    const sections = canonicalizeApplicationNamespaces([
      {
        namespace: "social.feed",
        records: [postRecord],
      },
      {
        namespace: "social.media",
        records: [external.record],
      },
    ]);
    const eventLogRoot = await applicationEventLogRootForSections(sections);
    const payload = await applicationDaPayload({
      profile: normalized.profile,
      coordinate: normalized.coordinate,
      payloadKind: "MediaManifest",
      previousPayloadHash: normalized.previousPayloadHash,
      applicationRoots: [
        {
          name: "social.event.log.root",
          hash: eventLogRoot,
        },
      ],
      namespaces: sections,
    });
    const production = await this.produceApplicationDaBatch({
      payload,
      dataShareCount: normalized.dataShareCount,
      parityShareCount: normalized.parityShareCount,
      certificateSigners: normalized.certificateSigners,
    });
    const lifecycleRecord = await externalBlobLifecycleRecord({
      reference: external.reference,
      lifecycleStage: normalized.lifecycleStage,
      status: normalized.lifecycleStatus,
      provider: external.upload.provider ?? null,
      operationReference: external.upload.operation_reference ?? null,
      observedAtHeight: normalized.observedAtHeight,
      observedBy: normalized.observedBy,
      note: normalized.lifecycleNote,
    });
    const persistedLifecycleRecord =
      await this.recordApplicationDaExternalBlobLifecycle(lifecycleRecord);

    return {
      upload: external.upload,
      reference: external.reference,
      referenceRecord: external.record,
      payload,
      production,
      lifecycleRecord: persistedLifecycleRecord,
    };
  }

  async retrieveVerifiedBlob(request) {
    const reference = parseExternalBlobReferenceRecord(request.record);
    const bytes = await this.blobClient.fetchBlob(reference);
    const localVerification = await externalBlobRetrievalVerification({
      reference,
      blobBytes: bytes,
      verifier: request.verifier,
      provider: request.provider ?? null,
      verifiedAtHeight: request.verifiedAtHeight,
    });
    if (!localVerification.available) {
      throw new DettaSdkError("external blob retrieval verification failed locally", {
        phase: "blob_verify",
        details: localVerification,
      });
    }
    if (bytes.length > this.maxRpcVerifiedBlobBytes) {
      return {
        reference,
        bytes,
        verification: localVerification,
        rpcVerification: null,
        verificationScope: "local",
      };
    }
    const verification = await this.verifyApplicationDaExternalBlobRetrieval({
      record: request.record,
      blobBytes: bytes,
      verifier: request.verifier,
      provider: request.provider ?? null,
      verifiedAtHeight: request.verifiedAtHeight,
    });
    if (!verification.available) {
      throw new DettaSdkError("external blob retrieval verification failed on DeTTa RPC", {
        phase: "blob_verify",
        details: verification,
      });
    }
    return {
      reference,
      bytes,
      verification,
      rpcVerification: verification,
      verificationScope: "local-and-rpc",
    };
  }

  async publishApplicationBatch({
    profile,
    coordinate,
    namespaces,
    payloadKind = "Batch",
    previousPayloadHash = null,
    rootName,
    dataShareCount = 4,
    parityShareCount = 2,
    certificateSigners = ["validator-1"],
    ensureProfileRegistered = true,
  }) {
    if (ensureProfileRegistered) {
      await this.ensureApplicationDaProfileRegistered(profile);
    }
    const sections = canonicalizeApplicationNamespaces(namespaces);
    const eventLogRoot = await applicationEventLogRootForSections(sections);
    const payload = await applicationDaPayload({
      profile,
      coordinate,
      payloadKind,
      previousPayloadHash,
      applicationRoots: [
        {
          name: rootName ?? `${profile.application_id}.event.log.root`,
          hash: eventLogRoot,
        },
      ],
      namespaces: sections,
    });
    const production = await this.produceApplicationDaBatch({
      payload,
      dataShareCount,
      parityShareCount,
      certificateSigners,
    });
    return { payload, production };
  }

  async ensureApplicationDaProfileRegistered(profile) {
    const response = await this.rpc.request({
      method: "register_application_da_profile",
      params: { profile },
    });
    if (response.status === "ok" && response.body.result === "application_da_profile") {
      return response.body.data;
    }
    if (
      response.status === "error" &&
      response.body.code === PROFILE_ALREADY_REGISTERED_CODE
    ) {
      return null;
    }
    return expectRpcResult(response, "application_da_profile");
  }

  async produceApplicationDaBatch({
    payload,
    dataShareCount,
    parityShareCount,
    certificateSigners,
  }) {
    const response = await this.rpc.request({
      method: "produce_application_da_batch",
      params: {
        payload,
        data_share_count: dataShareCount,
        parity_share_count: parityShareCount,
        certificate_signers: certificateSigners,
      },
    });
    return expectRpcResult(response, "application_da_production");
  }

  async recordApplicationDaExternalBlobLifecycle(record) {
    const response = await this.rpc.request({
      method: "record_application_da_external_blob_lifecycle",
      params: { record },
    });
    return expectRpcResult(response, "application_da_external_blob_lifecycle_record");
  }

  async verifyApplicationDaExternalBlobRetrieval({
    record,
    blobBytes,
    verifier,
    provider,
    verifiedAtHeight,
  }) {
    const response = await this.rpc.request({
      method: "verify_application_da_external_blob_retrieval",
      params: {
        record,
        blob_bytes: Array.from(await toBytes(blobBytes)),
        verifier,
        provider,
        verified_at_height: verifiedAtHeight,
      },
    });
    return expectRpcResult(
      response,
      "application_da_external_blob_retrieval_verification",
    );
  }

  async getApplicationDaPayload(manifestHash) {
    const response = await this.rpc.request({
      method: "get_application_da_payload",
      params: { manifest_hash: requiredString(manifestHash, "manifestHash") },
    });
    return expectRpcResult(response, "application_da_payload");
  }

  async getApplicationDaNamespace(manifestHash, namespace) {
    const response = await this.rpc.request({
      method: "get_application_da_namespace",
      params: {
        manifest_hash: requiredString(manifestHash, "manifestHash"),
        namespace: requiredString(namespace, "namespace"),
      },
    });
    return expectRpcResult(response, "application_da_namespace");
  }

  async getApplicationDaManifestIndexByApplicationId(applicationId) {
    const response = await this.rpc.request({
      method: "get_application_da_manifest_index_by_application_id",
      params: { application_id: requiredString(applicationId, "applicationId") },
    });
    return expectRpcResult(response, "application_da_manifest_index");
  }
}

export async function purpleFrenzChatProfileId() {
  return hashCanonical(purpleFrenzChatProfile());
}

export function purpleFrenzChatProfile() {
  return {
    schema: "detta.da-application-profile.v1",
    schema_version: 1,
    application_id: "purplefrenz.chat",
    profile_version: 1,
    profile_name: "PurpleFrenZ encrypted channel chat v1",
    da_profile: daProductionProfileV1(),
    namespace_policies: [
      namespacePolicy(
        "purplefrenz.feed",
        "Required",
        [
          "purplefrenz.edit",
          "purplefrenz.message",
          "purplefrenz.reaction",
          "purplefrenz.tombstone",
        ],
        1,
        10000,
        "Warm",
      ),
      namespacePolicy(
        "purplefrenz.media",
        "Optional",
        ["purplefrenz.media.reference"],
        0,
        10000,
        "Cold",
      ),
    ],
    record_policies: [
      recordPolicy(
        "purplefrenz.edit",
        ["purplefrenz.feed"],
        ["EncryptedBytes"],
        256 * 1024,
        true,
        true,
      ),
      recordPolicy(
        "purplefrenz.media.reference",
        ["purplefrenz.media"],
        ["ExternalContentAddress"],
        16 * 1024,
        true,
        true,
      ),
      recordPolicy(
        "purplefrenz.message",
        ["purplefrenz.feed"],
        ["EncryptedBytes"],
        256 * 1024,
        true,
        true,
      ),
      recordPolicy(
        "purplefrenz.reaction",
        ["purplefrenz.feed"],
        ["EncryptedBytes"],
        64 * 1024,
        true,
        true,
      ),
      recordPolicy(
        "purplefrenz.tombstone",
        ["purplefrenz.feed"],
        ["EncryptedBytes"],
        64 * 1024,
        true,
        true,
      ),
    ],
    coordinate_policy: {
      max_stream_id_bytes: 128,
      allow_epoch: true,
      require_parent_hash: false,
      require_subject_hash: false,
    },
    root_bindings: [{ name: "purplefrenz.chat.event.log.root", required: true }],
    retention_policy: {
      default_class: "Warm",
      namespace_overrides: [
        { namespace: "purplefrenz.media", retention_class: "Cold" },
      ],
      payload_kind_overrides: [
        { payload_kind: "Batch", retention_class: "Warm" },
      ],
    },
    validation_mode: "SchemaDecodableRecords",
    privacy_mode: "Encrypted",
    max_payload_bytes: 16 * 1024 * 1024,
    max_records_per_payload: 100000,
  };
}

export function purpleFrenzChannelCoordinate({
  channelId,
  sequence,
  epoch,
  parentHash = null,
  subjectHash = null,
}) {
  const normalizedChannelId = requiredString(String(channelId), "channelId");
  if (!Number.isSafeInteger(sequence) || sequence <= 0) {
    throw new DettaSdkError("sequence must be a positive integer", { phase: "input" });
  }
  if (!Number.isSafeInteger(epoch) || epoch <= 0) {
    throw new DettaSdkError("epoch must be a positive integer", { phase: "input" });
  }
  return {
    application_id: "purplefrenz.chat",
    stream_id: `base:8453:channel:${normalizedChannelId}`,
    sequence,
    epoch,
    parent_hash: parentHash,
    subject_hash: subjectHash,
  };
}

export async function purpleFrenzEncryptedRecord({
  schema = "purplefrenz.message",
  encryptedBytes,
  signer,
  signature,
}) {
  if (
    ![
      "purplefrenz.message",
      "purplefrenz.reaction",
      "purplefrenz.edit",
      "purplefrenz.tombstone",
    ].includes(schema)
  ) {
    throw new DettaSdkError(`unsupported PurpleFrenZ record schema ${schema}`, {
      phase: "input",
    });
  }
  return daRecordEnvelope({
    schema,
    contentType: "application/octet-stream",
    encoding: "EncryptedBytes",
    bytes: encryptedBytes,
    signer: requiredString(signer, "signer"),
    signature: requiredString(signature, "signature"),
  });
}

export async function socialDemoProfileId() {
  return hashCanonical(socialDemoProfile());
}

export function socialDemoProfile() {
  return {
    schema: "detta.da-application-profile.v1",
    schema_version: 1,
    application_id: "social.demo",
    profile_version: 1,
    profile_name: "Social Demo DA v1",
    da_profile: daProductionProfileV1(),
    namespace_policies: [
      namespacePolicy("social.feed", "Required", ["social.post"], 1, 10000, "Warm"),
      namespacePolicy(
        "social.media",
        "Optional",
        ["social.media.reference"],
        0,
        10000,
        "Cold",
      ),
      namespacePolicy(
        "social.moderation",
        "Optional",
        ["social.moderation.action"],
        0,
        10000,
        "Archive",
      ),
      namespacePolicy(
        "social.private",
        "Optional",
        ["social.private.message"],
        0,
        10000,
        "Cold",
      ),
    ],
    record_policies: [
      recordPolicy(
        "social.media.reference",
        ["social.media"],
        ["ExternalContentAddress"],
        16 * 1024,
        true,
        false,
      ),
      recordPolicy(
        "social.moderation.action",
        ["social.moderation"],
        ["CanonicalJson"],
        64 * 1024,
        true,
        true,
      ),
      recordPolicy(
        "social.post",
        ["social.feed"],
        ["CanonicalJson"],
        256 * 1024,
        true,
        true,
      ),
      recordPolicy(
        "social.private.message",
        ["social.private"],
        ["EncryptedBytes"],
        256 * 1024,
        true,
        true,
      ),
    ],
    coordinate_policy: {
      max_stream_id_bytes: 128,
      allow_epoch: true,
      require_parent_hash: false,
      require_subject_hash: false,
    },
    root_bindings: [{ name: "social.event.log.root", required: true }],
    retention_policy: {
      default_class: "Warm",
      namespace_overrides: [
        { namespace: "social.media", retention_class: "Cold" },
        { namespace: "social.moderation", retention_class: "Archive" },
        { namespace: "social.private", retention_class: "Cold" },
      ],
      payload_kind_overrides: [
        { payload_kind: "Batch", retention_class: "Warm" },
        { payload_kind: "MediaManifest", retention_class: "Cold" },
        { payload_kind: "ModerationLog", retention_class: "Archive" },
      ],
    },
    validation_mode: "SchemaDecodableRecords",
    privacy_mode: "MixedExplicit",
    max_payload_bytes: 16 * 1024 * 1024,
    max_records_per_payload: 100000,
  };
}

export function daProductionProfileV1() {
  return {
    schema: "detta.da-production-profile.v1",
    schema_version: 1,
    commitment_scheme: "MerkleSha256V1",
    erasure_scheme: "ReedSolomonV1",
    custody_mode: "DeterministicCustodyWithLightClientSampling",
    min_custody_share_count: 2,
    min_light_client_sample_count: 3,
    full_payload_required_for_rpc: true,
    receipts_are_payload_records: true,
    event_availability_mode: "RegeneratedFromExecutionRoots",
    validator_min_retention_blocks: 65536,
    archive_min_retention_blocks: 1048576,
    data_gas_bytes_per_unit: 1024,
    slashing_governance_mode: "TimelockedValidatorSetGovernance",
    archive_incentive_mode: "GovernanceRegisteredStorageProviders",
    mandatory_da_namespaces: [
      "detta.aspect",
      "detta.block",
      "detta.bridge",
      "detta.governance",
      "detta.oracle",
      "detta.receipt",
      "detta.tx",
    ],
  };
}

export async function socialAvatarPublishRequest(options) {
  const profile = options.profile ?? socialDemoProfile();
  const author = requiredString(options.author, "author");
  const avatarId = requiredString(options.avatarId, "avatarId");
  const sequence = options.sequence ?? 1;
  const avatarBytes = await toBytes(options.avatarBytes);
  return {
    profile,
    coordinate: options.coordinate ?? {
      application_id: profile.application_id,
      stream_id: options.streamId ?? `user:${author}.avatar`,
      sequence,
      epoch: options.epoch ?? 1,
      parent_hash: options.parentHash ?? null,
      subject_hash: options.subjectHash ?? null,
    },
    author,
    avatarId,
    avatarBytes,
    contentType: options.contentType ?? "image/png",
    statusText: options.statusText ?? "avatar updated",
    previousPayloadHash: options.previousPayloadHash ?? null,
    dataShareCount: options.dataShareCount ?? 4,
    parityShareCount: options.parityShareCount ?? 2,
    certificateSigners: options.certificateSigners ?? ["validator-1"],
    observedAtHeight: options.observedAtHeight ?? sequence,
    observedBy: options.observedBy ?? author,
    lifecycleStage: options.lifecycleStage ?? "Pinned",
    lifecycleStatus: options.lifecycleStatus ?? "Active",
    lifecycleNote: options.lifecycleNote ?? "published through @detta/client-sdk",
    ensureProfileRegistered: options.ensureProfileRegistered ?? true,
  };
}

export async function applicationDaPayload({
  profile,
  coordinate,
  payloadKind,
  previousPayloadHash = null,
  applicationRoots,
  namespaces,
}) {
  const profileId = await hashCanonical(profile);
  return {
    schema: "detta.application-da-payload.v1",
    schema_version: 1,
    application_id: profile.application_id,
    profile_id: profileId,
    coordinate,
    payload_kind: payloadKind,
    previous_payload_hash: previousPayloadHash,
    application_roots: [...applicationRoots].sort((a, b) => a.name.localeCompare(b.name)),
    namespaces: canonicalizeApplicationNamespaces(namespaces),
  };
}

export async function daRecordEnvelope({
  schema,
  schemaVersion = 1,
  contentType,
  encoding,
  bytes,
  signer = null,
  signature = null,
}) {
  const payloadBytes = await toBytes(bytes);
  return {
    schema,
    schema_version: schemaVersion,
    content_type: contentType,
    encoding,
    bytes: Array.from(payloadBytes),
    content_hash: await sha256Hex(payloadBytes),
    signer,
    signature,
  };
}

export async function externalBlobReferenceFromUpload({
  upload,
  contentType,
  blobBytes,
}) {
  validateContentType(contentType);
  const bytes = await toBytes(blobBytes);
  if (bytes.length === 0) {
    throw new DettaSdkError("external blob reference requires nonempty bytes", {
      phase: "blob_reference",
    });
  }
  if (bytes.length > DEFAULT_MAX_EXTERNAL_BLOB_BYTES) {
    throw new DettaSdkError("external blob exceeds the default adapter size limit", {
      phase: "blob_reference",
      details: { size: bytes.length, max: DEFAULT_MAX_EXTERNAL_BLOB_BYTES },
    });
  }
  return {
    schema: "detta.external-blob-reference.v1",
    schema_version: 1,
    backend: upload.backend,
    uri: canonicalExternalBlobUri(upload.backend, upload.locator),
    content_hash: await sha256Hex(bytes),
    content_type: contentType,
    size_bytes: bytes.length,
    provider_reference: upload.provider_reference ?? null,
    availability_proof: upload.availability_proof ?? null,
  };
}

export async function externalBlobReferenceRecord({
  recordSchema,
  reference,
  signer = null,
  signature = null,
}) {
  return daRecordEnvelope({
    schema: recordSchema,
    contentType: "application/json",
    encoding: "ExternalContentAddress",
    bytes: utf8Bytes(JSON.stringify(reference)),
    signer,
    signature,
  });
}

export function parseExternalBlobReferenceRecord(record) {
  if (record.encoding !== "ExternalContentAddress") {
    throw new DettaSdkError("record is not an external content address", {
      phase: "blob_reference",
      details: { record },
    });
  }
  if (record.content_type !== "application/json") {
    throw new DettaSdkError("external blob reference record must use application/json", {
      phase: "blob_reference",
      details: { record },
    });
  }
  const reference = JSON.parse(textFromByteArray(record.bytes));
  validateExternalBlobReference(reference);
  return reference;
}

export async function externalBlobLifecycleRecord({
  reference,
  lifecycleStage,
  status,
  provider,
  operationReference,
  observedAtHeight,
  observedBy,
  note,
}) {
  return {
    schema: "detta.external-blob-lifecycle.v1",
    schema_version: 1,
    reference,
    reference_hash: await hashCanonical(reference),
    lifecycle_stage: lifecycleStage,
    status,
    provider,
    operation_reference: operationReference,
    observed_at_height: observedAtHeight,
    observed_by: observedBy,
    note,
  };
}

export async function externalBlobRetrievalVerification({
  reference,
  blobBytes,
  verifier,
  provider,
  verifiedAtHeight,
}) {
  validateExternalBlobReference(reference);
  const bytes = blobBytes == null ? null : await toBytes(blobBytes);
  const bytesReturned = bytes !== null && bytes.length > 0;
  const actualSize = bytesReturned ? bytes.length : null;
  const actualHash = bytesReturned ? await sha256Hex(bytes) : null;
  const sizeVerified = actualSize === reference.size_bytes;
  const hashVerified = actualHash === reference.content_hash;
  const available = bytesReturned && sizeVerified && hashVerified;
  let failureReason = null;
  if (!available && !bytesReturned) {
    failureReason = "external blob retrieval returned no bytes";
  } else if (!available && !sizeVerified) {
    failureReason = "external blob retrieval returned the wrong byte length";
  } else if (!available) {
    failureReason = "external blob retrieval returned the wrong content hash";
  }
  return {
    schema: "detta.external-blob-retrieval-verification.v1",
    schema_version: 1,
    reference,
    reference_hash: await hashCanonical(reference),
    provider,
    verifier,
    verified_at_height: verifiedAtHeight,
    bytes_returned: bytesReturned,
    size_verified: sizeVerified,
    hash_verified: hashVerified,
    available,
    actual_size_bytes: actualSize,
    actual_content_hash: actualHash,
    failure_reason: failureReason,
  };
}

export async function applicationEventLogRootForSections(sections) {
  const contentHashes = [];
  for (const section of sections) {
    for (const record of section.records) {
      contentHashes.push(record.content_hash);
    }
  }
  if (contentHashes.length === 0) {
    throw new DettaSdkError("application event log root requires at least one record", {
      phase: "payload",
    });
  }
  return merkleRoot(contentHashes);
}

export async function merkleRoot(values) {
  let level = await Promise.all(values.map((value) => merkleLeaf(value)));
  if (level.length === 0) {
    return sha256Hex(utf8Bytes("detta.da.empty.v1"));
  }
  while (level.length > 1) {
    const next = [];
    for (let index = 0; index < level.length; index += 2) {
      const left = level[index];
      const right = level[index + 1] ?? left;
      next.push(await merkleParent(left, right));
    }
    level = next;
  }
  return hexFromBytes(level[0]);
}

export async function hashCanonical(value) {
  return sha256Hex(utf8Bytes(JSON.stringify(value)));
}

export async function sha256Hex(bytes) {
  const input = await toBytes(bytes);
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) {
    throw new DettaSdkError("SHA-256 requires globalThis.crypto.subtle", {
      phase: "hash",
    });
  }
  const digest = await subtle.digest("SHA-256", input);
  return hexFromBytes(new Uint8Array(digest));
}

export async function toBytes(input) {
  if (input instanceof Uint8Array) {
    return input;
  }
  if (ArrayBuffer.isView(input)) {
    return new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
  }
  if (input instanceof ArrayBuffer) {
    return new Uint8Array(input);
  }
  if (Array.isArray(input)) {
    return Uint8Array.from(input);
  }
  if (typeof input === "string") {
    return utf8Bytes(input);
  }
  if (typeof Blob !== "undefined" && input instanceof Blob) {
    return new Uint8Array(await input.arrayBuffer());
  }
  throw new DettaSdkError("unsupported byte input", {
    phase: "bytes",
    details: { inputType: typeof input },
  });
}

export function canonicalExternalBlobUri(backend, locator) {
  const prefix = externalBlobUriPrefix(backend);
  if (locator.startsWith(prefix)) {
    validateExternalBlobLocator(locator.slice(prefix.length), "external blob URI locator");
    return locator;
  }
  if (locator.includes("://")) {
    throw new DettaSdkError(`external blob locator must use ${prefix} URIs`, {
      phase: "blob_reference",
      details: { backend, locator },
    });
  }
  validateExternalBlobLocator(locator, "external blob locator");
  return `${prefix}${locator}`;
}

function externalBlobUriPrefix(backend) {
  switch (backend) {
    case "Ipfs":
      return "ipfs://";
    case "Arweave":
      return "ar://";
    case "Filecoin":
      return "filecoin://";
    default:
      throw new DettaSdkError(`unsupported external blob backend ${backend}`, {
        phase: "blob_reference",
      });
  }
}

function expectRpcResult(response, expected) {
  if (response.status === "error") {
    throw new RpcError(response.body);
  }
  if (response.status !== "ok" || response.body.result !== expected) {
    throw new DettaSdkError(`expected RPC result ${expected}`, {
      phase: "rpc",
      details: { response },
    });
  }
  return response.body.data;
}

async function normalizeSocialAvatarPublishRequest(request) {
  const profile = request.profile ?? socialDemoProfile();
  const author = requiredString(request.author, "author");
  const avatarId = requiredString(request.avatarId, "avatarId");
  const avatarBytes = await toBytes(request.avatarBytes);
  const sequence = request.sequence ?? request.coordinate?.sequence ?? 1;
  const normalized = {
    profile,
    coordinate: request.coordinate ?? {
      application_id: profile.application_id,
      stream_id: request.streamId ?? `user:${author}.avatar`,
      sequence,
      epoch: request.epoch ?? 1,
      parent_hash: request.parentHash ?? null,
      subject_hash: request.subjectHash ?? null,
    },
    author,
    avatarId,
    avatarBytes,
    contentType: request.contentType ?? "image/png",
    statusText: request.statusText ?? "avatar updated",
    previousPayloadHash: request.previousPayloadHash ?? null,
    dataShareCount: request.dataShareCount ?? 4,
    parityShareCount: request.parityShareCount ?? 2,
    certificateSigners: request.certificateSigners ?? ["validator-1"],
    observedAtHeight: request.observedAtHeight ?? sequence,
    observedBy: request.observedBy ?? author,
    lifecycleStage: request.lifecycleStage ?? "Pinned",
    lifecycleStatus: request.lifecycleStatus ?? "Active",
    lifecycleNote: request.lifecycleNote ?? "published through @detta/client-sdk",
    ensureProfileRegistered: request.ensureProfileRegistered ?? true,
  };
  validateSocialAvatarPublishRequest(normalized);
  return normalized;
}

function validateSocialAvatarPublishRequest(request) {
  validateContentType(request.contentType);
  if (!request.contentType.startsWith("image/")) {
    throw new DettaSdkError("avatar contentType must be an image media type", {
      phase: "input",
    });
  }
  if (request.avatarBytes.length === 0) {
    throw new DettaSdkError("avatarBytes must be nonempty", { phase: "input" });
  }
  if (request.coordinate.application_id !== request.profile.application_id) {
    throw new DettaSdkError("avatar coordinate application_id must match profile", {
      phase: "input",
    });
  }
  if (request.dataShareCount <= 0) {
    throw new DettaSdkError("dataShareCount must be positive", { phase: "input" });
  }
  if (!Array.isArray(request.certificateSigners) || request.certificateSigners.length === 0) {
    throw new DettaSdkError("certificateSigners must be nonempty", { phase: "input" });
  }
  requiredString(request.observedBy, "observedBy");
}

function validateExternalBlobReference(reference) {
  if (reference.schema !== "detta.external-blob-reference.v1") {
    throw new DettaSdkError("unexpected external blob reference schema", {
      phase: "blob_reference",
      details: { reference },
    });
  }
  if (reference.schema_version !== 1) {
    throw new DettaSdkError("unexpected external blob reference version", {
      phase: "blob_reference",
      details: { reference },
    });
  }
  if (reference.uri !== canonicalExternalBlobUri(reference.backend, reference.uri)) {
    throw new DettaSdkError("external blob reference URI is not canonical", {
      phase: "blob_reference",
      details: { reference },
    });
  }
  validateContentType(reference.content_type);
  if (!isSha256Hex(reference.content_hash)) {
    throw new DettaSdkError("external blob content_hash must be SHA-256 hex", {
      phase: "blob_reference",
      details: { reference },
    });
  }
  if (!Number.isSafeInteger(reference.size_bytes) || reference.size_bytes <= 0) {
    throw new DettaSdkError("external blob size_bytes must be positive", {
      phase: "blob_reference",
      details: { reference },
    });
  }
}

function validateContentType(value) {
  requiredString(value, "contentType");
  if ([...value].some((char) => char < " ")) {
    throw new DettaSdkError("contentType must be printable", { phase: "input" });
  }
}

function validateExternalBlobLocator(value, field) {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.trim() !== value ||
    value.startsWith("/") ||
    value.endsWith("/") ||
    value.includes("//") ||
    value.includes("..") ||
    value.includes("?") ||
    value.includes("#") ||
    [...value].some((char) => char <= " ")
  ) {
    throw new DettaSdkError(
      `${field} must be a nonempty canonical URI/locator without whitespace, traversal, query, or fragment`,
      { phase: "blob_reference", details: { value } },
    );
  }
}

function canonicalizeApplicationNamespaces(namespaces) {
  const merged = new Map();
  for (const section of namespaces) {
    const existing = merged.get(section.namespace) ?? [];
    existing.push(...section.records);
    merged.set(section.namespace, existing);
  }
  return [...merged.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([namespace, records]) => ({ namespace, records }));
}

function namespacePolicy(namespace, requirement, allowedRecordSchemas, minRecords, maxRecords, retentionClass) {
  return {
    namespace,
    requirement,
    allowed_record_schemas: allowedRecordSchemas,
    min_records: minRecords,
    max_records: maxRecords,
    retention_class: retentionClass,
  };
}

function recordPolicy(schema, allowedNamespaces, allowedEncodings, maxRecordBytes, requireContentHash, requireSigner) {
  return {
    schema,
    schema_version: 1,
    allowed_namespaces: allowedNamespaces,
    allowed_encodings: allowedEncodings,
    max_record_bytes: maxRecordBytes,
    require_content_hash: requireContentHash,
    require_signer: requireSigner,
  };
}

function resolveBlobClientBackend(blobClient) {
  if (typeof blobClient.backend === "function") {
    return blobClient.backend();
  }
  if (typeof blobClient.backend === "string") {
    return blobClient.backend;
  }
  throw new DettaSdkError("blob client must expose a backend string or backend() method", {
    phase: "blob_upload",
  });
}

async function merkleLeaf(value) {
  const bytes = utf8Bytes(JSON.stringify(value));
  return sha256Concat([
    utf8Bytes("detta.da.leaf.v1"),
    uint64BigEndian(bytes.length),
    bytes,
  ]);
}

async function merkleParent(left, right) {
  return sha256Concat([utf8Bytes("detta.da.node.v1"), left, right]);
}

async function sha256Concat(chunks) {
  const size = chunks.reduce((total, chunk) => total + chunk.length, 0);
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  const digest = await globalThis.crypto.subtle.digest("SHA-256", bytes);
  return new Uint8Array(digest);
}

function uint64BigEndian(value) {
  let number = BigInt(value);
  const bytes = new Uint8Array(8);
  for (let index = 7; index >= 0; index -= 1) {
    bytes[index] = Number(number & 0xffn);
    number >>= 8n;
  }
  return bytes;
}

function utf8Bytes(value) {
  return TEXT_ENCODER.encode(value);
}

function textFromByteArray(bytes) {
  return new TextDecoder().decode(Uint8Array.from(bytes));
}

function hexFromBytes(bytes) {
  return [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

function isSha256Hex(value) {
  return typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
}

function requiredString(value, field) {
  if (typeof value !== "string" || value.length === 0 || value.trim() !== value) {
    throw new DettaSdkError(`${field} must be a nonempty trimmed string`, {
      phase: "input",
    });
  }
  return value;
}

function sanitizedLocatorFragment(value) {
  const source = typeof value === "string" && value.length > 0 ? value : "blob";
  let output = "";
  for (const char of source) {
    if (/[a-z0-9]/.test(char)) {
      output += char;
    } else if (/[A-Z]/.test(char)) {
      output += char.toLowerCase();
    } else if (char === "-" || char === "_" || char === ".") {
      output += "-";
    }
    if (output.length >= 64) {
      break;
    }
  }
  return output.length === 0 ? "blob" : output;
}
