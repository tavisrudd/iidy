//! YAML tag validation logic
//!
//! This module handles semantic validation of YAML tags without building the full AST.
//! It provides comprehensive error collection for preprocessing and CloudFormation tags.

use std::collections::HashSet;
use tree_sitter::Node;
use url::Url;

use super::error::{ParseDiagnostics, ParseError, ParseWarning, error_codes};
use super::parser::node_meta;

/// Tag validation configuration
struct TagValidationConfig {
    allowed_node_types: &'static [&'static str],
    required_fields: &'static [&'static str],
    optional_fields: &'static [&'static str],
    error_message: &'static str,
}

/// Get validation configuration for a tag
fn get_tag_validation_config(tag_name: &str) -> TagValidationConfig {
    match tag_name {
        "!$include" => TagValidationConfig {
            allowed_node_types: &["plain_scalar", "single_quote_scalar", "double_quote_scalar", "flow_mapping", "block_mapping"],
            required_fields: &["path"],
            optional_fields: &["query"],
            error_message: "!$include expects string path or mapping with path field",
        },
        "!$let" => TagValidationConfig {
            allowed_node_types: &["flow_mapping", "block_mapping"],
            required_fields: &["in"],
            optional_fields: &[],
            error_message: "!$let expects mapping with variable bindings and 'in' field",
        },
        "!$map" => TagValidationConfig {
            allowed_node_types: &["flow_mapping", "block_mapping"],
            required_fields: &["items", "template", "var"],
            optional_fields: &["filter"],
            error_message: "!$map expects mapping with items, template, and var fields",
        },
        "!$if" => TagValidationConfig {
            allowed_node_types: &["flow_mapping", "block_mapping"],
            required_fields: &["test", "then"],
            optional_fields: &["else"],
            error_message: "!$if expects mapping with test, then, and optional else fields",
        },
        "!$eq" => TagValidationConfig {
            allowed_node_types: &["flow_mapping", "block_mapping"],
            required_fields: &["left", "right"],
            optional_fields: &[],
            error_message: "!$eq expects mapping with left and right fields",
        },
        "!$split" => TagValidationConfig {
            allowed_node_types: &["flow_mapping", "block_mapping"],
            required_fields: &["delimiter", "string"],
            optional_fields: &[],
            error_message: "!$split expects mapping with delimiter and string fields",
        },
        "!$join" => TagValidationConfig {
            allowed_node_types: &["flow_mapping", "block_mapping", "flow_sequence", "block_sequence"],
            required_fields: &["delimiter", "array"],
            optional_fields: &[],
            error_message: "!$join expects mapping with delimiter and array fields or sequence with [delimiter, array]",
        },
        "!$merge" | "!$concat" => TagValidationConfig {
            allowed_node_types: &["flow_mapping", "block_mapping"],
            required_fields: &["sources"],
            optional_fields: &[],
            error_message: "expects mapping with sources field",
        },
        _ => TagValidationConfig {
            allowed_node_types: &[],
            required_fields: &[],
            optional_fields: &[],
            error_message: "Unknown tag",
        },
    }
}

/// Validate node semantics without building AST
pub(crate) fn validate_node_semantics(
    node: Node,
    src: &[u8],
    uri: &Url,
    diagnostics: &mut ParseDiagnostics,
) {
    // Recursively validate all child nodes
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        validate_node_semantics(child, src, uri, diagnostics);
    }

    // Validate this node if it's a tagged node
    if matches!(node.kind(), "flow_node" | "block_node") {
        // Look for tag children, like the original code did
        if let Some(tag_child) = node.child_by_field_name("tag") {
            validate_tagged_node_semantics(tag_child, src, uri, diagnostics);
        } else {
            // Also check direct children for tag nodes (tree-sitter sometimes doesn't use field names)  
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "tag" {
                    validate_tagged_node_semantics(child, src, uri, diagnostics);
                    break; // Only validate the first tag found
                }
            }
        }
    }
}

/// Validate tagged nodes - receives the tag node directly
fn validate_tagged_node_semantics(
    tag_node: Node,
    src: &[u8],
    uri: &Url,
    diagnostics: &mut ParseDiagnostics,
) {
    let meta = node_meta(&tag_node, uri);

    // Extract tag text
    let tag_text = match tag_node.utf8_text(src) {
        Ok(text) => text,
        Err(_) => {
            diagnostics.add_error(
                ParseError::with_location(
                    "Invalid UTF-8 in tag",
                    uri.clone(),
                    meta.start,
                    meta.end,
                )
                .with_code(error_codes::SYNTAX_ERROR),
            );
            return;
        }
    };

    let tag_name = tag_text.split_whitespace().next().unwrap_or(tag_text);

    // Validate known tags
    if tag_name.starts_with("!$") {
        validate_preprocessing_tag_semantics(tag_name, tag_node, src, uri, diagnostics);
    } else if is_known_cloudformation_tag(tag_name) {
        validate_cloudformation_tag_semantics(tag_name, tag_node, src, uri, diagnostics);
    } else if !tag_name.starts_with("!") {
        // Not a tag at all
        return;
    } else {
        // Unknown tag
        diagnostics.add_error(
            ParseError::with_location(
                format!("Unknown tag '{}'", tag_name),
                uri.clone(),
                meta.start,
                meta.end,
            )
            .with_code(error_codes::UNKNOWN_TAG),
        );
    }
}

