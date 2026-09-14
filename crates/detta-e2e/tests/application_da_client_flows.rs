use detta_core::{Block, Method};
use detta_da::{
    verify_application_share_against_manifest, ApplicationDaNamespaceSection, ApplicationDaPayload,
    DaApplicationCoordinate, DaApplicationId, DaApplicationPrivacyMode, DaApplicationProfile,
    DaApplicationRetentionClass, DaApplicationRoot, DaApplicationValidationMode, DaNamespace,
    DaNamespacePolicy, DaNamespaceRequirement, DaPayloadKind, DaRecordEncoding, DaRecordEnvelope,
};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, temp_dir, tx_to, TOKEN_CONTRACT, USDC,
};
use detta_e2e::network::spawn_tcp_persistent_node;
use detta_node::PersistentValidatorNode;
use detta_rpc::{ApplicationDaProductionReport, RpcRequest, RpcResult};
use detta_storage::FileStorage;

#[test]
fn client_registers_social_profile_produces_batch_and_retrieves_da_evidence() {
    let dir = temp_dir("application-da-client-flow");
    let node = PersistentValidatorNode::bootstrap("validator-1", defi_genesis_state(), dir.path())
        .unwrap();
    let (addr, server) = spawn_tcp_persistent_node(node).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();
    let profile = DaApplicationProfile::social_demo_v1();
    let profile_id = profile.profile_id().unwrap();
    let payload = social_demo_payload(&profile);

    let registration = match client
        .ok(RpcRequest::RegisterApplicationDaProfile {
            profile: Box::new(profile.clone()),
        })
        .unwrap()
    {
        RpcResult::ApplicationDaProfile(registration) => registration,
        result => panic!("expected application DA profile registration, got {result:?}"),
    };
    assert_eq!(registration.profile_id, profile_id);

    let production = match client
        .ok(RpcRequest::ProduceApplicationDaBatch {
            payload: Box::new(payload.clone()),
            data_share_count: 4,
            parity_share_count: 2,
            certificate_signers: vec!["validator-1".into(), "validator-2".into()],
        })
        .unwrap()
    {
        RpcResult::ApplicationDaProduction(report) => report,
        result => panic!("expected application DA production report, got {result:?}"),
    };
    assert_eq!(production.profile_id, registration.profile_id);
    assert_eq!(production.payload_hash, payload.hash().unwrap());
    assert_eq!(production.original_share_count, 4);
    assert_eq!(production.encoded_share_count, 6);

    let manifest = match client
        .ok(RpcRequest::GetApplicationDaManifest {
            manifest_hash: production.manifest_hash.clone(),
        })
        .unwrap()
    {
        RpcResult::ApplicationDaManifest(manifest) => manifest,
        result => panic!("expected application DA manifest, got {result:?}"),
    };
    assert_eq!(manifest.payload_hash, production.payload_hash);
    assert_eq!(manifest.share_root, production.share_root);

    match client
        .ok(RpcRequest::GetApplicationDaCertificate {
            certificate_hash: production.certificate_hash.clone(),
        })
        .unwrap()
    {
        RpcResult::ApplicationDaAvailabilityCertificate(certificate) => {
            assert_eq!(certificate.manifest_hash, production.manifest_hash);
            assert_eq!(
                certificate.signers,
                vec!["validator-1".to_string(), "validator-2".to_string()]
            );
        }
        result => panic!("expected application DA certificate, got {result:?}"),
    }

    match client
        .ok(RpcRequest::GetApplicationDaShare {
            manifest_hash: production.manifest_hash.clone(),
            index: 0,
        })
        .unwrap()
    {
        RpcResult::ApplicationDaShare(share) => {
            assert_eq!(share.index, 0);
            assert_eq!(share.manifest_hash, production.manifest_hash);
        }
        result => panic!("expected application DA share, got {result:?}"),
    }

    for request in [
        RpcRequest::GetApplicationDaPayload {
            manifest_hash: production.manifest_hash.clone(),
        },
        RpcRequest::GetApplicationDaReconstructedPayload {
            manifest_hash: production.manifest_hash.clone(),
        },
    ] {
        match client.ok(request).unwrap() {
            RpcResult::ApplicationDaPayload(retrieved) => {
                assert_eq!(*retrieved, payload.canonicalized());
            }
            result => panic!("expected application DA payload, got {result:?}"),
        }
    }

    match client
        .ok(RpcRequest::GetApplicationDaNamespace {
            manifest_hash: production.manifest_hash.clone(),
            namespace: "social.feed".into(),
        })
        .unwrap()
    {
        RpcResult::ApplicationDaNamespace(section) => {
            assert_eq!(*section, payload.canonicalized().namespaces[0]);
        }
        result => panic!("expected application DA namespace, got {result:?}"),
    }

    match client
        .ok(RpcRequest::GetApplicationDaSampleProofs {
            manifest_hash: production.manifest_hash.clone(),
            client_randomness: "social-demo-client-randomness".into(),
            sample_count: 3,
            namespaces: vec!["social.feed".into()],
        })
        .unwrap()
    {
        RpcResult::ApplicationDaSampleProofs(bundle) => {
            assert!(bundle.verification.valid);
            assert_eq!(bundle.sample_proofs.len(), 3);
            assert_eq!(bundle.namespace_proofs.len(), 1);
        }
        result => panic!("expected application DA sample proofs, got {result:?}"),
    }

    match client
        .ok(RpcRequest::GetApplicationDaStatus {
            manifest_hash: production.manifest_hash.clone(),
        })
        .unwrap()
    {
        RpcResult::ApplicationDaStatus(status) => {
            assert!(status.manifest_available);
            assert!(status.certificate_available);
            assert!(status.payload_reconstructable);
            assert_eq!(status.application_id, Some(profile.application_id.clone()));
            assert_eq!(status.profile_id, Some(registration.profile_id.clone()));
            assert_eq!(status.coordinate, Some(payload.coordinate.clone()));
            assert_eq!(status.missing_share_indices, Vec::<u32>::new());
        }
        result => panic!("expected application DA status, got {result:?}"),
    }

    match client
        .ok(RpcRequest::GetApplicationDaRepairStatus {
            manifest_hash: production.manifest_hash.clone(),
        })
        .unwrap()
    {
        RpcResult::ApplicationDaRepairStatus(status) => {
            assert!(!status.repair_needed);
            assert_eq!(status.pending_repair_count, 0);
            assert_eq!(status.missing_share_indices, Vec::<u32>::new());
            assert!(status.payload_reconstructable);
        }
        result => panic!("expected application DA repair status, got {result:?}"),
    }

    match client
        .ok(RpcRequest::GetApplicationDaRetentionAudit)
        .unwrap()
    {
        RpcResult::ApplicationDaRetentionAudit(audit) => {
            assert_eq!(audit.manifest_count, 1);
            assert_eq!(audit.unsatisfied_manifest_count, 0);
            assert_eq!(audit.entries[0].manifest_hash, production.manifest_hash);
        }
        result => panic!("expected application DA retention audit, got {result:?}"),
    }

    match client
        .ok(RpcRequest::GetApplicationDaRetentionPrunePlan)
        .unwrap()
    {
        RpcResult::ApplicationDaRetentionPrunePlan(prune_plan) => {
            assert_eq!(prune_plan.manifest_count, 1);
            assert_eq!(prune_plan.prunable_payload_count, 0);
        }
        result => panic!("expected application DA retention prune plan, got {result:?}"),
    }

    match client
        .ok(RpcRequest::GetApplicationDaManifestIndexByCoordinate {
            coordinate: payload.coordinate.clone(),
        })
        .unwrap()
    {
        RpcResult::ApplicationDaManifestIndex(index) => {
            assert_eq!(index.len(), 1);
            assert_eq!(index[0].manifest_hash, production.manifest_hash);
        }
        result => panic!("expected application DA manifest index, got {result:?}"),
    }

    match client
        .ok(RpcRequest::GetApplicationDaCertificateIndexByProfileId {
            profile_id: registration.profile_id,
        })
        .unwrap()
    {
        RpcResult::ApplicationDaCertificateIndex(index) => {
            assert_eq!(index.len(), 1);
            assert_eq!(index[0].certificate_hash, production.certificate_hash);
        }
        result => panic!("expected application DA certificate index, got {result:?}"),
    }

    let mut unknown_profile_payload = payload;
    unknown_profile_payload.profile_id = "22".repeat(32);
    let error = client
        .error(RpcRequest::ProduceApplicationDaBatch {
            payload: Box::new(unknown_profile_payload),
            data_share_count: 4,
            parity_share_count: 2,
            certificate_signers: vec!["validator-1".into()],
        })
        .unwrap();
    assert_eq!(error.code, "rpc.application_da_profile_not_found");

    client.close();
    server.join().unwrap();
    std::fs::remove_dir_all(dir.path()).unwrap();
}

