use detta_da::{
    ApplicationDaNamespaceSection, ApplicationDaPayload, DaApplicationCoordinate, DaApplicationId,
    DaApplicationProfile, DaApplicationRoot, DaNamespace, DaPayloadKind, DaRecordEncoding,
    DaRecordEnvelope,
};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{defi_genesis_state, temp_dir};
use detta_e2e::network::spawn_tcp_persistent_node;
use detta_node::PersistentValidatorNode;
use detta_rpc::{RpcRequest, RpcResult};

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

fn social_demo_payload(profile: &DaApplicationProfile) -> ApplicationDaPayload {
    let coordinate = DaApplicationCoordinate {
        application_id: DaApplicationId::new("social.demo").unwrap(),
        stream_id: "main".into(),
        sequence: 1,
        epoch: Some(1),
        parent_hash: None,
        subject_hash: None,
    };
    let post = DaRecordEnvelope::new(
        "social.post",
        1,
        "application/json",
        DaRecordEncoding::CanonicalJson,
        br#"{"author":"alice","post_id":"post-1","text":"e2e"}"#.to_vec(),
        Some("alice".into()),
        Some("signature-1".into()),
    )
    .unwrap();
    ApplicationDaPayload::new(
        profile,
        coordinate,
        DaPayloadKind::Batch,
        None,
        vec![DaApplicationRoot::new("social.event.log.root", "11".repeat(32)).unwrap()],
        vec![ApplicationDaNamespaceSection::new(
            DaNamespace::new("social.feed").unwrap(),
            vec![post],
        )
        .unwrap()],
    )
    .unwrap()
}
