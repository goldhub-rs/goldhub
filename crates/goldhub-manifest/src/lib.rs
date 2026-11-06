//! Declarative package and configuration manifest schemas for GoldHub.

pub mod error;
pub mod kdl_graph;
pub mod package;

pub use error::ManifestError;
pub use kdl_graph::InputBindingsGraph;
pub use package::{
    CURRENT_SCHEMA_VERSION, EnvironmentSpec, GamePackageManifest, NetworkSpec, PackageMeta,
    RenderSpec,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_valid_game_package_manifest_parsing() {
        let toml_str = r#"
[package]
id = "valve.cstrike"
name = "Counter-Strike"
version = "1.6.0"
schema = 1
description = "Tactical Counter-Terrorist simulation"
authors = ["Valve", "Minh Le", "Jess Cliffe"]

[environment]
base = "valve.hl"
mount_point = "cstrike"
priority = 200

[render]
fov = 90.0
gamma = 1.8
clear_color = [0.0, 0.0, 0.0, 1.0]

[network]
default_port = 27015
protocols = ["48", "goldhub-v1"]
"#;

        let manifest = GamePackageManifest::parse_str(toml_str).expect("parse valid toml");
        assert_eq!(manifest.package.id, "valve.cstrike");
        assert_eq!(manifest.package.name, "Counter-Strike");
        assert_eq!(manifest.environment.base.as_deref(), Some("valve.hl"));
        assert_eq!(manifest.environment.mount_point, PathBuf::from("cstrike"));
        assert_eq!(manifest.environment.priority, 200);
        assert_eq!(manifest.render.fov, 90.0);
        assert_eq!(manifest.network.default_port, 27015);
    }

    #[test]
    fn test_rejects_path_traversal() {
        let toml_str = r#"
[package]
id = "evil.mod"
name = "Malicious Mod"
version = "1.0.0"
schema = 1

[environment]
mount_point = "../../../windows/system32"
"#;

        let err = GamePackageManifest::parse_str(toml_str).unwrap_err();
        assert!(matches!(err, ManifestError::PathTraversal { .. }));
    }

    #[test]
    fn test_rejects_self_dependency_cycle() {
        let toml_str = r#"
[package]
id = "valve.cstrike"
name = "Counter-Strike"
version = "1.6.0"
schema = 1

[environment]
base = "valve.cstrike"
mount_point = "cstrike"
"#;

        let err = GamePackageManifest::parse_str(toml_str).unwrap_err();
        assert_eq!(
            err,
            ManifestError::CyclicDependency {
                id: "valve.cstrike".to_string()
            }
        );
    }

    #[test]
    fn test_rejects_unknown_fields() {
        let toml_str = r#"
[package]
id = "valve.cstrike"
name = "Counter-Strike"
version = "1.6.0"
schema = 1
unknown_junk_field = "hacked"

[environment]
mount_point = "cstrike"
"#;

        let err = GamePackageManifest::parse_str(toml_str).unwrap_err();
        assert!(matches!(err, ManifestError::TomlDeserialization(_)));
    }

    #[test]
    fn test_kdl_input_bindings_parsing() {
        let kdl_str = r#"
context "in_game" {
    action "move_forward" {
        trigger key="W"
        trigger key="Up"
    }
    action "fire" {
        trigger mouse="Left"
    }
}
"#;

        let graph = InputBindingsGraph::parse_str(kdl_str).expect("parse kdl");
        assert_eq!(graph.contexts.len(), 1);
        assert_eq!(graph.contexts[0].name, "in_game");
        assert_eq!(graph.contexts[0].actions.len(), 2);
        assert_eq!(graph.contexts[0].actions[0].action_name, "move_forward");
        assert_eq!(
            graph.contexts[0].actions[0].triggers,
            vec!["key:W", "key:Up"]
        );
    }
}