#[test]
fn client_produces_legacy_detta_da_and_generic_application_batches_in_one_node() {
    let dir = temp_dir("application-da-detta-and-social");
    let node = PersistentValidatorNode::bootstrap("validator-1", defi_genesis_state(), dir.path())
        .unwrap();
    let (addr, server) = spawn_tcp_persistent_node(node).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();

    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-application-da-legacy-transfer-1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(5)],
        ),
    );
    let da_block = produce_da_block(&mut client, 1, 1_000, 128);
    let commitment = da_block
        .header
        .data_availability
        .clone()
        .expect("legacy DA block must carry DA commitment");
    let legacy_manifest = da_manifest(&mut client, &commitment.manifest_hash);
    assert_eq!(legacy_manifest.payload_hash, commitment.payload_root);
    assert_eq!(legacy_manifest.share_root, commitment.share_root);
    assert_eq!(
        da_payload(&mut client, &commitment.manifest_hash).height,
        da_block.header.height
    );
    match client
        .ok(RpcRequest::GetDaStatus {
            manifest_hash: commitment.manifest_hash.clone(),
        })
        .unwrap()
    {
        RpcResult::DaStatus(status) => {
            assert!(status.manifest_available);
            assert!(status.payload_reconstructable);
            assert_eq!(status.stored_share_count, status.expected_share_count);
        }
        result => panic!("expected legacy DA status, got {result:?}"),
    }

    let defi_profile = DaApplicationProfile::detta_defi_v1();
    let defi_profile_id = register_application_profile(&mut client, &defi_profile);
    let defi_payload = detta_defi_application_payload(&defi_profile, &da_block);
    let defi_report = produce_application_batch(&mut client, defi_payload.clone());
    assert_eq!(defi_report.profile_id, defi_profile_id);
    assert_eq!(defi_report.application_id.0, "detta.defi");
    assert_eq!(
        application_payload(&mut client, &defi_report.manifest_hash),
        defi_payload.canonicalized()
    );

    let social_profile = DaApplicationProfile::social_demo_v1();
    let social_profile_id = register_application_profile(&mut client, &social_profile);
    let social_payload = social_demo_payload(&social_profile);
    let social_report = produce_application_batch(&mut client, social_payload);
    assert_eq!(social_report.profile_id, social_profile_id);
    assert_eq!(social_report.application_id.0, "social.demo");

    assert_application_manifest_index_contains(
        &mut client,
        RpcRequest::GetApplicationDaManifestIndexByApplicationId {
            application_id: "detta.defi".into(),
        },
        &defi_report.manifest_hash,
    );
    assert_application_manifest_index_contains(
        &mut client,
        RpcRequest::GetApplicationDaManifestIndexByApplicationId {
            application_id: "social.demo".into(),
        },
        &social_report.manifest_hash,
    );

    client.close();
    server.join().unwrap();
    std::fs::remove_dir_all(dir.path()).unwrap();
}

