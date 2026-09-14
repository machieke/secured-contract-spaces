use detta_consensus::{EquivocationEvidence, FinalityCertificate};
use detta_core::{AspectModuleRecord, AspectModuleRoots, Block, GrantKey, Method, StateKey};
use detta_da::{
    ApplicationDaAvailabilityCertificate, ApplicationDaNamespaceSection, ApplicationDaPayload,
    ApplicationDaShareSet, DaApplicationCoordinate, DaApplicationId, DaApplicationProfile,
    DaApplicationRetentionClass, DaApplicationRoot, DaAvailabilityCertificate, DaAvailabilityVote,
    DaChallengeEvidence, DaExternalBlobAdapter, DaExternalBlobAvailabilityChallenge,
    DaExternalBlobBackend, DaExternalBlobChallengeEvidence, DaExternalBlobLifecycleRecord,
    DaExternalBlobLifecycleStage, DaExternalBlobLifecycleStatus,
    DaExternalBlobProviderHealthRecord, DaExternalBlobProviderHealthStatus,
    DaExternalBlobRepairAction, DaExternalBlobRepairJob, DaExternalBlobRepairJobStatus,
    DaNamespace, DaPayloadKind, DaRecordEncoding, DaRecordEnvelope, DaShareChallenge,
    DaShareChallengeResponse, IpfsAdapter,
};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, temp_dir, text, tx_to, ADMIN, BRIDGE_CONTRACT,
    CHAIN_ID, FACTORY_CONTRACT, GOVERNANCE_CONTRACT, TOKEN_CONTRACT, USDC,
};
use detta_e2e::network::{spawn_tcp_persistent_node, spawn_tcp_rpc_server};
use detta_node::PersistentValidatorNode;
use detta_protocol::{ProtocolMessage, ValidatorSetMetadataUpdate, ValidatorSigningKey};
use detta_rpc::{RpcRequest, RpcResponse, RpcResult, SignedClientTransaction, SubscriptionTopic};
use detta_storage::DaRetentionClass;
use std::collections::BTreeSet;

