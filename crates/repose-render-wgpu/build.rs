const ROOT_MODULES: &[&str] = &["rect", "border", "ellipse", "ellipse_border", "arc"];

fn main() {
    for module in ROOT_MODULES {
        wesl::Wesl::new("src/shaders")
            .build_artifact(&format!("package::{module}").parse().unwrap(), module);
    }
}