#[test]
fn client_rejects_invalid_social_application_payloads() {
    let dir = temp_dir("application-da-social-validation");
    let node = PersistentValidatorNode::bootstrap("validator-1", defi_genesis_state(), dir.path())
        .unwrap();
    let (addr, server) = spawn_tcp_persistent_node(node).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();
    let social_profile = DaApplicationProfile::social_demo_v1();
    register_application_profile(&mut client, &social_profile);

    let encrypted_private_payload = social_private_payload(&social_profile);
    let encrypted_report =
        produce_application_batch(&mut client, encrypted_private_payload.clone());
    assert_eq!(encrypted_report.application_id.0, "social.demo");

    let mut public_private_payload = encrypted_private_payload;
    public_private_payload.namespaces[1].records[0].encoding = DaRecordEncoding::CanonicalJson;
    assert_rpc_data_availability_error(
        &mut client,
        RpcRequest::ProduceApplicationDaBatch {
            payload: Box::new(public_private_payload),
            data_share_count: 4,
            parity_share_count: 2,
            certificate_signers: vec!["validator-1".into()],
        },
    );

    assert_rpc_data_availability_error(
        &mut client,
        RpcRequest::ProduceApplicationDaBatch {
            payload: Box::new(malformed_social_json_payload(&social_profile)),
            data_share_count: 4,
            parity_share_count: 2,
            certificate_signers: vec!["validator-1".into()],
        },
    );

    let forbidden_profile = forbidden_social_profile();
    register_application_profile(&mut client, &forbidden_profile);
    assert_rpc_data_availability_error(
        &mut client,
        RpcRequest::ProduceApplicationDaBatch {
            payload: Box::new(social_payload_with_forbidden_namespace(&forbidden_profile)),
            data_share_count: 4,
            parity_share_count: 2,
            certificate_signers: vec!["validator-1".into()],
        },
    );

    let mut wrong_profile_id_payload = social_demo_payload(&social_profile);
    wrong_profile_id_payload.profile_id = "22".repeat(32);
    let error = client
        .error(RpcRequest::ProduceApplicationDaBatch {
            payload: Box::new(wrong_profile_id_payload),
            data_share_count: 4,
            parity_share_count: 2,
            certificate_signers: vec!["validator-1".into()],
        })
        .unwrap();
    assert_eq!(error.code, "rpc.application_da_profile_not_found");

    client.close();
    server.join().unwrap();
    std::fs::remove_dir_all(dir.path()).unwrap();
}

