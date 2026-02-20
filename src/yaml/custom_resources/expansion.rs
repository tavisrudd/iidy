use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};
use serde_yaml::{Mapping, Value};

use crate::yaml::custom_resources::TemplateInfo;
use crate::yaml::custom_resources::params::{merge_params, validate_params};
use crate::yaml::custom_resources::ref_rewriting::rewrite_refs;
use crate::yaml::parsing;
use crate::yaml::resolution::context::TagContext;
use crate::yaml::resolution::resolve_ast;

pub(crate) const GLOBAL_SECTION_NAMES: &[&str] = &[
    "Parameters",
    "Metadata",
    "Mappings",
    "Conditions",
    "Transform",
    "Outputs",
];

/// Expand a custom resource into its constituent resources and global sections.
///
/// Given a resource entry like:
///   OrderEvents:
///     Type: MonitoredQueue
///     Properties: { QueueLabel: OrderEvents }
///
/// This re-parses and resolves the template with Properties as params,
/// prefixes resource names (e.g., Queue -> OrderEventsQueue),
/// rewrites Ref/GetAtt/Sub references, and accumulates global sections.
pub fn expand_custom_resource(
    name: &str,
    resource_value: &Value,
    template_info: &TemplateInfo,
    context: &TagContext,
) -> Result<Vec<(String, Value)>> {
    let resource_map = resource_value
        .as_mapping()
        .ok_or_else(|| anyhow!("Custom resource '{}' must be a mapping", name))?;

    // Extract prefix: NamePrefix override or the resource name
    let prefix = resource_map
        .get(Value::String("NamePrefix".into()))
        .and_then(|v| v.as_str())
        .unwrap_or(name);

    // Extract provided Properties (the param values)
    let provided_params = extract_properties(resource_map);

    // Merge with defaults and validate
    let merged = merge_params(&template_info.params, &provided_params);
    validate_params(&template_info.params, &merged, name)?;

    // Build sub-context with merged params + Prefix binding
    let mut bindings = merged.clone();
    bindings.insert("Prefix".to_string(), Value::String(prefix.to_string()));
    let sub_context = context
        .with_bindings(bindings)
        .with_input_uri(template_info.location.clone());

    // Re-parse and resolve the template body
    let ast = parsing::parse_yaml_from_file(&template_info.raw_body, &template_info.location)?;
    let mut resolved = resolve_ast(&ast, &sub_context)?;

    // Deep-merge Overrides into the resolved template (Overrides already resolved by outer context)
    if let Some(overrides) = resource_map.get(Value::String("Overrides".into())) {
        deep_merge(&mut resolved, overrides);
    }

    let resolved_map = resolved.as_mapping().ok_or_else(|| {
        anyhow!(
            "Custom resource template '{}' must resolve to a mapping",
            name
        )
    })?;

    // Collect global refs: entries with $global: true across sections + params with is_global
    let global_refs = collect_global_refs(resolved_map, &template_info.params);

    // Extract and process the Resources section
    let resources_value = resolved_map
        .get(Value::String("Resources".into()))
        .ok_or_else(|| {
            anyhow!(
                "Custom resource template for '{}' must have a Resources section",
                name
            )
        })?;

    // Rewrite refs in the Resources section
    let rewritten_resources = rewrite_refs(resources_value, prefix, &global_refs);

    // Prefix resource names and strip $global
    let expanded_resources = prefix_and_strip_global(&rewritten_resources, prefix)?;

    // Accumulate global sections
    accumulate_globals(resolved_map, prefix, &global_refs, context)?;

    Ok(expanded_resources)
}

/// Recursively deep-merge `source` into `target`. Mappings are merged key-by-key;
/// all other types are overwritten by `source`.
fn deep_merge(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Mapping(target_map), Value::Mapping(source_map)) => {
            for (key, source_val) in source_map {
                if let Some(target_val) = target_map.get_mut(key) {
                    deep_merge(target_val, source_val);
                } else {
                    target_map.insert(key.clone(), source_val.clone());
                }
            }
        }
        (target, source) => {
            *target = source.clone();
        }
    }
}

/// Extract Properties mapping from a resource value into a HashMap.
fn extract_properties(resource_map: &Mapping) -> HashMap<String, Value> {
    let mut result = HashMap::new();
    if let Some(props) = resource_map.get(Value::String("Properties".into()))
        && let Some(props_map) = props.as_mapping()
    {
        for (k, v) in props_map {
            if let Some(key_str) = k.as_str() {
                result.insert(key_str.to_string(), v.clone());
            }
        }
    }
    result
}

