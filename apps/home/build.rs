fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .with_include_paths(vec![std::path::PathBuf::from("../../crates/ui-kit/ui")]);
    slint_build::compile_with_config("ui/main.slint", config).unwrap();
}
