use std::collections::BTreeMap;
use std::fmt;

use detta_da::{
    application_event_log_root_for_sections, verify_external_blob_retrieval,
    ApplicationDaNamespaceSection, ApplicationDaPayload, ArweaveAdapter, DaApplicationCoordinate,
    DaApplicationProfile, DaApplicationRoot, DaError, DaExternalBlobAdapter, DaExternalBlobBackend,
    DaExternalBlobLifecycleRecord, DaExternalBlobLifecycleStage, DaExternalBlobLifecycleStatus,
    DaExternalBlobReference, DaExternalBlobRetrievalVerification, DaNamespace, DaPayloadKind,
    DaRecordEncoding, DaRecordEnvelope, FilecoinAdapter, IpfsAdapter,
};
use detta_rpc::{ApplicationDaProductionReport, RpcErrorBody, RpcRequest, RpcResponse, RpcResult};
use serde::Serialize;

pub const PROFILE_ALREADY_REGISTERED_CODE: &str = "rpc.application_da_profile_already_registered";

pub trait DettaRpcClient {
    fn request(&mut self, request: RpcRequest) -> Result<RpcResponse, ClientSdkError>;
}

impl<F> DettaRpcClient for F
where
    F: FnMut(RpcRequest) -> Result<RpcResponse, ClientSdkError>,
{
    fn request(&mut self, request: RpcRequest) -> Result<RpcResponse, ClientSdkError> {
        self(request)
    }
}

pub trait BlobClient {
    fn backend(&self) -> DaExternalBlobBackend;
    fn upload_blob(
        &mut self,
        request: BlobUploadRequest<'_>,
    ) -> Result<BlobUploadReceipt, ClientSdkError>;
    fn fetch_blob(
        &mut self,
        reference: &DaExternalBlobReference,
    ) -> Result<Vec<u8>, ClientSdkError>;
}

#[derive(Debug)]
pub enum ClientSdkError {
    Blob(String),
    BlobUnavailable(Box<DaExternalBlobRetrievalVerification>),
    Da(DaError),
    Encode(serde_json::Error),
    InvalidInput(String),
    Rpc(RpcErrorBody),
    Transport(String),
    UnexpectedResult {
        expected: &'static str,
        actual: String,
    },
}

impl fmt::Display for ClientSdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blob(error) => write!(f, "blob provider error: {error}"),
            Self::BlobUnavailable(report) => write!(
                f,
                "blob unavailable for reference {}: {}",
                report.reference_hash,
                report
                    .failure_reason
                    .as_deref()
                    .unwrap_or("retrieval verification failed")
            ),
            Self::Da(error) => write!(f, "DA error: {error:?}"),
            Self::Encode(error) => write!(f, "JSON encode error: {error}"),
            Self::InvalidInput(error) => write!(f, "invalid SDK input: {error}"),
            Self::Rpc(error) => write!(f, "RPC error {}: {}", error.code, error.message),
            Self::Transport(error) => write!(f, "transport error: {error}"),
            Self::UnexpectedResult { expected, actual } => {
                write!(f, "expected {expected}, got {actual}")
            }
        }
    }
}

impl std::error::Error for ClientSdkError {}

impl From<DaError> for ClientSdkError {
    fn from(error: DaError) -> Self {
        Self::Da(error)
    }
}

