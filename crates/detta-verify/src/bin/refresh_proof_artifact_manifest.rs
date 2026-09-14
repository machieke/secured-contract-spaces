use detta_verify::proof_artifact_manifest;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const PROOF_ARTIFACT_MANIFEST_PATH: &str = "models/detta-proof-artifact-manifest.json";
const PROOF_ARTIFACT_MANIFEST_SHA256_PATH: &str = "models/detta-proof-artifact-manifest.sha256";

fn main() -> Result<(), Box<dyn Error>> {
    let repo_root = repo_root()?;
    let manifest_path = repo_root.join(PROOF_ARTIFACT_MANIFEST_PATH);
    let attestation_path = repo_root.join(PROOF_ARTIFACT_MANIFEST_SHA256_PATH);
    let manifest = proof_artifact_manifest();

    write_pretty_json(&manifest_path, &manifest)?;
    write_sha256sum(&attestation_path, &manifest_path)?;
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

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}