#[test]
fn client_opaque_application_da_accepts_bytes_and_enforces_payload_commitments() {
    let dir = temp_dir("application-da-opaque-validation");
    let node = PersistentValidatorNode::bootstrap("validator-1", defi_genesis_state(), dir.path())
        .unwrap();
    let (addr, server) = spawn_tcp_persistent_node(node).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();
    let opaque_profile = opaque_demo_profile("opaque.demo", 16 * 1024 * 1024);
    register_application_profile(&mut client, &opaque_profile);

    let opaque_payload = opaque_demo_payload(&opaque_profile, b"\xffopaque-not-json".to_vec());
    let report = produce_application_batch(&mut client, opaque_payload.clone());
    assert_eq!(report.application_id.0, "opaque.demo");
    assert_eq!(
        application_payload(&mut client, &report.manifest_hash),
        opaque_payload.canonicalized()
    );

    let manifest = application_manifest(&mut client, &report.manifest_hash);
    let share = application_share(&mut client, &report.manifest_hash, 0);
    verify_application_share_against_manifest(&manifest, &share).unwrap();
    match client
        .ok(RpcRequest::GetApplicationDaSampleProofs {
            manifest_hash: report.manifest_hash.clone(),
            client_randomness: "opaque-client-randomness".into(),
            sample_count: 3,
            namespaces: vec!["social.feed".into()],
        })
        .unwrap()
    {
        RpcResult::ApplicationDaSampleProofs(bundle) => {
            assert!(bundle.verification.valid);
            assert_eq!(bundle.sample_proofs.len(), 3);
        }
        result => panic!("expected application DA sample proofs, got {result:?}"),
    }

    let mut stale_content_hash = opaque_payload.clone();
    stale_content_hash.namespaces[0].records[0].bytes.push(0x01);
    assert_rpc_data_availability_error(
        &mut client,
        RpcRequest::ProduceApplicationDaBatch {
            payload: Box::new(stale_content_hash),
            data_share_count: 4,
            parity_share_count: 2,
            certificate_signers: vec!["validator-1".into()],
        },
    );

    let tight_profile_source = opaque_demo_profile("opaque.tight", 16 * 1024 * 1024);
    let mut oversized_payload = opaque_demo_payload(&tight_profile_source, vec![0x41; 128]);
    let tight_profile = opaque_demo_profile("opaque.tight", 1);
    oversized_payload.profile_id = tight_profile.profile_id().unwrap();
    register_application_profile(&mut client, &tight_profile);
    assert_rpc_data_availability_error(
        &mut client,
        RpcRequest::ProduceApplicationDaBatch {
            payload: Box::new(oversized_payload),
            data_share_count: 4,
            parity_share_count: 2,
            certificate_signers: vec!["validator-1".into()],
        },
    );

    let mut unknown_namespace_payload = opaque_demo_payload(&opaque_profile, b"opaque".to_vec());
    unknown_namespace_payload.namespaces = vec![
        application_section(
            "social.admin",
            vec![application_record(
                "social.post",
                DaRecordEncoding::OpaqueBytes,
                b"admin",
                Some("alice"),
            )],
        ),
        unknown_namespace_payload.namespaces[0].clone(),
    ];
    assert_rpc_data_availability_error(
        &mut client,
        RpcRequest::ProduceApplicationDaBatch {
            payload: Box::new(unknown_namespace_payload),
            data_share_count: 4,
            parity_share_count: 2,
            certificate_signers: vec!["validator-1".into()],
        },
    );

    client.close();
    server.join().unwrap();
    std::fs::remove_dir_all(dir.path()).unwrap();
}