/// Collect names that should NOT be prefixed during ref rewriting.
/// These are entries marked with $global: true in any section, plus params with is_global.
fn collect_global_refs(
    resolved_map: &Mapping,
    params: &[crate::yaml::custom_resources::params::ParamDef],
) -> HashSet<String> {
    let mut global_refs = HashSet::new();

    // Params with is_global
    for param in params {
        if param.is_global {
            global_refs.insert(param.name.clone());
        }
    }

    // Scan sections for entries with $global: true
    let sections_to_scan = ["Parameters", "Resources", "Mappings", "Conditions"];
    for section_name in &sections_to_scan {
        if let Some(section) = resolved_map.get(Value::String((*section_name).to_string()))
            && let Some(section_map) = section.as_mapping()
        {
            for (key, value) in section_map {
                if has_global_flag(value)
                    && let Some(key_str) = key.as_str()
                {
                    global_refs.insert(key_str.to_string());
                }
            }
        }
    }

    global_refs
}

/// Check if a value has $global: true
fn has_global_flag(value: &Value) -> bool {
    value
        .as_mapping()
        .and_then(|m| m.get(Value::String("$global".into())))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Prefix resource names and strip $global flag from values.
/// Resources with $global: true keep their original name.
fn prefix_and_strip_global(resources: &Value, prefix: &str) -> Result<Vec<(String, Value)>> {
    let resources_map = resources
        .as_mapping()
        .ok_or_else(|| anyhow!("Resources section must be a mapping"))?;

    let mut result = Vec::with_capacity(resources_map.len());
    for (key, value) in resources_map {
        let key_str = key
            .as_str()
            .ok_or_else(|| anyhow!("Resource key must be a string"))?;

        let is_global = has_global_flag(value);
        let output_name = if is_global {
            key_str.to_string()
        } else {
            format!("{prefix}{key_str}")
        };

        // Strip $global from the value
        let clean_value = strip_global_key(value);
        result.push((output_name, clean_value));
    }
    Ok(result)
}

/// Remove the $global key from a mapping value (if present).
fn strip_global_key(value: &Value) -> Value {
    if let Value::Mapping(map) = value {
        let global_key = Value::String("$global".into());
        if map.contains_key(&global_key) {
            let mut cleaned = map.clone();
            cleaned.remove(&global_key);
            return Value::Mapping(cleaned);
        }
    }
    value.clone()
}

/// Accumulate global sections (Parameters, Outputs, etc.) from the resolved template
/// into the shared accumulated_globals on the context.
fn accumulate_globals(
    resolved_map: &Mapping,
    prefix: &str,
    global_refs: &HashSet<String>,
    context: &TagContext,
) -> Result<()> {
    let mut acc = context.accumulated_globals.borrow_mut();

    for section_name in GLOBAL_SECTION_NAMES {
        let section_key = Value::String((*section_name).to_string());
        let Some(section_value) = resolved_map.get(&section_key) else {
            continue;
        };
        let Some(section_map) = section_value.as_mapping() else {
            continue;
        };

        let target = acc.entry(section_name.to_string()).or_default();

        for (key, value) in section_map {
            let key_str = key.as_str().unwrap_or("");
            let is_global = has_global_flag(value);
            let output_key = if is_global {
                key_str.to_string()
            } else {
                format!("{prefix}{key_str}")
            };
            let clean_value = strip_global_key(value);

            // Rewrite refs in the global section entry
            let rewritten = rewrite_refs(&clean_value, prefix, global_refs);
            target.insert(Value::String(output_key), rewritten);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_properties_basic() {
        let mut map = Mapping::new();
        let mut props = Mapping::new();
        props.insert(
            Value::String("QueueLabel".into()),
            Value::String("OrderEvents".into()),
        );
        props.insert(
            Value::String("AlarmPriority".into()),
            Value::String("P2".into()),
        );
        map.insert(Value::String("Properties".into()), Value::Mapping(props));

        let result = extract_properties(&map);
        assert_eq!(result.len(), 2);
        assert_eq!(result["QueueLabel"], Value::String("OrderEvents".into()));
        assert_eq!(result["AlarmPriority"], Value::String("P2".into()));
    }

    #[test]
    fn extract_properties_missing() {
        let map = Mapping::new();
        let result = extract_properties(&map);
        assert!(result.is_empty());
    }

    #[test]
    fn has_global_flag_true() {
        let mut map = Mapping::new();
        map.insert(Value::String("Type".into()), Value::String("String".into()));
        map.insert(Value::String("$global".into()), Value::Bool(true));
        assert!(has_global_flag(&Value::Mapping(map)));
    }

    #[test]
    fn has_global_flag_false() {
        let mut map = Mapping::new();
        map.insert(Value::String("Type".into()), Value::String("String".into()));
        assert!(!has_global_flag(&Value::Mapping(map)));
    }

    #[test]
    fn has_global_flag_non_mapping() {
        assert!(!has_global_flag(&Value::String("foo".into())));
    }

    #[test]
    fn strip_global_key_removes_flag() {
        let mut map = Mapping::new();
        map.insert(Value::String("Type".into()), Value::String("String".into()));
        map.insert(Value::String("$global".into()), Value::Bool(true));
        let result = strip_global_key(&Value::Mapping(map));
        let result_map = result.as_mapping().unwrap();
        assert_eq!(result_map.len(), 1);
        assert!(result_map.contains_key(Value::String("Type".into())));
        assert!(!result_map.contains_key(Value::String("$global".into())));
    }

    #[test]
    fn strip_global_key_noop_when_absent() {
        let mut map = Mapping::new();
        map.insert(Value::String("Type".into()), Value::String("String".into()));
        let original = Value::Mapping(map.clone());
        let result = strip_global_key(&original);
        assert_eq!(result, original);
    }

    #[test]
    fn prefix_and_strip_global_basic() {
        let mut resources = Mapping::new();

        let mut queue = Mapping::new();
        queue.insert(
            Value::String("Type".into()),
            Value::String("AWS::SQS::Queue".into()),
        );
        resources.insert(Value::String("Queue".into()), Value::Mapping(queue));

        let mut alarm = Mapping::new();
        alarm.insert(
            Value::String("Type".into()),
            Value::String("AWS::CloudWatch::Alarm".into()),
        );
        resources.insert(
            Value::String("QueueDepthAlarm".into()),
            Value::Mapping(alarm),
        );

        let result = prefix_and_strip_global(&Value::Mapping(resources), "OrderEvents").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "OrderEventsQueue");
        assert_eq!(result[1].0, "OrderEventsQueueDepthAlarm");
    }

    #[test]
    fn prefix_and_strip_global_with_global_flag() {
        let mut resources = Mapping::new();

        let mut global_res = Mapping::new();
        global_res.insert(
            Value::String("Type".into()),
            Value::String("AWS::SQS::Queue".into()),
        );
        global_res.insert(Value::String("$global".into()), Value::Bool(true));
        resources.insert(
            Value::String("SharedQueue".into()),
            Value::Mapping(global_res),
        );

        let mut regular = Mapping::new();
        regular.insert(
            Value::String("Type".into()),
            Value::String("AWS::SQS::Queue".into()),
        );
        resources.insert(Value::String("LocalQueue".into()), Value::Mapping(regular));

        let result = prefix_and_strip_global(&Value::Mapping(resources), "Prefix").unwrap();
        assert_eq!(result.len(), 2);
        // Global resource keeps original name
        assert_eq!(result[0].0, "SharedQueue");
        // $global stripped from value
        assert!(
            !result[0]
                .1
                .as_mapping()
                .unwrap()
                .contains_key(Value::String("$global".into()))
        );
        // Non-global resource gets prefixed
        assert_eq!(result[1].0, "PrefixLocalQueue");
    }

    #[test]
    fn collect_global_refs_from_params() {
        let params = vec![
            crate::yaml::custom_resources::params::ParamDef {
                name: "RoleName".into(),
                default: None,
                param_type: None,
                allowed_values: None,
                allowed_pattern: None,
                schema: None,
                is_global: true,
            },
            crate::yaml::custom_resources::params::ParamDef {
                name: "Other".into(),
                default: None,
                param_type: None,
                allowed_values: None,
                allowed_pattern: None,
                schema: None,
                is_global: false,
            },
        ];
        let map = Mapping::new();
        let refs = collect_global_refs(&map, &params);
        assert!(refs.contains("RoleName"));
        assert!(!refs.contains("Other"));
    }

    #[test]
    fn collect_global_refs_from_sections() {
        let mut map = Mapping::new();
        let mut params_section = Mapping::new();

        let mut env_param = Mapping::new();
        env_param.insert(Value::String("Type".into()), Value::String("String".into()));
        env_param.insert(Value::String("$global".into()), Value::Bool(true));
        params_section.insert(
            Value::String("Environment".into()),
            Value::Mapping(env_param),
        );

        let mut local_param = Mapping::new();
        local_param.insert(Value::String("Type".into()), Value::String("String".into()));
        params_section.insert(
            Value::String("LocalParam".into()),
            Value::Mapping(local_param),
        );

        map.insert(
            Value::String("Parameters".into()),
            Value::Mapping(params_section),
        );

        let refs = collect_global_refs(&map, &[]);
        assert!(refs.contains("Environment"));
        assert!(!refs.contains("LocalParam"));
    }
}