/// Check if this is a known CloudFormation tag
fn is_known_cloudformation_tag(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "!Ref"
            | "!GetAtt"
            | "!Sub"
            | "!Join"
            | "!Split"
            | "!Select"
            | "!FindInMap"
            | "!ImportValue"
            | "!Condition"
            | "!And"
            | "!Or"
            | "!Not"
            | "!Equals"
            | "!If"
            | "!Base64"
            | "!GetAZs"
            | "!Cidr"
    )
}

/// Validate CloudFormation tags without building AST
fn validate_cloudformation_tag_semantics(
    _tag_name: &str,
    _node: Node,
    _src: &[u8],
    _uri: &Url,
    _diagnostics: &mut ParseDiagnostics,
) {
    // For now, just assume CloudFormation tags are valid
    // Could add specific validation for each tag type later
}

/// Validate preprocessing tags without building AST
fn validate_preprocessing_tag_semantics(
    tag_name: &str,
    tag_node: Node,
    src: &[u8],
    uri: &Url,
    diagnostics: &mut ParseDiagnostics,
) {
    let meta = node_meta(&tag_node, uri);

    // Find the content node (the next sibling of the tag node within the flow_node/block_node)
    let parent_node = if let Some(parent) = tag_node.parent() {
        parent
    } else {
        diagnostics.add_error(
            ParseError::with_location(
                format!("Tag '{}' missing parent context", tag_name),
                uri.clone(),
                meta.start,
                meta.end,
            )
            .with_code(error_codes::SYNTAX_ERROR),
        );
        return;
    };

    // Find content node (should be after the tag)
    let content_node = {
        let mut cursor = parent_node.walk();
        let mut found_tag = false;
        let mut content_node = None;

        for child in parent_node.named_children(&mut cursor) {
            if found_tag && child.kind() != "tag" {
                content_node = Some(child);
                break;
            }
            if child.id() == tag_node.id() {
                found_tag = true;
            }
        }
        content_node
    };

    let content_node = match content_node {
        Some(node) => node,
        None => {
            diagnostics.add_error(
                ParseError::with_location(
                    format!("Tag '{}' missing content", tag_name),
                    uri.clone(),
                    meta.start,
                    meta.end,
                )
                .with_code(error_codes::MISSING_FIELD),
            );
            return;
        }
    };

    match tag_name {
        "!$include" => validate_include_tag_semantics(content_node, src, uri, diagnostics),
        "!$let" => validate_let_tag_semantics(content_node, src, uri, diagnostics),
        "!$map" => validate_map_tag_semantics(content_node, src, uri, diagnostics),
        "!$if" => validate_if_tag_semantics(content_node, src, uri, diagnostics),
        "!$eq" | "!$split" => {
            validate_binary_tag_semantics(tag_name, content_node, src, uri, diagnostics)
        }
        "!$join" => validate_join_tag_semantics(content_node, src, uri, diagnostics),
        "!$merge" | "!$concat" => {
            validate_variadic_tag_semantics(tag_name, content_node, src, uri, diagnostics)
        }
        _ => {
            // Unknown preprocessing tag
            diagnostics.add_error(
                ParseError::with_location(
                    format!("Unknown preprocessing tag '{}'", tag_name),
                    uri.clone(),
                    meta.start,
                    meta.end,
                )
                .with_code(error_codes::UNKNOWN_TAG),
            );
        }
    }
}

/// Generic tag content validation
#[inline]
fn validate_tag_content(
    tag_name: &str,
    content_node: Node,
    src: &[u8],
    uri: &Url,
    diagnostics: &mut ParseDiagnostics,
) {
    let config = get_tag_validation_config(tag_name);
    let meta = node_meta(&content_node, uri);

    // Check if node type is allowed
    let node_kind = content_node.kind();
    if !config.allowed_node_types.contains(&node_kind) {
        diagnostics.add_error(
            ParseError::with_location(
                if tag_name.starts_with("!$merge") || tag_name.starts_with("!$concat") {
                    format!("{} {}", tag_name, config.error_message)
                } else {
                    config.error_message.to_string()
                },
                uri.clone(),
                meta.start,
                meta.end,
            )
            .with_code(error_codes::INVALID_TYPE),
        );
        return;
    }

    // Special handling for different node types
    match node_kind {
        "plain_scalar" | "single_quote_scalar" | "double_quote_scalar" => {
            // Scalar values are valid for some tags (like !$include)
            return;
        }
        "flow_mapping" | "block_mapping" => {
            // Validate mapping fields
            validate_mapping_fields(
                content_node,
                src,
                uri,
                config.required_fields,
                config.optional_fields,
                tag_name,
                diagnostics,
            );
        }
        "flow_sequence" | "block_sequence" => {
            // Special handling for !$join sequence form
            if tag_name == "!$join" {
                let child_count = content_node.named_child_count();
                if child_count != 2 {
                    diagnostics.add_error(
                        ParseError::with_location(
                            format!("!$join sequence form expects exactly 2 elements (delimiter, array), found {}", child_count),
                            uri.clone(),
                            meta.start,
                            meta.end,
                        )
                        .with_code(error_codes::INVALID_FORMAT),
                    );
                }
            }
        }
        _ => {
            // Unexpected node type - this should be caught by the allowed_node_types check above
        }
    }
}

