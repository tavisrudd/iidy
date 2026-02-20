use anyhow::Result;

use crate::{
    aws::{config_from_normalized_opts, format_aws_error},
    cli::{Cli, GetImportArgs},
    yaml::imports::{ImportLoader, loaders::ProductionImportLoader},
    yaml::jmespath::{apply_jmespath_query, yaml_to_json_value},
};

/// Retrieve and display data from any import location supported by the iidy import system.
///
/// This is a data extraction command (output piped to stdout), so it uses
/// direct stderr output rather than the output manager.
pub async fn get_import(cli: &Cli, args: &GetImportArgs) -> Result<i32> {
    let normalized_opts = cli.aws_opts.clone().normalize();

    let aws_config = match config_from_normalized_opts(&normalized_opts).await {
        Ok((config, _credential_sources)) => Some(config),
        Err(e) => {
            eprintln!("{}", format_aws_error(&e));
            return Ok(1);
        }
    };

    let import_loader = match aws_config {
        Some(config) => ProductionImportLoader::new().with_aws_config(config),
        None => ProductionImportLoader::new(),
    };

    let base_location = ".";
    let import_data = match import_loader.load(&args.import, base_location).await {
        Ok(data) => data,
        Err(e) => {
            let error_msg = if e.to_string().contains("AWS") {
                format_aws_error(&e)
            } else {
                format!("Import error: {e}")
            };
            eprintln!("{error_msg}");
            return Ok(1);
        }
    };

    let mut output_doc = import_data.doc;
    if let Some(query_str) = &args.query {
        match apply_jmespath_query(&output_doc, query_str) {
            Ok(result) => output_doc = result,
            Err(e) => {
                eprintln!("{e}");
                return Ok(1);
            }
        }
    }

    match args.format.as_str() {
        "yaml" => match serde_yaml::to_string(&output_doc) {
            Ok(yaml_str) => print!("{yaml_str}"),
            Err(e) => {
                eprintln!("YAML serialization error: {e}");
                return Ok(1);
            }
        },
        "json" => match serde_json::to_string_pretty(&yaml_to_json_value(&output_doc)?) {
            Ok(json_str) => println!("{json_str}"),
            Err(e) => {
                eprintln!("JSON serialization error: {e}");
                return Ok(1);
            }
        },
        _ => {
            eprintln!(
                "Unsupported format: '{}'. Use 'yaml' or 'json'.",
                args.format
            );
            return Ok(1);
        }
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use crate::yaml::jmespath::{apply_jmespath_query, json_to_yaml_value, yaml_to_json_value};
    use serde_json::json;
    use serde_yaml::Value as YamlValue;

    #[test]
    fn test_yaml_to_json_conversion() {
        let yaml_value = serde_yaml::from_str(
            r#"
            name: "test"
            values: [1, 2, 3]
            nested:
              key: "value"
        "#,
        )
        .unwrap();

        let json_value = yaml_to_json_value(&yaml_value).unwrap();

        assert_eq!(json_value["name"], "test");
        assert_eq!(json_value["values"], json!([1, 2, 3]));
        assert_eq!(json_value["nested"]["key"], "value");
    }

    #[test]
    fn test_json_to_yaml_conversion() {
        let json_value = json!({
            "name": "test",
            "values": [1, 2, 3],
            "nested": {
                "key": "value"
            }
        });

        let yaml_value = json_to_yaml_value(&json_value).unwrap();

        if let YamlValue::Mapping(map) = yaml_value {
            assert!(map.contains_key(YamlValue::String("name".to_string())));
            assert!(map.contains_key(YamlValue::String("values".to_string())));
            assert!(map.contains_key(YamlValue::String("nested".to_string())));
        } else {
            panic!("Expected YAML mapping");
        }
    }

    #[test]
    fn test_round_trip_conversion() {
        let original_yaml = serde_yaml::from_str(
            r#"
            database:
              host: "localhost"
              port: 5432
              enabled: true
            features: ["auth", "logging"]
        "#,
        )
        .unwrap();

        let json_value = yaml_to_json_value(&original_yaml).unwrap();
        let final_yaml = json_to_yaml_value(&json_value).unwrap();

        let original_str = serde_yaml::to_string(&original_yaml).unwrap();
        let final_str = serde_yaml::to_string(&final_yaml).unwrap();

        let original_reparsed: serde_yaml::Value = serde_yaml::from_str(&original_str).unwrap();
        let final_reparsed: serde_yaml::Value = serde_yaml::from_str(&final_str).unwrap();

        assert_eq!(original_reparsed, final_reparsed);
    }

    #[test]
    fn test_apply_jmespath_query_field_access() {
        let yaml_value: YamlValue = serde_yaml::from_str(
            r#"
            name: "test"
            count: 42
        "#,
        )
        .unwrap();

        let result = apply_jmespath_query(&yaml_value, "name").unwrap();
        assert_eq!(result, YamlValue::String("test".to_string()));
    }

    #[test]
    fn test_apply_jmespath_query_nested() {
        let yaml_value: YamlValue = serde_yaml::from_str(
            r#"
            database:
              host: "localhost"
              port: 5432
        "#,
        )
        .unwrap();

        let result = apply_jmespath_query(&yaml_value, "database.host").unwrap();
        assert_eq!(result, YamlValue::String("localhost".to_string()));
    }

    #[test]
    fn test_apply_jmespath_query_invalid_expression() {
        let yaml_value: YamlValue = serde_yaml::from_str("name: test").unwrap();
        let err = apply_jmespath_query(&yaml_value, "invalid[").unwrap_err();
        assert!(err.to_string().contains("Invalid JMESPath expression"));
        assert!(err.to_string().contains("invalid["));
    }
}
