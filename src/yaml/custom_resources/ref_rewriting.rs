use std::collections::HashSet;

use regex::Regex;
use serde_yaml::Value;
use serde_yaml::value::{Tag, TaggedValue};

/// Walk a Value tree, rewriting CFN refs by prepending `prefix`.
/// `global_refs` contains names that should NOT be prefixed (e.g., $global entries).
pub fn rewrite_refs(value: &Value, prefix: &str, global_refs: &HashSet<String>) -> Value {
    walk(value, prefix, global_refs)
}

fn should_rewrite(ref_name: &str, global_refs: &HashSet<String>) -> bool {
    // AWS pseudo-references use single colon (AWS::StackName, etc.)
    if ref_name.starts_with("AWS:") {
        return false;
    }
    if global_refs.contains(ref_name) {
        return false;
    }
    true
}

fn walk(value: &Value, prefix: &str, global_refs: &HashSet<String>) -> Value {
    match value {
        Value::Tagged(tagged) => rewrite_tagged(tagged, prefix, global_refs),
        Value::Mapping(map) => walk_mapping(map, prefix, global_refs),
        Value::Sequence(seq) => {
            Value::Sequence(seq.iter().map(|v| walk(v, prefix, global_refs)).collect())
        }
        other => other.clone(),
    }
}

fn rewrite_tagged(tagged: &TaggedValue, prefix: &str, global_refs: &HashSet<String>) -> Value {
    let tag_str = tagged.tag.to_string();
    let tag_name = tag_str.strip_prefix('!').unwrap_or(&tag_str);

    match tag_name {
        "Ref" => rewrite_ref_tag(tagged, prefix, global_refs),
        "GetAtt" => rewrite_getatt_tag(tagged, prefix, global_refs),
        "Sub" => rewrite_sub_tag(tagged, prefix, global_refs),
        _ => {
            // Recurse into the tagged value's content
            let rewritten = walk(&tagged.value, prefix, global_refs);
            Value::Tagged(Box::new(TaggedValue {
                tag: tagged.tag.clone(),
                value: rewritten,
            }))
        }
    }
}

fn rewrite_ref_tag(tagged: &TaggedValue, prefix: &str, global_refs: &HashSet<String>) -> Value {
    if let Value::String(name) = &tagged.value {
        let trimmed = name.trim();
        if should_rewrite(trimmed, global_refs) {
            return make_tagged("Ref", Value::String(format!("{}{}", prefix, trimmed)));
        }
    }
    // If not a string or not rewritable, return as-is
    Value::Tagged(Box::new(tagged.clone()))
}

fn rewrite_getatt_tag(tagged: &TaggedValue, prefix: &str, global_refs: &HashSet<String>) -> Value {
    if let Value::String(dotted) = &tagged.value {
        let trimmed = dotted.trim();
        if let Some(first_dot) = trimmed.find('.') {
            let resource_name = &trimmed[..first_dot];
            if should_rewrite(resource_name, global_refs) {
                return make_tagged("GetAtt", Value::String(format!("{}{}", prefix, trimmed)));
            }
        } else if should_rewrite(trimmed, global_refs) {
            return make_tagged("GetAtt", Value::String(format!("{}{}", prefix, trimmed)));
        }
    }
    Value::Tagged(Box::new(tagged.clone()))
}

fn rewrite_sub_tag(tagged: &TaggedValue, prefix: &str, global_refs: &HashSet<String>) -> Value {
    match &tagged.value {
        Value::String(template) => {
            let rewritten = rewrite_sub_template(template, prefix, global_refs);
            make_tagged("Sub", Value::String(rewritten))
        }
        Value::Sequence(seq) if seq.len() == 2 => {
            // !Sub [template, {vars}]
            // Variables in the vars mapping are added as globals (not prefixed)
            let mut extended_globals = global_refs.clone();
            if let Some(vars_map) = seq[1].as_mapping() {
                for key in vars_map.keys() {
                    if let Some(k) = key.as_str() {
                        extended_globals.insert(k.to_string());
                    }
                }
            }
            let template = seq[0].as_str().unwrap_or("");
            let rewritten_template = rewrite_sub_template(template, prefix, &extended_globals);
            let rewritten_vars = walk(&seq[1], prefix, global_refs);
            make_tagged(
                "Sub",
                Value::Sequence(vec![Value::String(rewritten_template), rewritten_vars]),
            )
        }
        other => {
            // Unexpected shape -- recurse into it
            let rewritten = walk(other, prefix, global_refs);
            make_tagged("Sub", rewritten)
        }
    }
}

fn rewrite_sub_template(template: &str, prefix: &str, global_refs: &HashSet<String>) -> String {
    // Match ${RefName} but not ${!literal} -- the ! prefix means "literal" in CFN Sub
    let re = Regex::new(r"\$\{([^!].*?)\}").unwrap();
    re.replace_all(template, |caps: &regex::Captures| {
        let inner = &caps[1];
        let ref_name = inner.trim().split('.').next().unwrap_or("");
        if should_rewrite(ref_name, global_refs) {
            format!("${{{}{}}}", prefix, inner.trim())
        } else {
            caps[0].to_string()
        }
    })
    .into_owned()
}

