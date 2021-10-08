fn main() {
    protobuf_codegen_pure::Codegen::new()
        .out_dir("src/proto")
        .inputs(&["proto/instantiate_reply.proto"])
        .include("proto")
        .run()
        .expect("Protobuf codegen failed.");
}
