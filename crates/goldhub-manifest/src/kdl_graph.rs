//! KDL document parsers for complex node-graph configurations:
//! input bindings (`bindings.kdl`), render passes (`render.kdl`), and VFS overlays (`vfs.kdl`).

use kdl::{KdlDocument, KdlNode};

use crate::error::ManifestError;

/// Node-graph input bindings specification parsed from `bindings.kdl`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputBindingsGraph {
    pub contexts: Vec<InputContextNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputContextNode {
    pub name: String,
    pub actions: Vec<InputActionNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputActionNode {
    pub action_name: String,
    pub triggers: Vec<String>,
}

impl InputBindingsGraph {
    /// Parses a KDL string into an input bindings node-graph.
    pub fn parse_str(content: &str) -> Result<Self, ManifestError> {
        let doc: KdlDocument = content
            .parse()
            .map_err(|e: kdl::KdlError| ManifestError::KdlParse(e.to_string()))?;

        let mut contexts = Vec::new();

        for node in doc.nodes() {
            if node.name().value() == "context" {
                contexts.push(parse_context_node(node)?);
            }
        }

        Ok(Self { contexts })
    }
}

fn parse_context_node(node: &KdlNode) -> Result<InputContextNode, ManifestError> {
    let name_str = node
        .get(0)
        .and_then(|entry| entry.value().as_string())
        .ok_or_else(|| ManifestError::KdlParse("context node missing name argument".to_string()))?;

    let name = name_str.trim().to_string();
    if name.is_empty() {
        return Err(ManifestError::KdlParse(
            "context name cannot be empty or purely whitespace".to_string(),
        ));
    }

    let mut actions = Vec::new();

    if let Some(children) = node.children() {
        for child in children.nodes() {
            let child_name = child.name().value();
            match child_name {
                "action" => actions.push(parse_action_node(child)?),
                other => {
                    return Err(ManifestError::KdlParse(format!(
                        "unexpected node '{other}' inside context '{name}', expected 'action'"
                    )));
                }
            }
        }
    }

    Ok(InputContextNode { name, actions })
}

fn parse_action_node(node: &KdlNode) -> Result<InputActionNode, ManifestError> {
    let raw_name = node
        .get(0)
        .and_then(|entry| entry.value().as_string())
        .ok_or_else(|| ManifestError::KdlParse("action node missing name argument".to_string()))?;

    let action_name = raw_name.trim().to_string();
    if action_name.is_empty() {
        return Err(ManifestError::KdlParse(
            "action name cannot be empty or purely whitespace".to_string(),
        ));
    }

    let mut triggers = Vec::new();

    if let Some(children) = node.children() {
        for child in children.nodes() {
            let child_name = child.name().value();
            if child_name != "trigger" {
                return Err(ManifestError::KdlParse(format!(
                    "unexpected node '{child_name}' inside action '{action_name}', expected 'trigger'"
                )));
            }

            let key_opt = child.get("key").and_then(|e| e.value().as_string());
            let mouse_opt = child.get("mouse").and_then(|e| e.value().as_string());

            match (key_opt, mouse_opt) {
                (Some(k), None) => {
                    let kt = k.trim();
                    if kt.is_empty() {
                        return Err(ManifestError::KdlParse(
                            "key trigger cannot be empty".to_string(),
                        ));
                    }
                    triggers.push(format!("key:{kt}"));
                }
                (None, Some(m)) => {
                    let mt = m.trim();
                    if mt.is_empty() {
                        return Err(ManifestError::KdlParse(
                            "mouse trigger cannot be empty".to_string(),
                        ));
                    }
                    triggers.push(format!("mouse:{mt}"));
                }
                (Some(_), Some(_)) => {
                    return Err(ManifestError::KdlParse(
                        "trigger cannot define both 'key' and 'mouse' simultaneously".to_string(),
                    ));
                }
                (None, None) => {
                    return Err(ManifestError::KdlParse(
                        "trigger node must define either 'key' or 'mouse'".to_string(),
                    ));
                }
            }
        }
    }

    Ok(InputActionNode {
        action_name,
        triggers,
    })
}