fn walk_mapping(map: &serde_yaml::Mapping, prefix: &str, global_refs: &HashSet<String>) -> Value {
    // Check if this is a CFN resource entry (has a "Type" key)
    let is_resource_entry = map.contains_key(&Value::String("Type".into()));

    let mut result = serde_yaml::Mapping::new();
    for (key, value) in map {
        let rewritten_value = if is_resource_entry {
            rewrite_resource_field(key, value, prefix, global_refs)
        } else {
            walk(value, prefix, global_refs)
        };
        result.insert(key.clone(), rewritten_value);
    }
    Value::Mapping(result)
}

fn rewrite_resource_field(
    key: &Value,
    value: &Value,
    prefix: &str,
    global_refs: &HashSet<String>,
) -> Value {
    let key_str = key.as_str().unwrap_or("");
    match key_str {
        "Condition" => {
            if let Value::String(name) = value {
                if should_rewrite(name.trim(), global_refs) {
                    return Value::String(format!("{}{}", prefix, name.trim()));
                }
            }
            value.clone()
        }
        "DependsOn" => rewrite_depends_on(value, prefix, global_refs),
        _ => walk(value, prefix, global_refs),
    }
}

fn rewrite_depends_on(value: &Value, prefix: &str, global_refs: &HashSet<String>) -> Value {
    match value {
        Value::String(name) => {
            if should_rewrite(name.trim(), global_refs) {
                Value::String(format!("{}{}", prefix, name.trim()))
            } else {
                value.clone()
            }
        }
        Value::Sequence(seq) => Value::Sequence(
            seq.iter()
                .map(|v| {
                    if let Value::String(name) = v {
                        if should_rewrite(name.trim(), global_refs) {
                            Value::String(format!("{}{}", prefix, name.trim()))
                        } else {
                            v.clone()
                        }
                    } else {
                        v.clone()
                    }
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

fn make_tagged(tag_name: &str, value: Value) -> Value {
    Value::Tagged(Box::new(TaggedValue {
        tag: Tag::new(tag_name),
        value,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Mapping;

    fn globals(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn make_ref(name: &str) -> Value {
        make_tagged("Ref", Value::String(name.into()))
    }

    fn make_getatt(dotted: &str) -> Value {
        make_tagged("GetAtt", Value::String(dotted.into()))
    }

    fn make_sub(template: &str) -> Value {
        make_tagged("Sub", Value::String(template.into()))
    }

    fn extract_tagged_string(val: &Value) -> &str {
        match val {
            Value::Tagged(t) => t.value.as_str().unwrap(),
            _ => panic!("expected tagged value"),
        }
    }

    #[test]
    fn ref_rewrite_basic() {
        let val = make_ref("Queue");
        let result = rewrite_refs(&val, "OrderEvents", &HashSet::new());
        assert_eq!(extract_tagged_string(&result), "OrderEventsQueue");
    }

    #[test]
    fn ref_aws_no_rewrite() {
        let val = make_ref("AWS::StackName");
        let result = rewrite_refs(&val, "Prefix", &HashSet::new());
        assert_eq!(extract_tagged_string(&result), "AWS::StackName");
    }

    #[test]
    fn ref_global_no_rewrite() {
        let val = make_ref("AlertTopicArn");
        let result = rewrite_refs(&val, "Prefix", &globals(&["AlertTopicArn"]));
        assert_eq!(extract_tagged_string(&result), "AlertTopicArn");
    }

    #[test]
    fn getatt_dotted_rewrite() {
        let val = make_getatt("Queue.QueueName");
        let result = rewrite_refs(&val, "OrderEvents", &HashSet::new());
        assert_eq!(extract_tagged_string(&result), "OrderEventsQueue.QueueName");
    }

    #[test]
    fn getatt_global_no_rewrite() {
        let val = make_getatt("SharedResource.Arn");
        let result = rewrite_refs(&val, "Prefix", &globals(&["SharedResource"]));
        assert_eq!(extract_tagged_string(&result), "SharedResource.Arn");
    }

    #[test]
    fn sub_template_rewrite() {
        let val = make_sub("arn:aws:sqs:${AWS::Region}:${AWS::AccountId}:${Queue}");
        let result = rewrite_refs(&val, "OrderEvents", &HashSet::new());
        assert_eq!(
            extract_tagged_string(&result),
            "arn:aws:sqs:${AWS::Region}:${AWS::AccountId}:${OrderEventsQueue}"
        );
    }

    #[test]
    fn sub_template_with_getatt_style() {
        let val = make_sub("${Queue.QueueName}");
        let result = rewrite_refs(&val, "Prefix", &HashSet::new());
        assert_eq!(extract_tagged_string(&result), "${PrefixQueue.QueueName}");
    }

    #[test]
    fn sub_with_vars() {
        // !Sub [template, {LocalVar: value}]
        let mut vars = Mapping::new();
        vars.insert(Value::String("LocalVar".into()), make_ref("SomeResource"));
        let sub_value = Value::Sequence(vec![
            Value::String("${LocalVar}-${Queue}".into()),
            Value::Mapping(vars),
        ]);
        let val = make_tagged("Sub", sub_value);
        let result = rewrite_refs(&val, "Prefix", &HashSet::new());

        // LocalVar should NOT be rewritten (it's in the vars map)
        // Queue should be rewritten
        if let Value::Tagged(t) = &result {
            if let Value::Sequence(seq) = &t.value {
                assert_eq!(seq[0].as_str().unwrap(), "${LocalVar}-${PrefixQueue}");
                // The vars mapping values should be walked (SomeResource ref rewritten)
                if let Value::Mapping(m) = &seq[1] {
                    let ref_val = m.get(&Value::String("LocalVar".into())).unwrap();
                    assert_eq!(extract_tagged_string(ref_val), "PrefixSomeResource");
                } else {
                    panic!("expected mapping");
                }
            } else {
                panic!("expected sequence");
            }
        } else {
            panic!("expected tagged");
        }
    }

    #[test]
    fn condition_rewrite() {
        let mut resource = Mapping::new();
        resource.insert(
            Value::String("Type".into()),
            Value::String("AWS::SQS::Queue".into()),
        );
        resource.insert(
            Value::String("Condition".into()),
            Value::String("IsProduction".into()),
        );
        resource.insert(
            Value::String("Properties".into()),
            Value::Mapping(Mapping::new()),
        );

        let result = rewrite_refs(&Value::Mapping(resource), "Prefix", &HashSet::new());
        if let Value::Mapping(m) = result {
            let cond = m.get(&Value::String("Condition".into())).unwrap();
            assert_eq!(cond.as_str().unwrap(), "PrefixIsProduction");
        } else {
            panic!("expected mapping");
        }
    }

    #[test]
    fn depends_on_scalar() {
        let mut resource = Mapping::new();
        resource.insert(
            Value::String("Type".into()),
            Value::String("AWS::SQS::Queue".into()),
        );
        resource.insert(
            Value::String("DependsOn".into()),
            Value::String("OtherResource".into()),
        );

        let result = rewrite_refs(&Value::Mapping(resource), "Prefix", &HashSet::new());
        if let Value::Mapping(m) = result {
            let dep = m.get(&Value::String("DependsOn".into())).unwrap();
            assert_eq!(dep.as_str().unwrap(), "PrefixOtherResource");
        } else {
            panic!("expected mapping");
        }
    }

    #[test]
    fn depends_on_array() {
        let mut resource = Mapping::new();
        resource.insert(
            Value::String("Type".into()),
            Value::String("AWS::Lambda::Function".into()),
        );
        resource.insert(
            Value::String("DependsOn".into()),
            Value::Sequence(vec![
                Value::String("ResourceA".into()),
                Value::String("ResourceB".into()),
            ]),
        );

        let result = rewrite_refs(&Value::Mapping(resource), "Prefix", &HashSet::new());
        if let Value::Mapping(m) = result {
            let dep = m.get(&Value::String("DependsOn".into())).unwrap();
            if let Value::Sequence(seq) = dep {
                assert_eq!(seq[0].as_str().unwrap(), "PrefixResourceA");
                assert_eq!(seq[1].as_str().unwrap(), "PrefixResourceB");
            } else {
                panic!("expected sequence");
            }
        } else {
            panic!("expected mapping");
        }
    }

    #[test]
    fn nested_structure() {
        // A mapping containing a ref nested inside properties
        let mut props = Mapping::new();
        props.insert(Value::String("QueueArn".into()), make_getatt("Queue.Arn"));
        props.insert(Value::String("TopicArn".into()), make_ref("AlertTopicArn"));

        let mut resource = Mapping::new();
        resource.insert(
            Value::String("Type".into()),
            Value::String("AWS::SNS::Subscription".into()),
        );
        resource.insert(Value::String("Properties".into()), Value::Mapping(props));

        let result = rewrite_refs(
            &Value::Mapping(resource),
            "Prefix",
            &globals(&["AlertTopicArn"]),
        );

        if let Value::Mapping(m) = result {
            let props = m
                .get(&Value::String("Properties".into()))
                .unwrap()
                .as_mapping()
                .unwrap();
            let queue_arn = props.get(&Value::String("QueueArn".into())).unwrap();
            assert_eq!(extract_tagged_string(queue_arn), "PrefixQueue.Arn");
            let topic_arn = props.get(&Value::String("TopicArn".into())).unwrap();
            assert_eq!(extract_tagged_string(topic_arn), "AlertTopicArn");
        } else {
            panic!("expected mapping");
        }
    }
}