#[test]
fn client_application_da_indexes_survive_restart_and_backup_restore() {
    let dir = temp_dir("application-da-index-source");
    let backup_dir = temp_dir("application-da-index-backup");
    let restore_dir = temp_dir("application-da-index-restore");
    let node = PersistentValidatorNode::bootstrap("validator-1", defi_genesis_state(), dir.path())
        .unwrap();
    let (addr, server) = spawn_tcp_persistent_node(node).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();
    let profile = DaApplicationProfile::social_demo_v1();
    let profile_id = register_application_profile(&mut client, &profile);
    let payload = social_demo_payload(&profile);
    let coordinate = payload.coordinate.clone();
    let report = produce_application_batch(&mut client, payload);
    let manifest = application_manifest(&mut client, &report.manifest_hash);
    let application_root = manifest.application_root.clone().unwrap();
    assert_social_application_indexes(
        &mut client,
        &profile_id,
        &coordinate,
        &report.manifest_hash,
        &report.certificate_hash,
        &application_root,
    );
    client.close();
    server.join().unwrap();

    let restarted = PersistentValidatorNode::restart("validator-1", dir.path()).unwrap();
    let (restart_addr, restart_server) = spawn_tcp_persistent_node(restarted).unwrap();
    let mut restart_client = TcpRpcClient::connect(restart_addr).unwrap();
    assert_social_application_indexes(
        &mut restart_client,
        &profile_id,
        &coordinate,
        &report.manifest_hash,
        &report.certificate_hash,
        &application_root,
    );
    restart_client.close();
    restart_server.join().unwrap();

    let storage = FileStorage::open(dir.path()).unwrap();
    let backup_manifest = storage.backup_to(backup_dir.path()).unwrap();
    assert!(backup_manifest.file_count > 0);
    drop(storage);
    FileStorage::restore_from_backup(backup_dir.path(), restore_dir.path()).unwrap();

    let restored = PersistentValidatorNode::restart("validator-1", restore_dir.path()).unwrap();
    let (restore_addr, restore_server) = spawn_tcp_persistent_node(restored).unwrap();
    let mut restore_client = TcpRpcClient::connect(restore_addr).unwrap();
    assert_social_application_indexes(
        &mut restore_client,
        &profile_id,
        &coordinate,
        &report.manifest_hash,
        &report.certificate_hash,
        &application_root,
    );
    assert_eq!(
        application_payload(&mut restore_client, &report.manifest_hash).coordinate,
        coordinate
    );
    restore_client.close();
    restore_server.join().unwrap();
}

fn social_demo_payload(profile: &DaApplicationProfile) -> ApplicationDaPayload {
    social_feed_payload(
        profile,
        "main",
        1,
        br#"{"author":"alice","post_id":"post-1","text":"e2e"}"#,
    )
}

fn social_private_payload(profile: &DaApplicationProfile) -> ApplicationDaPayload {
    ApplicationDaPayload::new(
        profile,
        social_coordinate(profile, "private", 1),
        DaPayloadKind::Batch,
        None,
        vec![DaApplicationRoot::new("social.event.log.root", "11".repeat(32)).unwrap()],
        vec![
            application_section(
                "social.feed",
                vec![application_record(
                    "social.post",
                    DaRecordEncoding::CanonicalJson,
                    br#"{"author":"alice","post_id":"post-1","text":"private"}"#,
                    Some("alice"),
                )],
            ),
            application_section(
                "social.private",
                vec![application_record(
                    "social.private.message",
                    DaRecordEncoding::EncryptedBytes,
                    b"encrypted-private-message",
                    Some("alice"),
                )],
            ),
        ],
    )
    .unwrap()
}

fn malformed_social_json_payload(profile: &DaApplicationProfile) -> ApplicationDaPayload {
    social_feed_payload(profile, "malformed", 2, b"{not-json")
}

fn social_payload_with_forbidden_namespace(profile: &DaApplicationProfile) -> ApplicationDaPayload {
    let mut payload = social_feed_payload(
        profile,
        "blocked",
        1,
        br#"{"author":"alice","post_id":"post-1","text":"blocked"}"#,
    );
    payload.namespaces = vec![
        application_section(
            "social.blocked",
            vec![application_record(
                "social.post",
                DaRecordEncoding::CanonicalJson,
                br#"{"author":"alice","post_id":"post-2","text":"blocked"}"#,
                Some("alice"),
            )],
        ),
        payload.namespaces[0].clone(),
    ];
    payload
}

