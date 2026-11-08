//! Compiles every `.proto` file under the repository's `proto/` directory into
//! Rust (prost messages plus tonic clients and servers).
//!
//! `protox` parses the files in pure Rust, so building this crate needs no
//! system `protoc`. Well-known types (`google/protobuf/timestamp.proto`) are
//! built into protox.

use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proto");
    let proto_root = proto_root.canonicalize()?;

    let mut files = Vec::new();
    collect_protos(&proto_root, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(format!("no .proto files under {}", proto_root.display()).into());
    }

    // Rebuild when any proto file is added, removed or edited.
    println!("cargo:rerun-if-changed={}", proto_root.display());
    for file in &files {
        println!("cargo:rerun-if-changed={}", file.display());
    }

    let fds = protox::compile(&files, [&proto_root])?;

    tonic_prost_build::configure()
        // Transport-agnostic clients: callers build their own `tonic` channel
        // (pinned TLS through the relay), so no `connect()` helper is generated.
        .build_transport(false)
        .emit_rerun_if_changed(false)
        .compile_fds(fds)?;
    Ok(())
}

fn collect_protos(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_protos(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "proto") {
            out.push(path);
        }
    }
    Ok(())
}
