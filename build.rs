//! Compila el contrato gRPC con protox (compilador de protobuf en Rust): no hace falta `protoc` en la máquina.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/smartpot/v1/ai.proto");
    let descriptors = protox::compile(["smartpot/v1/ai.proto"], ["proto"])?;
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_fds(descriptors)?;
    Ok(())
}