fn social_feed_payload(
    profile: &DaApplicationProfile,
    stream_id: &str,
    sequence: u64,
    record_bytes: &[u8],
) -> ApplicationDaPayload {
    ApplicationDaPayload::new(
        profile,
        social_coordinate(profile, stream_id, sequence),
        DaPayloadKind::Batch,
        None,
        vec![DaApplicationRoot::new("social.event.log.root", "11".repeat(32)).unwrap()],
        vec![application_section(
            "social.feed",
            vec![application_record(
                "social.post",
                DaRecordEncoding::CanonicalJson,
                record_bytes,
                Some("alice"),
            )],
        )],
    )
    .unwrap()
}

fn social_coordinate(
    profile: &DaApplicationProfile,
    stream_id: &str,
    sequence: u64,
) -> DaApplicationCoordinate {
    DaApplicationCoordinate {
        application_id: profile.application_id.clone(),
        stream_id: stream_id.into(),
        sequence,
        epoch: Some(1),
        parent_hash: None,
        subject_hash: None,
    }
}

fn forbidden_social_profile() -> DaApplicationProfile {
    let mut profile = DaApplicationProfile::social_demo_v1();
    profile.application_id = DaApplicationId::new("social.forbidden").unwrap();
    profile.profile_name = "Social Forbidden E2E DA v1".into();
    profile.namespace_policies.push(DaNamespacePolicy {
        namespace: DaNamespace::new("social.blocked").unwrap(),
        requirement: DaNamespaceRequirement::Forbidden,
        allowed_record_schemas: Vec::new(),
        min_records: 0,
        max_records: 0,
        retention_class: DaApplicationRetentionClass::Cold,
    });
    let profile = profile.canonicalized();
    profile.validate().unwrap();
    profile
}

fn opaque_demo_profile(application_id: &str, max_payload_bytes: u64) -> DaApplicationProfile {
    let mut profile = DaApplicationProfile::social_demo_v1();
    profile.application_id = DaApplicationId::new(application_id).unwrap();
    profile.profile_name = format!("{application_id} Opaque DA v1");
    profile.validation_mode = DaApplicationValidationMode::OpaqueBytes;
    profile.privacy_mode = DaApplicationPrivacyMode::CommitmentOnly;
    profile.max_payload_bytes = max_payload_bytes;
    for policy in &mut profile.record_policies {
        if policy.schema == "social.post" {
            policy.allowed_encodings = vec![DaRecordEncoding::OpaqueBytes];
        }
    }
    let profile = profile.canonicalized();
    profile.validate().unwrap();
    profile
}

fn opaque_demo_payload(profile: &DaApplicationProfile, bytes: Vec<u8>) -> ApplicationDaPayload {
    ApplicationDaPayload::new(
        profile,
        social_coordinate(profile, "opaque", 1),
        DaPayloadKind::Batch,
        None,
        vec![DaApplicationRoot::new("social.event.log.root", "11".repeat(32)).unwrap()],
        vec![application_section(
            "social.feed",
            vec![application_record(
                "social.post",
                DaRecordEncoding::OpaqueBytes,
                &bytes,
                Some("alice"),
            )],
        )],
    )
    .unwrap()
}

fn detta_defi_application_payload(
    profile: &DaApplicationProfile,
    block: &Block,
) -> ApplicationDaPayload {
    let coordinate = DaApplicationCoordinate {
        application_id: DaApplicationId::new("detta.defi").unwrap(),
        stream_id: block.header.chain_id.clone(),
        sequence: block.header.height,
        epoch: None,
        parent_hash: None,
        subject_hash: None,
    };
    let block_header_bytes = serde_json::to_vec(&block.header).unwrap();
    let mut sections = vec![application_section(
        "detta.block",
        vec![application_record(
            "detta.block.header",
            DaRecordEncoding::CanonicalJson,
            &block_header_bytes,
            None,
        )],
    )];
    if !block.transactions.is_empty() {
        sections.push(application_section(
            "detta.tx",
            block
                .transactions
                .iter()
                .map(|transaction| {
                    let bytes = serde_json::to_vec(transaction).unwrap();
                    application_record(
                        "detta.tx.signed",
                        DaRecordEncoding::CanonicalJson,
                        &bytes,
                        Some(&transaction.sender),
                    )
                })
                .collect(),
        ));
    }
    if !block.receipts.is_empty() {
        sections.push(application_section(
            "detta.receipt",
            block
                .receipts
                .iter()
                .map(|receipt| {
                    let bytes = serde_json::to_vec(receipt).unwrap();
                    application_record(
                        "detta.receipt",
                        DaRecordEncoding::CanonicalJson,
                        &bytes,
                        None,
                    )
                })
                .collect(),
        ));
    }
    ApplicationDaPayload::new(
        profile,
        coordinate,
        DaPayloadKind::Block,
        None,
        vec![
            DaApplicationRoot::new("detta.block.hash", block.block_hash()).unwrap(),
            DaApplicationRoot::new(
                "detta.global.state.root",
                block.header.global_state_root.clone(),
            )
            .unwrap(),
            DaApplicationRoot::new("detta.receipt.root", block.header.receipt_root.clone())
                .unwrap(),
            DaApplicationRoot::new("detta.tx.root", block.header.tx_root.clone()).unwrap(),
        ],
        sections,
    )
    .unwrap()
}