#[test]
fn public_rpc_method_coverage_guard_calls_every_openapi_method() {
    let mut covered = BTreeSet::new();
    call_import_block_method(&mut covered);

    let dir = temp_dir("e2e-rpc-method-coverage");
    let signer_key = validator_key("validator-1", 7);
    let peer_key = validator_key("validator-2", 8);
    let added_key = validator_key("validator-3", 9);
    let signed_transfer = SignedClientTransaction::sign_with_seed(
        tx_to(
            TOKEN_CONTRACT,
            "coverage-rpc-signed-transfer-1",
            "Bob",
            1,
            Method::Transfer,
            vec![principal("Alice"), asset(USDC), amount(1)],
        ),
        [42; 32],
    );
    let mut state = defi_genesis_state();
    state
        .register_account_key("Bob", signed_transfer.public_key_hex.clone())
        .unwrap();
    let mut node = PersistentValidatorNode::bootstrap_with_validator_set(
        "validator-1",
        state,
        dir.path(),
        "detta-testnet",
        vec![signer_key.public_key(), peer_key.public_key()],
    )
    .unwrap();

    let setup_block = seed_coverage_state(&mut node);
    let da_block = seed_da_coverage_state(&mut node);
    let da_manifest_hash = da_block
        .header
        .data_availability
        .as_ref()
        .unwrap()
        .manifest_hash
        .clone();
    let da_share_set = node.load_da_share_set(&da_manifest_hash).unwrap();
    let da_certificate = DaAvailabilityCertificate::from_manifest(
        &da_share_set.manifest,
        vec!["validator-1".into(), "validator-2".into()],
    )
    .unwrap();
    let da_certificate_hash = node.persist_da_certificate(&da_certificate).unwrap();
    let da_vote = DaAvailabilityVote::from_manifest_with_custody(
        &da_share_set.manifest,
        "validator-2",
        [0],
        [],
    )
    .unwrap();
    let da_challenge =
        DaShareChallenge::from_availability_vote(&da_vote, "validator-1", 0, 12).unwrap();
    let da_challenge_id = node
        .persist_da_share_challenge(da_challenge.clone())
        .unwrap();
    let mut invalid_da_share = da_share_set.shares[0].clone();
    invalid_da_share.bytes[0] ^= 0x01;
    let da_challenge_response =
        DaShareChallengeResponse::from_share(&da_challenge, invalid_da_share).unwrap();
    node.persist_da_share_challenge_response(da_challenge_response.clone())
        .unwrap();
    let da_challenge_evidence = DaChallengeEvidence::invalid_response(
        &da_challenge,
        &da_challenge_response,
        &da_share_set.manifest,
        "validator-1",
        10,
    )
    .unwrap();
    node.persist_da_challenge_evidence(da_challenge_evidence)
        .unwrap();
    node.persist_finality_certificate(&FinalityCertificate {
        height: setup_block.header.height,
        block_hash: setup_block.block_hash(),
        signers: vec!["validator-1".into(), "validator-2".into()],
    })
    .unwrap();
    node.persist_equivocation_evidence(EquivocationEvidence {
        validator_id: "validator-1".into(),
        height: 7,
        first_block_hash: "coverage-block-a".into(),
        second_block_hash: "coverage-block-b".into(),
    })
    .unwrap();

    let metadata_update = ValidatorSetMetadataUpdate {
        update_id: "coverage-validator-set-update-1".into(),
        add_validators: vec![added_key.public_key()],
        remove_validators: vec![],
        expires_at_height: None,
    };
    let metadata_authorization = signer_key
        .sign_message(
            "detta-testnet",
            CHAIN_ID,
            ProtocolMessage::ValidatorSetMetadataUpdate(metadata_update),
        )
        .unwrap();
    let aspect_module_hash = aspect_module_record().module_hash;
    let app_profile = DaApplicationProfile::social_demo_v1();
    let app_profile_id = app_profile.profile_id().unwrap();
    let app_coordinate = DaApplicationCoordinate {
        application_id: DaApplicationId::new("social.demo").unwrap(),
        stream_id: "main".into(),
        sequence: 1,
        epoch: Some(1),
        parent_hash: None,
        subject_hash: None,
    };
    let app_payload = ApplicationDaPayload::new(
        &app_profile,
        app_coordinate.clone(),
        DaPayloadKind::Batch,
        None,
        vec![DaApplicationRoot::new("social.event.log.root", "11".repeat(32)).unwrap()],
        vec![ApplicationDaNamespaceSection::new(
            DaNamespace::new("social.feed").unwrap(),
            vec![DaRecordEnvelope::new(
                "social.post",
                1,
                "application/json",
                DaRecordEncoding::CanonicalJson,
                br#"{"author":"alice","post_id":"post-1","text":"coverage"}"#.to_vec(),
                Some("alice".into()),
                Some("signature-1".into()),
            )
            .unwrap()],
        )
        .unwrap()],
    )
    .unwrap();
    let app_share_set =
        ApplicationDaShareSet::from_payload_reed_solomon(&app_payload, &app_profile, 4, 2).unwrap();
    let app_storage = detta_storage::FileStorage::open(dir.path()).unwrap();
    let app_manifest_hash = app_storage
        .commit_application_da_share_set(&app_share_set, &app_profile)
        .unwrap();
    let app_application_root = app_share_set.manifest.application_root.clone().unwrap();
    let app_certificate = ApplicationDaAvailabilityCertificate::from_manifest(
        &app_share_set.manifest,
        &app_profile,
        vec!["validator-1".into(), "validator-2".into()],
    )
    .unwrap();
    let app_certificate_hash = app_storage
        .commit_application_da_certificate(&app_certificate)
        .unwrap();
    let mut app_profile_v2 = DaApplicationProfile::social_demo_v1();
    app_profile_v2.profile_version = 2;
    app_profile_v2.profile_name = "Social Demo DA v2".into();
    let app_profile_v2_id = app_profile_v2.profile_id().unwrap();
    let blob_bytes = b"coverage avatar bytes".to_vec();
    let blob_adapter = IpfsAdapter::default();
    let blob_reference = blob_adapter
        .commit_uploaded_blob(
            "bafycoverageavatar",
            "image/png",
            &blob_bytes,
            Some("pin.provider".into()),
            None,
        )
        .unwrap();
    let blob_record = blob_adapter
        .reference_record("social.media.reference", &blob_reference, None, None)
        .unwrap();
    let blob_failed_retrieval = detta_da::verify_external_blob_retrieval(
        &blob_reference,
        None,
        "validator-2",
        Some("pin.provider".into()),
        6,
    )
    .unwrap();
    let blob_lifecycle = DaExternalBlobLifecycleRecord::new(
        blob_reference.clone(),
        DaExternalBlobLifecycleStage::Pinned,
        DaExternalBlobLifecycleStatus::Active,
        Some("pin.provider".into()),
        Some("pin-coverage-1".into()),
        5,
        "validator-1",
        None,
    )
    .unwrap();
    let blob_lifecycle_id = blob_lifecycle.record_hash().unwrap();
    let blob_health = DaExternalBlobProviderHealthRecord::new(
        DaExternalBlobBackend::Ipfs,
        "pin.provider",
        6,
        DaExternalBlobProviderHealthStatus::Unavailable,
        false,
        None,
        Some("coverage timeout".into()),
        "validator-2",
    )
    .unwrap();
    let blob_health_id = blob_health.record_hash().unwrap();
    let blob_repair = DaExternalBlobRepairJob::new(
        blob_reference.clone(),
        vec![DaExternalBlobBackend::Ipfs, DaExternalBlobBackend::Arweave],
        DaExternalBlobRepairJobStatus::Open,
        "coverage repin and archive",
        7,
        7,
        Some("operator-1".into()),
        vec![blob_failed_retrieval.report_hash().unwrap()],
        vec![DaExternalBlobRepairAction::Repin {
            backend: DaExternalBlobBackend::Ipfs,
            provider: "backup.pin.provider".into(),
        }],
    )
    .unwrap();
    let blob_repair_id = blob_repair.job_hash().unwrap();
    let blob_challenge = DaExternalBlobAvailabilityChallenge::new(
        blob_reference,
        Some("pin.provider".into()),
        "validator-3",
        6,
        10,
        "coverage blob unavailable",
    )
    .unwrap();
    let blob_evidence = DaExternalBlobChallengeEvidence::unavailable(
        blob_challenge,
        "validator-4",
        8,
        vec![blob_failed_retrieval],
        vec![blob_health.clone()],
        Some(blob_repair.clone()),
    )
    .unwrap();
    let blob_evidence_id = blob_evidence.evidence_hash().unwrap();

    let (addr, server) = spawn_tcp_persistent_node(node).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();

    let subscription_id = match call_ok(
        &mut client,
        &mut covered,
        RpcRequest::Subscribe {
            topics: vec![
                SubscriptionTopic::Blocks,
                SubscriptionTopic::Receipts,
                SubscriptionTopic::Events,
                SubscriptionTopic::Finality,
            ],
        },
    ) {
        RpcResult::SubscriptionStatus(status) => status.subscription_id,
        result => panic!("expected subscription status, got {result:?}"),
    };

    call_ok(&mut client, &mut covered, RpcRequest::GetNodeHealth);
    call_ok(&mut client, &mut covered, RpcRequest::GetOperatorMetrics);
    call_ok(&mut client, &mut covered, RpcRequest::GetOperatorAlerts);
    call_ok(&mut client, &mut covered, RpcRequest::GetMempoolStatus);
    call_ok(&mut client, &mut covered, RpcRequest::GetStateRoot);
    call_ok(&mut client, &mut covered, RpcRequest::GetSnapshot);
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetPersistentNodeSnapshotRoots,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetSnapshotMetadataRootStatus,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetSnapshotSyncClientMetrics,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetRequiredSnapshotMetadataRoots,
    );

    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::SubmitSignedTransaction {
            signed: signed_transfer,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::SubmitTransaction {
            transaction: tx_to(
                TOKEN_CONTRACT,
                "coverage-rpc-transfer-1",
                "Bob",
                2,
                Method::Transfer,
                vec![principal("Alice"), asset(USDC), amount(1)],
            ),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::ProduceBlock {
            height: 3,
            timestamp: 3_000,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::SubmitTransaction {
            transaction: tx_to(
                TOKEN_CONTRACT,
                "coverage-rpc-da-transfer-1",
                "Bob",
                3,
                Method::Transfer,
                vec![principal("Alice"), asset(USDC), amount(1)],
            ),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::ProduceDaBlock {
            height: 4,
            timestamp: 4_000,
            share_size_bytes: 128,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::RegisterApplicationDaProfile {
            profile: Box::new(app_profile.clone()),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::ClaimApplicationDaId {
            application_id: "social.demo".into(),
            owner: "coverage-governance".into(),
            delegated_profile_governance: Vec::new(),
            reason: "coverage application id claim".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::UpdateApplicationDaIdOwner {
            application_id: "social.demo".into(),
            owner: "coverage-council".into(),
            delegated_profile_governance: vec!["coverage-governance".into()],
            requested_by: "coverage-governance".into(),
            reason: "coverage owner update".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaIdOwner {
            application_id: "social.demo".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaIdOwners,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::PlanApplicationDaProfileRegistration {
            profile: Box::new(app_profile_v2),
            execute_after_sequence: 5,
            requested_by: "coverage-governance".into(),
            reason: "coverage profile registration plan".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::ProduceBlock {
            height: 5,
            timestamp: 5_000,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::ActivateApplicationDaProfile {
            profile_id: app_profile_v2_id.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::RecordApplicationDaProfileMigration {
            profile_id: app_profile_v2_id,
            supersedes_profile_id: app_profile_id.clone(),
            migration_evidence_hash: "66".repeat(32),
            requested_by: "coverage-governance".into(),
            reason: "coverage migration evidence".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::ProduceApplicationDaBatch {
            payload: Box::new(app_payload.clone()),
            data_share_count: 4,
            parity_share_count: 2,
            certificate_signers: vec!["validator-1".into(), "validator-2".into()],
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::DeprecateApplicationDaProfile {
            profile_id: app_profile_id.clone(),
            requested_by: "coverage-governance".into(),
            reason: "coverage superseded by v2".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaProfileLifecycleRecords,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::RecordApplicationDaExternalBlobLifecycle {
            record: Box::new(blob_lifecycle),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::RecordApplicationDaExternalBlobProviderHealth {
            record: Box::new(blob_health),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::RecordApplicationDaExternalBlobRepairJob {
            job: Box::new(blob_repair),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::RecordApplicationDaExternalBlobChallengeEvidence {
            evidence: Box::new(blob_evidence),
        },
    );
    match call_ok(
        &mut client,
        &mut covered,
        RpcRequest::VerifyApplicationDaExternalBlobRetrieval {
            record: Box::new(blob_record),
            blob_bytes: Some(blob_bytes),
            verifier: "validator-1".into(),
            provider: Some("pin.provider".into()),
            verified_at_height: 9,
        },
    ) {
        RpcResult::ApplicationDaExternalBlobRetrievalVerification(report) => {
            assert!(report.available);
            assert!(report.hash_verified);
        }
        result => panic!("expected external blob retrieval verification, got {result:?}"),
    }
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaExternalBlobLifecycleRecord {
            record_id: blob_lifecycle_id,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaExternalBlobLifecycleRecords,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaExternalBlobProviderHealthRecord {
            record_id: blob_health_id,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaExternalBlobProviderHealthRecords,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaExternalBlobRepairJob {
            job_id: blob_repair_id,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaExternalBlobRepairJobs,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaExternalBlobChallengeEvidence {
            evidence_id: blob_evidence_id,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaExternalBlobChallengeEvidenceRecords,
    );

    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetTransaction {
            tx_hash: "coverage-rpc-transfer-1".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetReceipt {
            tx_hash: "coverage-rpc-transfer-1".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetReceiptProof {
            height: 3,
            index: 0,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetBlock { height: 1 },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetBlocksPage {
            start_height: 1,
            limit: 10,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetFinalityCertificate { height: 1 },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetSlashingRecord {
            validator_id: "validator-1".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaManifest {
            manifest_hash: da_manifest_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaShare {
            manifest_hash: da_manifest_hash.clone(),
            index: 0,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaCertificate {
            certificate_hash: da_certificate_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaChallengeRecord {
            challenge_id: da_challenge_id,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaPayload {
            manifest_hash: da_manifest_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaNamespace {
            manifest_hash: da_manifest_hash.clone(),
            namespace: "detta.tx".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaSampleProofs {
            manifest_hash: da_manifest_hash.clone(),
            client_randomness: "coverage-randomness".into(),
            sample_count: 2,
            namespaces: vec!["detta.tx".into()],
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaStatus {
            manifest_hash: da_manifest_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaRepairStatus {
            manifest_hash: da_manifest_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaCodingFraudProof {
            manifest_hash: da_manifest_hash.clone(),
        },
    );
    call_ok(&mut client, &mut covered, RpcRequest::GetDaStorageStats);
    call_ok(&mut client, &mut covered, RpcRequest::GetDaRetentionAudit);
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaRetentionPrunePlan,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaManifestIndexByHeight {
            height: da_share_set.manifest.height,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaManifestIndexByBlockHash {
            block_hash: da_share_set.manifest.block_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaManifestIndexByNamespace {
            namespace: "detta.tx".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaManifestIndexByRetentionClass {
            class: DaRetentionClass::Hot,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaCertificateIndexByManifest {
            manifest_hash: da_manifest_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaCertificateIndexByHeight {
            height: da_share_set.manifest.height,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetDaCertificateIndexByBlockHash {
            block_hash: da_share_set.manifest.block_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaProfile {
            profile_id: app_profile_id.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaProfileIndexByApplicationId {
            application_id: "social.demo".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaProfileIndexByApplicationVersion {
            application_id: "social.demo".into(),
            profile_version: 1,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaManifest {
            manifest_hash: app_manifest_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaShare {
            manifest_hash: app_manifest_hash.clone(),
            index: 0,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaCertificate {
            certificate_hash: app_certificate_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaPayload {
            manifest_hash: app_manifest_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaReconstructedPayload {
            manifest_hash: app_manifest_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaNamespace {
            manifest_hash: app_manifest_hash.clone(),
            namespace: "social.feed".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaSampleProofs {
            manifest_hash: app_manifest_hash.clone(),
            client_randomness: "application-da-coverage-randomness".into(),
            sample_count: 2,
            namespaces: vec!["social.feed".into()],
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaStatus {
            manifest_hash: app_manifest_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaRepairStatus {
            manifest_hash: app_manifest_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaRetentionAudit,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaRetentionPrunePlan,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaManifestIndexByApplicationId {
            application_id: "social.demo".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaManifestIndexByProfileId {
            profile_id: app_profile_id.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaManifestIndexByCoordinate {
            coordinate: app_coordinate.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaManifestIndexByNamespace {
            namespace: "social.feed".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaManifestIndexByRetentionClass {
            class: DaApplicationRetentionClass::Warm,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaManifestIndexByApplicationRoot {
            application_root: app_application_root,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaCertificateIndexByManifest {
            manifest_hash: app_manifest_hash,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaCertificateIndexByApplicationId {
            application_id: "social.demo".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaCertificateIndexByProfileId {
            profile_id: app_profile_id,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetApplicationDaCertificateIndexByCoordinate {
            coordinate: app_coordinate,
        },
    );

    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetBalance {
            contract: TOKEN_CONTRACT.into(),
            owner: "Bob".into(),
            asset: USDC.into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetTotalSupply {
            contract: TOKEN_CONTRACT.into(),
            asset: USDC.into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetStorageProof {
            key: StateKey::Balance {
                contract: TOKEN_CONTRACT.into(),
                owner: "Bob".into(),
                asset: USDC.into(),
            },
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetStorageNonInclusionProof {
            key: StateKey::Balance {
                contract: TOKEN_CONTRACT.into(),
                owner: "MissingAccount".into(),
                asset: USDC.into(),
            },
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetRegistryProof {
            key: GrantKey::Allowance {
                contract: TOKEN_CONTRACT.into(),
                owner: "Alice".into(),
                spender: "Bob".into(),
                asset: USDC.into(),
            },
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetRegistryNonInclusionProof {
            key: GrantKey::Allowance {
                contract: TOKEN_CONTRACT.into(),
                owner: "Alice".into(),
                spender: "Mallory".into(),
                asset: USDC.into(),
            },
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetOutboxMessageProof { index: 0 },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetEventProof { index: 0 },
    );
    call_ok(&mut client, &mut covered, RpcRequest::GetEvents);
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetEventsPage {
            offset: 0,
            limit: 10,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetSubscriptionEvents {
            subscription_id,
            from_sequence: 0,
            limit: 10,
        },
    );

    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetContract {
            contract: TOKEN_CONTRACT.into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetAspectModule {
            module_hash: aspect_module_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetAspectModuleProof {
            module_hash: aspect_module_hash.clone(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetAspectModuleArtifacts {
            module_hash: aspect_module_hash,
        },
    );
    call_ok(&mut client, &mut covered, RpcRequest::GetAspectModules);
    call_ok(&mut client, &mut covered, RpcRequest::GetScheduledUpgrades);
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetScheduledPolicyUpdates,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetUpgradeRehearsalReport {
            upgrade_id: "coverage-upgrade-1".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::EvaluateRestrictedScript {
            source: "(state-get (balance TokenA Alice USDC))\n(pure-add 40 2)".into(),
            max_steps: 100,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetRestrictedEvaluatorFixtureInventory,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::ProposeValidatorSetMetadataUpdate {
            authorization: metadata_authorization,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetValidatorSetMetadataUpdateStatus {
            update_id: "coverage-validator-set-update-1".into(),
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetValidatorSetMetadataAuditRecords {
            offset: 0,
            limit: 10,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetSnapshotImportAuditRecords {
            offset: 0,
            limit: 10,
        },
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetSnapshotImportAuditRoot,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetSnapshotImportAuditConfigRoot,
    );
    call_ok(
        &mut client,
        &mut covered,
        RpcRequest::GetSnapshotImportAuditConfig,
    );

    client.close();
    server.join().unwrap();

    assert_eq!(covered, openapi_rpc_methods());
}

fn seed_coverage_state(node: &mut PersistentValidatorNode) -> Block {
    node.submit_transaction(tx_to(
        TOKEN_CONTRACT,
        "coverage-setup-transfer-1",
        "Alice",
        1,
        Method::Transfer,
        vec![principal("Bob"), asset(USDC), amount(10)],
    ))
    .unwrap();
    node.submit_transaction(tx_to(
        TOKEN_CONTRACT,
        "coverage-setup-approve-1",
        "Alice",
        2,
        Method::Approve,
        vec![principal("Bob"), asset(USDC), amount(25)],
    ))
    .unwrap();
    node.submit_transaction(tx_to(
        BRIDGE_CONTRACT,
        "coverage-setup-bridge-queue-1",
        "Alice",
        3,
        Method::QueueBridgeMessage,
        vec![
            text("ShardB"),
            text("BridgeB"),
            text("coverage-outbound-msg-1"),
            principal("Bob"),
            asset(USDC),
            amount(5),
        ],
    ))
    .unwrap();
    let module = aspect_module_record();
    node.submit_transaction(tx_to(
        FACTORY_CONTRACT,
        "coverage-setup-submit-aspect-module-1",
        "Alice",
        4,
        Method::SubmitAspectModule,
        aspect_module_submission_args(&module),
    ))
    .unwrap();
    node.submit_transaction(tx_to(
        GOVERNANCE_CONTRACT,
        "coverage-setup-schedule-upgrade-1",
        ADMIN,
        1,
        Method::ScheduleUpgrade,
        vec![text("coverage-upgrade-1"), text("coverage-token-code-v2")],
    ))
    .unwrap();
    node.submit_transaction(tx_to(
        GOVERNANCE_CONTRACT,
        "coverage-setup-schedule-policy-1",
        ADMIN,
        2,
        Method::SchedulePolicyUpdate,
        vec![
            text("coverage-policy-1"),
            text("transfer"),
            text("registryWrite"),
        ],
    ))
    .unwrap();
    node.produce_block(1, 1_000).unwrap()
}

fn seed_da_coverage_state(node: &mut PersistentValidatorNode) -> Block {
    node.submit_transaction(tx_to(
        TOKEN_CONTRACT,
        "coverage-da-transfer-1",
        "Alice",
        5,
        Method::Transfer,
        vec![principal("Bob"), asset(USDC), amount(1)],
    ))
    .unwrap();
    node.produce_block_with_data_availability(2, 2_000, 128)
        .unwrap()
}

fn aspect_module_record() -> AspectModuleRecord {
    AspectModuleRecord::new(
        "MinimalTransferToken",
        "NormalizedBalanceFirst.v1",
        AspectModuleRoots {
            source_root: "source-root".into(),
            ir_root: "ir-root".into(),
            abi_root: "abi-root".into(),
            policy_root: "policy-root".into(),
            storage_schema_root: "storage-schema-root".into(),
            registry_schema_root: "registry-schema-root".into(),
            invariant_root: "invariant-root".into(),
        },
    )
}

fn aspect_module_submission_args(module: &AspectModuleRecord) -> Vec<detta_core::Argument> {
    vec![
        text(&module.module_id),
        text(&module.taxonomy_version),
        text(&module.source_root),
        text(&module.ir_root),
        text(&module.abi_root),
        text(&module.policy_root),
        text(&module.storage_schema_root),
        text(&module.registry_schema_root),
        text(&module.invariant_root),
    ]
}

fn call_import_block_method(covered: &mut BTreeSet<String>) {
    let (addr, server) = spawn_tcp_rpc_server(defi_genesis_state()).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();
    let block = import_fixture_block();
    assert!(matches!(
        call_ok(
            &mut client,
            covered,
            RpcRequest::ImportBlock {
                block: Box::new(block)
            },
        ),
        RpcResult::Imported
    ));
    client.close();
    server.join().unwrap();
}

fn import_fixture_block() -> Block {
    let state = defi_genesis_state();
    let (block, _) = state.build_block(
        1,
        vec![tx_to(
            TOKEN_CONTRACT,
            "coverage-import-transfer-1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(3)],
        )],
        1_000,
        "validator-import",
        "coverage-import-cert",
    );
    block
}

fn call_ok(
    client: &mut TcpRpcClient,
    covered: &mut BTreeSet<String>,
    request: RpcRequest,
) -> RpcResult {
    covered.insert(method_tag(&request));
    match client.request(request).unwrap() {
        RpcResponse::Ok(result) => result,
        RpcResponse::Error(error) => panic!("expected RPC success, got {error:?}"),
    }
}

fn method_tag(request: &RpcRequest) -> String {
    serde_json::to_value(request).unwrap()["method"]
        .as_str()
        .unwrap()
        .to_string()
}

fn openapi_rpc_methods() -> BTreeSet<String> {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../detta-rpc-openapi.json")).unwrap();
    schema["components"]["schemas"]["RpcMethod"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|method| method.as_str().unwrap().to_string())
        .collect()
}

fn validator_key(validator_id: &str, seed_byte: u8) -> ValidatorSigningKey {
    ValidatorSigningKey::from_seed(validator_id, "consensus-key-1", [seed_byte; 32])
}
