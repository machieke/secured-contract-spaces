use detta_verify::{
    validate_aspect_proof_obligations, validate_aspect_stdlib_artifact,
    AspectProofObligationManifest, AspectStdlibArtifactManifest, ASPECT_STDLIB_ARTIFACT_PATH,
    ASPECT_STDLIB_ARTIFACT_SCHEMA, ASPECT_STDLIB_ARTIFACT_SCHEMA_VERSION,
    ASPECT_STDLIB_PROOF_OBLIGATIONS_PATH, ASPECT_STDLIB_PROOF_OBLIGATIONS_SCHEMA,
    ASPECT_STDLIB_PROOF_OBLIGATIONS_SCHEMA_VERSION, ASPECT_STDLIB_SOURCE_PATH,
};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let repo_root = repo_root()?;
    let source_path = repo_root.join(ASPECT_STDLIB_SOURCE_PATH);
    let artifact_path = repo_root.join(ASPECT_STDLIB_ARTIFACT_PATH);
    let obligations_path = repo_root.join(ASPECT_STDLIB_PROOF_OBLIGATIONS_PATH);

    let source = fs::read_to_string(&source_path)?;
    let existing_artifact: AspectStdlibArtifactManifest =
        serde_json::from_str(&fs::read_to_string(&artifact_path)?)?;
    let existing_obligations: AspectProofObligationManifest =
        serde_json::from_str(&fs::read_to_string(&obligations_path)?)?;

    let (canonical_source, ir, verified) = detta_aspects::parse_verify_module(&source)
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let module =
        detta_aspects::module_artifact(&existing_artifact.module_id, canonical_source, &verified);
    let artifact = AspectStdlibArtifactManifest {
        schema: ASPECT_STDLIB_ARTIFACT_SCHEMA.into(),
        schema_version: ASPECT_STDLIB_ARTIFACT_SCHEMA_VERSION,
        module_id: existing_artifact.module_id,
        source_path: ASPECT_STDLIB_SOURCE_PATH.into(),
        taxonomy_version: module.taxonomy_version,
        accepted_language: module.accepted_language,
        verifier_version: module.verifier_version,
        source_root: module.source_root,
        ir_root: module.ir_root,
        abi_root: module.abi_root,
        policy_root: module.policy_root,
        storage_schema_root: module.storage_schema_root,
        registry_schema_root: module.registry_schema_root,
        invariant_root: module.invariant_root,
        bundle_count: ir.bundles.len(),
        aspect_count: ir.aspects.len(),
        projection_count: ir.projections.len(),
        abi_count: ir.abi.len(),
        policy_count: ir.policies.len(),
        storage_schema_count: ir.storage_schema.len(),
        registry_schema_count: ir.registry_schema.len(),
        invariant_count: ir.invariants.len(),
    };

    let obligations = AspectProofObligationManifest {
        schema: ASPECT_STDLIB_PROOF_OBLIGATIONS_SCHEMA.into(),
        schema_version: ASPECT_STDLIB_PROOF_OBLIGATIONS_SCHEMA_VERSION,
        module_id: artifact.module_id.clone(),
        source_path: ASPECT_STDLIB_SOURCE_PATH.into(),
        artifact_path: ASPECT_STDLIB_ARTIFACT_PATH.into(),
        source_root: artifact.source_root.clone(),
        ir_root: artifact.ir_root.clone(),
        obligations: existing_obligations.obligations,
    };

    let artifact_errors = validate_aspect_stdlib_artifact(&source, &artifact);
    if !artifact_errors.is_empty() {
        return Err(io::Error::other(format!("{artifact_errors:?}")).into());
    }
    let obligation_errors = validate_aspect_proof_obligations(&obligations, &artifact);
    if !obligation_errors.is_empty() {
        return Err(io::Error::other(format!("{obligation_errors:?}")).into());
    }

    write_pretty_json(&artifact_path, &artifact)?;
    write_pretty_json(&obligations_path, &obligations)?;
    write_sha256sum(&attestation_path(&source_path)?, &source_path)?;
    write_sha256sum(&attestation_path(&artifact_path)?, &artifact_path)?;
    write_sha256sum(&attestation_path(&obligations_path)?, &obligations_path)?;
    Ok(())
}

fn repo_root() -> io::Result<PathBuf> {
    let mut directory = std::env::current_dir()?;
    loop {
        if directory.join("Cargo.toml").is_file() && directory.join("models").is_dir() {
            return Ok(directory);
        }
        if !directory.pop() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "could not locate repository root",
            ));
        }
    }
}

fn write_pretty_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_string_pretty(value)?;
    fs::write(path, format!("{json}\n"))?;
    Ok(())
}

fn write_sha256sum(attestation_path: &Path, target_path: &Path) -> io::Result<()> {
    let bytes = fs::read(target_path)?;
    let file_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "non-UTF-8 filename"))?;
    fs::write(
        attestation_path,
        format!("{}  {file_name}\n", sha256_hex(&bytes)),
    )
}

fn attestation_path(target_path: &Path) -> io::Result<PathBuf> {
    if target_path
        .extension()
        .and_then(|extension| extension.to_str())
        == Some("json")
    {
        return Ok(target_path.with_extension("sha256"));
    }

    let target = target_path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "non-UTF-8 path"))?;
    Ok(PathBuf::from(format!("{target}.sha256")))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