/// Validate !$include tag structure
fn validate_include_tag_semantics(
    content_node: Node,
    src: &[u8],
    uri: &Url,
    diagnostics: &mut ParseDiagnostics,
) {
    validate_tag_content("!$include", content_node, src, uri, diagnostics);
}

/// Validate !$let tag structure
fn validate_let_tag_semantics(
    content_node: Node,
    src: &[u8],
    uri: &Url,
    diagnostics: &mut ParseDiagnostics,
) {
    validate_tag_content("!$let", content_node, src, uri, diagnostics);
}

/// Validate !$map tag structure
fn validate_map_tag_semantics(
    content_node: Node,
    src: &[u8],
    uri: &Url,
    diagnostics: &mut ParseDiagnostics,
) {
    validate_tag_content("!$map", content_node, src, uri, diagnostics);
}

/// Validate !$if tag structure
fn validate_if_tag_semantics(
    content_node: Node,
    src: &[u8],
    uri: &Url,
    diagnostics: &mut ParseDiagnostics,
) {
    validate_tag_content("!$if", content_node, src, uri, diagnostics);
}

/// Validate binary operation tags like !$eq, !$split
fn validate_binary_tag_semantics(
    tag_name: &str,
    content_node: Node,
    src: &[u8],
    uri: &Url,
    diagnostics: &mut ParseDiagnostics,
) {
    validate_tag_content(tag_name, content_node, src, uri, diagnostics);
}

/// Validate !$join tag structure (supports both mapping and sequence forms)
fn validate_join_tag_semantics(
    content_node: Node,
    src: &[u8],
    uri: &Url,
    diagnostics: &mut ParseDiagnostics,
) {
    validate_tag_content("!$join", content_node, src, uri, diagnostics);
}

/// Validate variadic operation tags like !$merge, !$concat
fn validate_variadic_tag_semantics(
    tag_name: &str,
    content_node: Node,
    src: &[u8],
    uri: &Url,
    diagnostics: &mut ParseDiagnostics,
) {
    validate_tag_content(tag_name, content_node, src, uri, diagnostics);
}

/// Validate mapping has required fields
fn validate_mapping_fields(
    mapping_node: Node,
    src: &[u8],
    uri: &Url,
    required_fields: &[&str],
    optional_fields: &[&str],
    tag_name: &str,
    diagnostics: &mut ParseDiagnostics,
) {
    let mut found_fields = HashSet::with_capacity(required_fields.len() + optional_fields.len());

    // Walk through mapping pairs
    let mut cursor = mapping_node.walk();
    for child in mapping_node.named_children(&mut cursor) {
        if child.kind() == "flow_pair" || child.kind() == "block_mapping_pair" {
            if let Some(key_node) = child.child_by_field_name("key") {
                if let Ok(key_text) = key_node.utf8_text(src) {
                    // Extract key (remove quotes if present)
                    let key = if key_text.starts_with('"')
                        && key_text.ends_with('"')
                        && key_text.len() >= 2
                    {
                        &key_text[1..key_text.len() - 1]
                    } else if key_text.starts_with('\'')
                        && key_text.ends_with('\'')
                        && key_text.len() >= 2
                    {
                        &key_text[1..key_text.len() - 1]
                    } else {
                        key_text
                    };
                    found_fields.insert(key);
                }
            }
        }
    }

    // Check for missing required fields
    for &required_field in required_fields {
        if !found_fields.contains(required_field) {
            let meta = node_meta(&mapping_node, uri);
            diagnostics.add_error(
                ParseError::with_location(
                    format!(
                        "Missing required '{}' field in {} tag",
                        required_field, tag_name
                    ),
                    uri.clone(),
                    meta.start,
                    meta.end,
                )
                .with_code(error_codes::MISSING_FIELD),
            );
        }
    }

    // Check for unexpected fields (warnings)
    let all_valid_fields: HashSet<_> = required_fields
        .iter()
        .chain(optional_fields.iter())
        .cloned()
        .collect();

    for found_field in &found_fields {
        if !all_valid_fields.contains(found_field) {
            let meta = node_meta(&mapping_node, uri);
            diagnostics.add_warning(ParseWarning::with_location(
                format!("Unexpected field '{}' in {} tag", found_field, tag_name),
                uri.clone(),
                meta.start,
                meta.end,
            ));
        }
    }
}