impl From<serde_json::Error> for ClientSdkError {
    fn from(error: serde_json::Error) -> Self {
        Self::Encode(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlobUploadRequest<'a> {
    pub content_type: &'a str,
    pub bytes: &'a [u8],
    pub suggested_name: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlobUploadReceipt {
    pub backend: DaExternalBlobBackend,
    pub locator: String,
    pub provider: Option<String>,
    pub provider_reference: Option<String>,
    pub operation_reference: Option<String>,
    pub availability_proof: Option<String>,
}

#[derive(Clone, Debug)]
pub struct InMemoryBlobClient {
    backend: DaExternalBlobBackend,
    provider: String,
    next_id: u64,
    objects: BTreeMap<String, Vec<u8>>,
}

impl InMemoryBlobClient {
    pub fn ipfs(provider: impl Into<String>) -> Self {
        Self::new(DaExternalBlobBackend::Ipfs, provider)
    }

    pub fn arweave(provider: impl Into<String>) -> Self {
        Self::new(DaExternalBlobBackend::Arweave, provider)
    }

    pub fn filecoin(provider: impl Into<String>) -> Self {
        Self::new(DaExternalBlobBackend::Filecoin, provider)
    }

    pub fn new(backend: DaExternalBlobBackend, provider: impl Into<String>) -> Self {
        Self {
            backend,
            provider: provider.into(),
            next_id: 0,
            objects: BTreeMap::new(),
        }
    }

    pub fn contains_uri(&self, uri: &str) -> bool {
        self.objects.contains_key(uri)
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    fn next_locator(&mut self, suggested_name: Option<&str>) -> String {
        self.next_id += 1;
        let label = sanitized_locator_fragment(suggested_name);
        match self.backend {
            DaExternalBlobBackend::Ipfs => format!("sdk-ipfs-{}-{label}", self.next_id),
            DaExternalBlobBackend::Arweave => format!("sdk-arweave-{}-{label}", self.next_id),
            DaExternalBlobBackend::Filecoin => {
                format!("sdk-filecoin-deal-{}/piece-{label}", self.next_id)
            }
        }
    }
}

impl BlobClient for InMemoryBlobClient {
    fn backend(&self) -> DaExternalBlobBackend {
        self.backend.clone()
    }

    fn upload_blob(
        &mut self,
        request: BlobUploadRequest<'_>,
    ) -> Result<BlobUploadReceipt, ClientSdkError> {
        if request.bytes.is_empty() {
            return Err(ClientSdkError::InvalidInput(
                "blob upload requires nonempty bytes".into(),
            ));
        }
        let locator = self.next_locator(request.suggested_name);
        let uri = canonical_uri_for_backend(&self.backend, &locator)?;
        self.objects.insert(uri, request.bytes.to_vec());
        let provider_reference = format!("{}:{locator}", self.provider);
        Ok(BlobUploadReceipt {
            backend: self.backend.clone(),
            locator,
            provider: Some(self.provider.clone()),
            provider_reference: Some(provider_reference.clone()),
            operation_reference: Some(provider_reference),
            availability_proof: Some(format!("in-memory-proof-{}", self.next_id)),
        })
    }

    fn fetch_blob(
        &mut self,
        reference: &DaExternalBlobReference,
    ) -> Result<Vec<u8>, ClientSdkError> {
        if reference.backend != self.backend {
            return Err(ClientSdkError::Blob(format!(
                "reference backend {:?} does not match blob client backend {:?}",
                reference.backend, self.backend
            )));
        }
        self.objects
            .get(&reference.uri)
            .cloned()
            .ok_or_else(|| ClientSdkError::Blob(format!("missing blob {}", reference.uri)))
    }
}

pub struct DettaClientSdk<R, B> {
    rpc: R,
    blob_client: B,
}

impl<R, B> DettaClientSdk<R, B> {
    pub fn new(rpc: R, blob_client: B) -> Self {
        Self { rpc, blob_client }
    }

    pub fn rpc_mut(&mut self) -> &mut R {
        &mut self.rpc
    }

    pub fn blob_client_mut(&mut self) -> &mut B {
        &mut self.blob_client
    }

    pub fn into_parts(self) -> (R, B) {
        (self.rpc, self.blob_client)
    }
}

impl<R, B> DettaClientSdk<R, B>
where
    R: DettaRpcClient,
    B: BlobClient,
{
    pub fn upload_external_blob_reference(
        &mut self,
        request: ExternalBlobReferenceUploadRequest,
    ) -> Result<ExternalBlobReferenceUploadReceipt, ClientSdkError> {
        validate_content_type(&request.content_type)?;
        let upload = self.blob_client.upload_blob(BlobUploadRequest {
            content_type: &request.content_type,
            bytes: &request.blob_bytes,
            suggested_name: request.suggested_name.as_deref(),
        })?;
        let reference = reference_from_upload(
            &upload,
            &request.content_type,
            &request.blob_bytes,
            self.blob_client.backend(),
        )?;
        let record = adapter_reference_record(
            &reference.backend,
            &request.record_schema,
            &reference,
            request.signer,
            request.signature,
        )?;
        Ok(ExternalBlobReferenceUploadReceipt {
            upload,
            reference,
            record,
        })
    }

    pub fn publish_social_avatar(
        &mut self,
        request: SocialAvatarPublishRequest,
    ) -> Result<SocialAvatarPublishReceipt, ClientSdkError> {
        request.validate()?;
        if request.ensure_profile_registered {
            self.ensure_application_da_profile_registered(&request.profile)?;
        }

        let external = self.upload_external_blob_reference(ExternalBlobReferenceUploadRequest {
            record_schema: "social.media.reference".into(),
            content_type: request.content_type.clone(),
            blob_bytes: request.avatar_bytes.clone(),
            suggested_name: Some(request.avatar_id.clone()),
            signer: None,
            signature: None,
        })?;
        let media_reference_hash = external.reference.reference_hash()?;
        let post = SocialAvatarPost {
            author: &request.author,
            post_id: &request.avatar_id,
            text: &request.status_text,
            media_reference_hash: &media_reference_hash,
        };
        let post_record = DaRecordEnvelope::new(
            "social.post",
            1,
            "application/json",
            DaRecordEncoding::CanonicalJson,
            serde_json::to_vec(&post)?,
            Some(request.author.clone()),
            Some(format!("sig-{}", request.author)),
        )?;
        let sections = vec![
            ApplicationDaNamespaceSection::new(
                DaNamespace::new("social.feed")?,
                vec![post_record],
            )?,
            ApplicationDaNamespaceSection::new(
                DaNamespace::new("social.media")?,
                vec![external.record.clone()],
            )?,
        ];
        let event_log_root = application_event_log_root_for_sections(&sections)?;
        let payload = ApplicationDaPayload::new(
            &request.profile,
            request.coordinate.clone(),
            DaPayloadKind::MediaManifest,
            request.previous_payload_hash.clone(),
            vec![DaApplicationRoot::new(
                "social.event.log.root",
                event_log_root,
            )?],
            sections,
        )?;
        let production = self.produce_application_da_batch(
            payload.clone(),
            request.data_share_count,
            request.parity_share_count,
            request.certificate_signers.clone(),
        )?;
        let lifecycle_record = DaExternalBlobLifecycleRecord::new(
            external.reference.clone(),
            request.lifecycle_stage,
            request.lifecycle_status,
            external.upload.provider.clone(),
            external.upload.operation_reference.clone(),
            request.observed_at_height,
            request.observed_by,
            request.lifecycle_note,
        )?;
        let persisted_lifecycle_record =
            self.record_external_blob_lifecycle(lifecycle_record.clone())?;
        Ok(SocialAvatarPublishReceipt {
            upload: external.upload,
            reference: external.reference,
            reference_record: external.record,
            payload,
            production,
            lifecycle_record: persisted_lifecycle_record,
        })
    }

    pub fn retrieve_verified_blob(
        &mut self,
        request: BlobRetrievalRequest,
    ) -> Result<BlobRetrievalReceipt, ClientSdkError> {
        let reference = DaExternalBlobReference::from_record_envelope(&request.record)?;
        let bytes = self.blob_client.fetch_blob(&reference)?;
        let local_verification = verify_external_blob_retrieval(
            &reference,
            Some(&bytes),
            request.verifier.clone(),
            request.provider.clone(),
            request.verified_at_height,
        )?;
        if !local_verification.available {
            return Err(ClientSdkError::BlobUnavailable(Box::new(
                local_verification,
            )));
        }
        let verification = self.verify_application_da_external_blob_retrieval(
            request.record,
            Some(bytes.clone()),
            request.verifier,
            request.provider,
            request.verified_at_height,
        )?;
        if !verification.available {
            return Err(ClientSdkError::BlobUnavailable(Box::new(verification)));
        }
        Ok(BlobRetrievalReceipt {
            reference,
            bytes,
            verification,
        })
    }

    fn ensure_application_da_profile_registered(
        &mut self,
        profile: &DaApplicationProfile,
    ) -> Result<(), ClientSdkError> {
        match self.rpc.request(RpcRequest::RegisterApplicationDaProfile {
            profile: Box::new(profile.clone()),
        })? {
            RpcResponse::Ok(RpcResult::ApplicationDaProfile(_)) => Ok(()),
            RpcResponse::Error(error) if error.code == PROFILE_ALREADY_REGISTERED_CODE => Ok(()),
            RpcResponse::Error(error) => Err(ClientSdkError::Rpc(error)),
            RpcResponse::Ok(result) => {
                Err(unexpected("application DA profile registration", result))
            }
        }
    }

    fn produce_application_da_batch(
        &mut self,
        payload: ApplicationDaPayload,
        data_share_count: u32,
        parity_share_count: u32,
        certificate_signers: Vec<String>,
    ) -> Result<ApplicationDaProductionReport, ClientSdkError> {
        match self.rpc.request(RpcRequest::ProduceApplicationDaBatch {
            payload: Box::new(payload),
            data_share_count,
            parity_share_count,
            certificate_signers,
        })? {
            RpcResponse::Ok(RpcResult::ApplicationDaProduction(report)) => Ok(*report),
            RpcResponse::Error(error) => Err(ClientSdkError::Rpc(error)),
            RpcResponse::Ok(result) => Err(unexpected("application DA production report", result)),
        }
    }

    fn record_external_blob_lifecycle(
        &mut self,
        record: DaExternalBlobLifecycleRecord,
    ) -> Result<DaExternalBlobLifecycleRecord, ClientSdkError> {
        match self
            .rpc
            .request(RpcRequest::RecordApplicationDaExternalBlobLifecycle {
                record: Box::new(record),
            })? {
            RpcResponse::Ok(RpcResult::ApplicationDaExternalBlobLifecycleRecord(record)) => {
                Ok(*record)
            }
            RpcResponse::Error(error) => Err(ClientSdkError::Rpc(error)),
            RpcResponse::Ok(result) => Err(unexpected("external blob lifecycle record", result)),
        }
    }

    fn verify_application_da_external_blob_retrieval(
        &mut self,
        record: DaRecordEnvelope,
        blob_bytes: Option<Vec<u8>>,
        verifier: String,
        provider: Option<String>,
        verified_at_height: u64,
    ) -> Result<DaExternalBlobRetrievalVerification, ClientSdkError> {
        match self
            .rpc
            .request(RpcRequest::VerifyApplicationDaExternalBlobRetrieval {
                record: Box::new(record),
                blob_bytes,
                verifier,
                provider,
                verified_at_height,
            })? {
            RpcResponse::Ok(RpcResult::ApplicationDaExternalBlobRetrievalVerification(report)) => {
                Ok(*report)
            }
            RpcResponse::Error(error) => Err(ClientSdkError::Rpc(error)),
            RpcResponse::Ok(result) => {
                Err(unexpected("external blob retrieval verification", result))
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalBlobReferenceUploadRequest {
    pub record_schema: String,
    pub content_type: String,
    pub blob_bytes: Vec<u8>,
    pub suggested_name: Option<String>,
    pub signer: Option<String>,
    pub signature: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalBlobReferenceUploadReceipt {
    pub upload: BlobUploadReceipt,
    pub reference: DaExternalBlobReference,
    pub record: DaRecordEnvelope,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SocialAvatarPublishRequest {
    pub profile: DaApplicationProfile,
    pub coordinate: DaApplicationCoordinate,
    pub author: String,
    pub avatar_id: String,
    pub avatar_bytes: Vec<u8>,
    pub content_type: String,
    pub status_text: String,
    pub previous_payload_hash: Option<String>,
    pub data_share_count: u32,
    pub parity_share_count: u32,
    pub certificate_signers: Vec<String>,
    pub observed_at_height: u64,
    pub observed_by: String,
    pub lifecycle_stage: DaExternalBlobLifecycleStage,
    pub lifecycle_status: DaExternalBlobLifecycleStatus,
    pub lifecycle_note: Option<String>,
    pub ensure_profile_registered: bool,
}

impl SocialAvatarPublishRequest {
    pub fn social_demo(
        author: impl Into<String>,
        avatar_id: impl Into<String>,
        sequence: u64,
        avatar_bytes: Vec<u8>,
        certificate_signers: Vec<String>,
    ) -> Result<Self, ClientSdkError> {
        let profile = DaApplicationProfile::social_demo_v1();
        let author = author.into();
        let avatar_id = avatar_id.into();
        Ok(Self {
            coordinate: DaApplicationCoordinate {
                application_id: profile.application_id.clone(),
                stream_id: format!("user:{author}.avatar"),
                sequence,
                epoch: Some(1),
                parent_hash: None,
                subject_hash: None,
            },
            profile,
            author: author.clone(),
            avatar_id,
            avatar_bytes,
            content_type: "image/png".into(),
            status_text: "avatar updated".into(),
            previous_payload_hash: None,
            data_share_count: 4,
            parity_share_count: 2,
            certificate_signers,
            observed_at_height: sequence,
            observed_by: author,
            lifecycle_stage: DaExternalBlobLifecycleStage::Pinned,
            lifecycle_status: DaExternalBlobLifecycleStatus::Active,
            lifecycle_note: Some("published through detta-client-sdk".into()),
            ensure_profile_registered: true,
        })
    }

    pub fn validate(&self) -> Result<(), ClientSdkError> {
        self.profile.validate()?;
        self.coordinate.validate(&self.profile.coordinate_policy)?;
        if self.coordinate.application_id != self.profile.application_id {
            return Err(ClientSdkError::InvalidInput(
                "avatar coordinate application_id must match profile".into(),
            ));
        }
        if self.author.is_empty() || self.author.trim() != self.author {
            return Err(ClientSdkError::InvalidInput(
                "avatar author must be nonempty and trimmed".into(),
            ));
        }
        if self.avatar_id.is_empty() || self.avatar_id.trim() != self.avatar_id {
            return Err(ClientSdkError::InvalidInput(
                "avatar_id must be nonempty and trimmed".into(),
            ));
        }
        if self.avatar_bytes.is_empty() {
            return Err(ClientSdkError::InvalidInput(
                "avatar bytes must be nonempty".into(),
            ));
        }
        if !self.content_type.starts_with("image/") {
            return Err(ClientSdkError::InvalidInput(
                "avatar content_type must be an image media type".into(),
            ));
        }
        if self.data_share_count == 0 {
            return Err(ClientSdkError::InvalidInput(
                "data_share_count must be positive".into(),
            ));
        }
        if self.certificate_signers.is_empty() {
            return Err(ClientSdkError::InvalidInput(
                "certificate_signers must be nonempty".into(),
            ));
        }
        if self.observed_by.is_empty() || self.observed_by.trim() != self.observed_by {
            return Err(ClientSdkError::InvalidInput(
                "observed_by must be nonempty and trimmed".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SocialAvatarPublishReceipt {
    pub upload: BlobUploadReceipt,
    pub reference: DaExternalBlobReference,
    pub reference_record: DaRecordEnvelope,
    pub payload: ApplicationDaPayload,
    pub production: ApplicationDaProductionReport,
    pub lifecycle_record: DaExternalBlobLifecycleRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlobRetrievalRequest {
    pub record: DaRecordEnvelope,
    pub verifier: String,
    pub provider: Option<String>,
    pub verified_at_height: u64,
}

impl BlobRetrievalRequest {
    pub fn new(
        record: DaRecordEnvelope,
        verifier: impl Into<String>,
        verified_at_height: u64,
    ) -> Self {
        Self {
            record,
            verifier: verifier.into(),
            provider: None,
            verified_at_height,
        }
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlobRetrievalReceipt {
    pub reference: DaExternalBlobReference,
    pub bytes: Vec<u8>,
    pub verification: DaExternalBlobRetrievalVerification,
}

#[derive(Serialize)]
struct SocialAvatarPost<'a> {
    author: &'a str,
    post_id: &'a str,
    text: &'a str,
    media_reference_hash: &'a str,
}

fn reference_from_upload(
    upload: &BlobUploadReceipt,
    content_type: &str,
    blob_bytes: &[u8],
    expected_backend: DaExternalBlobBackend,
) -> Result<DaExternalBlobReference, ClientSdkError> {
    if upload.backend != expected_backend {
        return Err(ClientSdkError::Blob(format!(
            "upload backend {:?} did not match blob client backend {:?}",
            upload.backend, expected_backend
        )));
    }
    match upload.backend {
        DaExternalBlobBackend::Ipfs => IpfsAdapter::default().commit_uploaded_blob(
            &upload.locator,
            content_type,
            blob_bytes,
            upload.provider_reference.clone(),
            upload.availability_proof.clone(),
        ),
        DaExternalBlobBackend::Arweave => ArweaveAdapter::default().commit_uploaded_blob(
            &upload.locator,
            content_type,
            blob_bytes,
            upload.provider_reference.clone(),
            upload.availability_proof.clone(),
        ),
        DaExternalBlobBackend::Filecoin => FilecoinAdapter::default().commit_uploaded_blob(
            &upload.locator,
            content_type,
            blob_bytes,
            upload.provider_reference.clone(),
            upload.availability_proof.clone(),
        ),
    }
    .map_err(ClientSdkError::Da)
}

fn adapter_reference_record(
    backend: &DaExternalBlobBackend,
    record_schema: &str,
    reference: &DaExternalBlobReference,
    signer: Option<String>,
    signature: Option<String>,
) -> Result<DaRecordEnvelope, ClientSdkError> {
    match backend {
        DaExternalBlobBackend::Ipfs => {
            IpfsAdapter::default().reference_record(record_schema, reference, signer, signature)
        }
        DaExternalBlobBackend::Arweave => {
            ArweaveAdapter::default().reference_record(record_schema, reference, signer, signature)
        }
        DaExternalBlobBackend::Filecoin => {
            FilecoinAdapter::default().reference_record(record_schema, reference, signer, signature)
        }
    }
    .map_err(ClientSdkError::Da)
}

fn canonical_uri_for_backend(
    backend: &DaExternalBlobBackend,
    locator: &str,
) -> Result<String, ClientSdkError> {
    match backend {
        DaExternalBlobBackend::Ipfs => IpfsAdapter::default().canonical_uri(locator),
        DaExternalBlobBackend::Arweave => ArweaveAdapter::default().canonical_uri(locator),
        DaExternalBlobBackend::Filecoin => FilecoinAdapter::default().canonical_uri(locator),
    }
    .map_err(ClientSdkError::Da)
}

fn validate_content_type(value: &str) -> Result<(), ClientSdkError> {
    if value.is_empty()
        || value.trim() != value
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(ClientSdkError::InvalidInput(
            "content_type must be nonempty, trimmed, and printable".into(),
        ));
    }
    Ok(())
}

fn sanitized_locator_fragment(value: Option<&str>) -> String {
    let mut out = String::new();
    for byte in value.unwrap_or("blob").bytes() {
        let next = match byte {
            b'a'..=b'z' | b'0'..=b'9' => Some(byte as char),
            b'A'..=b'Z' => Some((byte + 32) as char),
            b'-' | b'_' | b'.' => Some('-'),
            _ => None,
        };
        if let Some(next) = next {
            if out.len() < 64 {
                out.push(next);
            }
        }
    }
    if out.is_empty() {
        "blob".into()
    } else {
        out
    }
}

fn unexpected(expected: &'static str, actual: RpcResult) -> ClientSdkError {
    ClientSdkError::UnexpectedResult {
        expected,
        actual: format!("{actual:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingRpcClient {
        requests: Vec<RpcRequest>,
    }

    impl DettaRpcClient for RecordingRpcClient {
        fn request(&mut self, request: RpcRequest) -> Result<RpcResponse, ClientSdkError> {
            let response = match request.clone() {
                RpcRequest::RegisterApplicationDaProfile { .. } => {
                    RpcResponse::Error(RpcErrorBody {
                        code: PROFILE_ALREADY_REGISTERED_CODE.into(),
                        message: "already registered".into(),
                    })
                }
                RpcRequest::ProduceApplicationDaBatch {
                    payload,
                    data_share_count,
                    parity_share_count,
                    certificate_signers,
                } => {
                    let payload = *payload;
                    RpcResponse::Ok(RpcResult::ApplicationDaProduction(Box::new(
                        ApplicationDaProductionReport {
                            application_id: payload.application_id.clone(),
                            profile_id: payload.profile_id.clone(),
                            coordinate: payload.coordinate.clone(),
                            manifest_hash: "11".repeat(32),
                            certificate_hash: "22".repeat(32),
                            payload_hash: payload.hash()?,
                            namespace_root: payload.namespace_root()?,
                            share_root: "33".repeat(32),
                            payload_bytes: 4096,
                            original_share_count: data_share_count,
                            encoded_share_count: data_share_count + parity_share_count,
                            reconstruction_threshold: data_share_count,
                            certificate_signers,
                        },
                    )))
                }
                RpcRequest::RecordApplicationDaExternalBlobLifecycle { record } => {
                    RpcResponse::Ok(RpcResult::ApplicationDaExternalBlobLifecycleRecord(record))
                }
                RpcRequest::VerifyApplicationDaExternalBlobRetrieval {
                    record,
                    blob_bytes,
                    verifier,
                    provider,
                    verified_at_height,
                } => {
                    let reference = DaExternalBlobReference::from_record_envelope(&record)?;
                    let report = verify_external_blob_retrieval(
                        &reference,
                        blob_bytes.as_deref(),
                        verifier,
                        provider,
                        verified_at_height,
                    )?;
                    RpcResponse::Ok(RpcResult::ApplicationDaExternalBlobRetrievalVerification(
                        Box::new(report),
                    ))
                }
                request => {
                    return Err(ClientSdkError::UnexpectedResult {
                        expected: "SDK-supported request",
                        actual: format!("{request:?}"),
                    });
                }
            };
            self.requests.push(request);
            Ok(response)
        }
    }

    #[test]
    fn sdk_publishes_social_avatar_and_retrieves_verified_blob() {
        let avatar = b"tiny-png-avatar".to_vec();
        let rpc = RecordingRpcClient::default();
        let blob = InMemoryBlobClient::ipfs("ipfs.local");
        let mut sdk = DettaClientSdk::new(rpc, blob);
        let publish = sdk
            .publish_social_avatar(
                SocialAvatarPublishRequest::social_demo(
                    "alice",
                    "avatar-1",
                    1,
                    avatar.clone(),
                    vec!["validator-1".into(), "validator-2".into()],
                )
                .unwrap(),
            )
            .unwrap();

        assert_eq!(publish.reference.backend, DaExternalBlobBackend::Ipfs);
        assert_eq!(publish.reference.content_type, "image/png");
        assert_eq!(publish.reference.size_bytes, avatar.len() as u64);
        assert_eq!(publish.production.original_share_count, 4);
        assert_eq!(publish.production.encoded_share_count, 6);
        assert_eq!(publish.lifecycle_record.reference, publish.reference);

        let retrieval = sdk
            .retrieve_verified_blob(
                BlobRetrievalRequest::new(publish.reference_record.clone(), "bob", 2)
                    .with_provider("ipfs.local"),
            )
            .unwrap();
        assert_eq!(retrieval.bytes, avatar);
        assert!(retrieval.verification.available);
        assert!(retrieval.verification.hash_verified);

        let (rpc, blob) = sdk.into_parts();
        assert_eq!(rpc.requests.len(), 4);
        assert!(blob.contains_uri(&publish.reference.uri));
    }

    #[test]
    fn generic_blob_reference_upload_builds_external_address_record() {
        let rpc = RecordingRpcClient::default();
        let blob = InMemoryBlobClient::arweave("arweave.local");
        let mut sdk = DettaClientSdk::new(rpc, blob);

        let receipt = sdk
            .upload_external_blob_reference(ExternalBlobReferenceUploadRequest {
                record_schema: "social.media.reference".into(),
                content_type: "image/webp".into(),
                blob_bytes: b"avatar-webp".to_vec(),
                suggested_name: Some("avatar.webp".into()),
                signer: None,
                signature: None,
            })
            .unwrap();

        assert_eq!(receipt.reference.backend, DaExternalBlobBackend::Arweave);
        assert!(receipt.reference.uri.starts_with("ar://"));
        assert_eq!(
            receipt.record.encoding,
            DaRecordEncoding::ExternalContentAddress
        );
        assert_eq!(
            DaExternalBlobReference::from_record_envelope(&receipt.record).unwrap(),
            receipt.reference
        );
    }

    #[test]
    fn social_avatar_requires_image_content_type() {
        let mut request = SocialAvatarPublishRequest::social_demo(
            "alice",
            "avatar-1",
            1,
            b"not-image".to_vec(),
            vec!["validator-1".into()],
        )
        .unwrap();
        request.content_type = "application/octet-stream".into();
        assert!(request.validate().is_err());
    }
}