fn application_record(
    schema: &str,
    encoding: DaRecordEncoding,
    bytes: &[u8],
    signer: Option<&str>,
) -> DaRecordEnvelope {
    let content_type = match &encoding {
        DaRecordEncoding::OpaqueBytes | DaRecordEncoding::EncryptedBytes => {
            "application/octet-stream"
        }
        DaRecordEncoding::CanonicalJson | DaRecordEncoding::ExternalContentAddress => {
            "application/json"
        }
    };
    DaRecordEnvelope::new(
        schema,
        1,
        content_type,
        encoding,
        bytes.to_vec(),
        signer.map(String::from),
        signer.map(|value| format!("sig-{value}")),
    )
    .unwrap()
}

fn application_section(
    namespace: &str,
    records: Vec<DaRecordEnvelope>,
) -> ApplicationDaNamespaceSection {
    ApplicationDaNamespaceSection::new(DaNamespace::new(namespace).unwrap(), records).unwrap()
}

fn submit_ok(client: &mut TcpRpcClient, transaction: detta_core::Transaction) {
    assert!(matches!(
        client
            .ok(RpcRequest::SubmitTransaction { transaction })
            .unwrap(),
        RpcResult::Submitted
    ));
}

fn produce_da_block(
    client: &mut TcpRpcClient,
    height: u64,
    timestamp: u64,
    share_size_bytes: u32,
) -> Block {
    match client
        .ok(RpcRequest::ProduceDaBlock {
            height,
            timestamp,
            share_size_bytes,
        })
        .unwrap()
    {
        RpcResult::Block(block) => *block,
        result => panic!("expected produced DA block, got {result:?}"),
    }
}

fn da_manifest(client: &mut TcpRpcClient, manifest_hash: &str) -> detta_da::DaManifest {
    match client
        .ok(RpcRequest::GetDaManifest {
            manifest_hash: manifest_hash.into(),
        })
        .unwrap()
    {
        RpcResult::DaManifest(manifest) => *manifest,
        result => panic!("expected DA manifest, got {result:?}"),
    }
}

fn da_payload(client: &mut TcpRpcClient, manifest_hash: &str) -> detta_da::DaPayload {
    match client
        .ok(RpcRequest::GetDaPayload {
            manifest_hash: manifest_hash.into(),
        })
        .unwrap()
    {
        RpcResult::DaPayload(payload) => *payload,
        result => panic!("expected DA payload, got {result:?}"),
    }
}

fn register_application_profile(
    client: &mut TcpRpcClient,
    profile: &DaApplicationProfile,
) -> String {
    match client
        .ok(RpcRequest::RegisterApplicationDaProfile {
            profile: Box::new(profile.clone()),
        })
        .unwrap()
    {
        RpcResult::ApplicationDaProfile(registration) => registration.profile_id,
        result => panic!("expected application DA profile registration, got {result:?}"),
    }
}

fn produce_application_batch(
    client: &mut TcpRpcClient,
    payload: ApplicationDaPayload,
) -> ApplicationDaProductionReport {
    match client
        .ok(RpcRequest::ProduceApplicationDaBatch {
            payload: Box::new(payload),
            data_share_count: 4,
            parity_share_count: 2,
            certificate_signers: vec!["validator-1".into(), "validator-2".into()],
        })
        .unwrap()
    {
        RpcResult::ApplicationDaProduction(report) => *report,
        result => panic!("expected application DA production report, got {result:?}"),
    }
}

fn application_manifest(
    client: &mut TcpRpcClient,
    manifest_hash: &str,
) -> detta_da::ApplicationDaManifest {
    match client
        .ok(RpcRequest::GetApplicationDaManifest {
            manifest_hash: manifest_hash.into(),
        })
        .unwrap()
    {
        RpcResult::ApplicationDaManifest(manifest) => *manifest,
        result => panic!("expected application DA manifest, got {result:?}"),
    }
}

