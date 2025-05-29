fn main() {
    // Tells cargo to rerun this script if the proto file changes
    println!("cargo:rerun-if-changed=src/message.proto");

    prost_build::Config::new()
        .out_dir("src/")  // Output generated files to src/
        .compile_protos(&["src/message.proto"], &["src"])
        .unwrap();
}