fn application_share(
    client: &mut TcpRpcClient,
    manifest_hash: &str,
    index: u32,
) -> detta_da::DaShare {
    match client
        .ok(RpcRequest::GetApplicationDaShare {
            manifest_hash: manifest_hash.into(),
            index,
        })
        .unwrap()
    {
        RpcResult::ApplicationDaShare(share) => *share,
        result => panic!("expected application DA share, got {result:?}"),
    }
}

fn application_payload(client: &mut TcpRpcClient, manifest_hash: &str) -> ApplicationDaPayload {
    match client
        .ok(RpcRequest::GetApplicationDaPayload {
            manifest_hash: manifest_hash.into(),
        })
        .unwrap()
    {
        RpcResult::ApplicationDaPayload(payload) => *payload,
        result => panic!("expected application DA payload, got {result:?}"),
    }
}

fn assert_rpc_data_availability_error(client: &mut TcpRpcClient, request: RpcRequest) {
    let error = client.error(request).unwrap();
    assert_eq!(error.code, "node.data_availability_error");
}

fn assert_social_application_indexes(
    client: &mut TcpRpcClient,
    profile_id: &str,
    coordinate: &DaApplicationCoordinate,
    manifest_hash: &str,
    certificate_hash: &str,
    application_root: &str,
) {
    assert_application_manifest_index_contains(
        client,
        RpcRequest::GetApplicationDaManifestIndexByApplicationId {
            application_id: "social.demo".into(),
        },
        manifest_hash,
    );
    assert_application_manifest_index_contains(
        client,
        RpcRequest::GetApplicationDaManifestIndexByProfileId {
            profile_id: profile_id.into(),
        },
        manifest_hash,
    );
    assert_application_manifest_index_contains(
        client,
        RpcRequest::GetApplicationDaManifestIndexByCoordinate {
            coordinate: coordinate.clone(),
        },
        manifest_hash,
    );
    assert_application_manifest_index_contains(
        client,
        RpcRequest::GetApplicationDaManifestIndexByNamespace {
            namespace: "social.feed".into(),
        },
        manifest_hash,
    );
    assert_application_manifest_index_contains(
        client,
        RpcRequest::GetApplicationDaManifestIndexByRetentionClass {
            class: DaApplicationRetentionClass::Warm,
        },
        manifest_hash,
    );
    assert_application_manifest_index_contains(
        client,
        RpcRequest::GetApplicationDaManifestIndexByApplicationRoot {
            application_root: application_root.into(),
        },
        manifest_hash,
    );
    assert_application_certificate_index_contains(
        client,
        RpcRequest::GetApplicationDaCertificateIndexByManifest {
            manifest_hash: manifest_hash.into(),
        },
        certificate_hash,
    );
    assert_application_certificate_index_contains(
        client,
        RpcRequest::GetApplicationDaCertificateIndexByApplicationId {
            application_id: "social.demo".into(),
        },
        certificate_hash,
    );
    assert_application_certificate_index_contains(
        client,
        RpcRequest::GetApplicationDaCertificateIndexByProfileId {
            profile_id: profile_id.into(),
        },
        certificate_hash,
    );
    assert_application_certificate_index_contains(
        client,
        RpcRequest::GetApplicationDaCertificateIndexByCoordinate {
            coordinate: coordinate.clone(),
        },
        certificate_hash,
    );
}

fn assert_application_manifest_index_contains(
    client: &mut TcpRpcClient,
    request: RpcRequest,
    manifest_hash: &str,
) {
    match client.ok(request).unwrap() {
        RpcResult::ApplicationDaManifestIndex(index) => assert!(
            index
                .iter()
                .any(|entry| entry.manifest_hash == manifest_hash),
            "application DA manifest index did not include {manifest_hash}: {index:?}"
        ),
        result => panic!("expected application DA manifest index, got {result:?}"),
    }
}

fn assert_application_certificate_index_contains(
    client: &mut TcpRpcClient,
    request: RpcRequest,
    certificate_hash: &str,
) {
    match client.ok(request).unwrap() {
        RpcResult::ApplicationDaCertificateIndex(index) => assert!(
            index
                .iter()
                .any(|entry| entry.certificate_hash == certificate_hash),
            "application DA certificate index did not include {certificate_hash}: {index:?}"
        ),
        result => panic!("expected application DA certificate index, got {result:?}"),
    }
}
