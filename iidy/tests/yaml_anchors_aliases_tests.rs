use anyhow::Result;
use iidy::yaml::preprocess_yaml_with_base_location;
use serde_yaml::Value;

/// Tests for YAML anchors, aliases, and merge key operations
/// 
/// YAML anchors (&) and aliases (*) are part of YAML 1.2 and should be handled by the parser
/// before our preprocessing pipeline runs.
/// 
/// YAML merge keys (<<) were part of YAML 1.1 but removed in YAML 1.2. Since serde_yaml 
/// follows YAML 1.2, we should detect merge key usage and provide helpful error messages.

#[tokio::test]
async fn test_basic_yaml_anchor_and_alias() -> Result<()> {
    let yaml_input = r#"
$defs:
  app_name: "test-app"

# Define an anchor
default_config: &default_config
  timeout: 30
  retries: 3
  debug: false

# Use the alias in preprocessing
service1:
  name: "{{app_name}}-service1"
  config: *default_config

service2:
  name: "{{app_name}}-service2" 
  config: *default_config
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    // Verify that anchors/aliases were resolved by the parser before preprocessing
    let service1 = result.get("service1").unwrap().as_mapping().unwrap();
    let service1_config = service1.get(&Value::String("config".to_string())).unwrap().as_mapping().unwrap();
    
    // The alias should be expanded to the full config
    assert_eq!(service1_config.get(&Value::String("timeout".to_string())), Some(&Value::Number(serde_yaml::Number::from(30))));
    assert_eq!(service1_config.get(&Value::String("retries".to_string())), Some(&Value::Number(serde_yaml::Number::from(3))));
    assert_eq!(service1_config.get(&Value::String("debug".to_string())), Some(&Value::Bool(false)));
    
    // Verify handlebars processing still worked
    assert_eq!(service1.get(&Value::String("name".to_string())), Some(&Value::String("test-app-service1".to_string())));
    
    // Verify service2 has the same config (from the same alias)
    let service2 = result.get("service2").unwrap().as_mapping().unwrap();
    let service2_config = service2.get(&Value::String("config".to_string())).unwrap().as_mapping().unwrap();
    assert_eq!(service2_config.get(&Value::String("timeout".to_string())), Some(&Value::Number(serde_yaml::Number::from(30))));
    
    Ok(())
}

#[tokio::test]
async fn test_yaml_merge_key_detection_and_error() -> Result<()> {
    let yaml_input = r#"
$defs:
  environment: "prod"

# Define base configuration with anchor
base_config: &base
  timeout: 30
  retries: 3
  region: "us-west-2"

# Use merge key - this should be detected and cause an error
prod_config:
  <<: *base
  timeout: 60
  environment: "{{environment}}"
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await;
    
    // This should now fail with a proper error message about merge keys
    assert!(result.is_err());
    let error = result.unwrap_err();
    let error_message = error.to_string();
    
    // Verify the error message contains the expected information
    assert!(error_message.contains("YAML merge keys ('<<') are not supported in YAML 1.2"));
    assert!(error_message.contains("in file 'test.yaml'"));
    assert!(error_message.contains("Consider using iidy's !$merge tag instead"));
    assert!(error_message.contains("!$merge"));
    
    println!("✅ Merge key properly detected with helpful error:");
    println!("{}", error_message);
    
    Ok(())
}

#[tokio::test]
async fn test_suggested_alternative_to_merge_keys() -> Result<()> {
    let yaml_input = r#"
$defs:
  app_name: "multi-app"

# Multiple base configurations
network_config: &network
  port: 8080
  host: "0.0.0.0"

security_config: &security  
  ssl: true
  auth_required: true

logging_config: &logging
  level: "INFO"
  format: "json"

# Alternative to merge keys: use !$merge tag to combine configurations
service_config: !$merge
  - *network
  - *security  
  - *logging
  - name: "{{app_name}}-service"
    port: 9090  # Override network port
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    let service_config = result.get("service_config").unwrap().as_mapping().unwrap();
    
    // Verify all configs were merged using iidy's !$merge tag
    assert_eq!(service_config.get(&Value::String("port".to_string())), Some(&Value::Number(serde_yaml::Number::from(9090)))); // overridden
    assert_eq!(service_config.get(&Value::String("host".to_string())), Some(&Value::String("0.0.0.0".to_string()))); // from network
    assert_eq!(service_config.get(&Value::String("ssl".to_string())), Some(&Value::Bool(true))); // from security
    assert_eq!(service_config.get(&Value::String("auth_required".to_string())), Some(&Value::Bool(true))); // from security
    assert_eq!(service_config.get(&Value::String("level".to_string())), Some(&Value::String("INFO".to_string()))); // from logging
    assert_eq!(service_config.get(&Value::String("format".to_string())), Some(&Value::String("json".to_string()))); // from logging
    
    // Verify handlebars processing worked
    assert_eq!(service_config.get(&Value::String("name".to_string())), Some(&Value::String("multi-app-service".to_string())));
    
    Ok(())
}

#[tokio::test]
async fn test_anchors_aliases_with_iidy_preprocessing_tags() -> Result<()> {
    let yaml_input = r#"
$defs:
  environment: "test"
  services: ["api", "web"]

# Define template with anchor
service_template: &service_template
  replicas: 2
  timeout: 30
  health_check: "/health"

# Use aliases with iidy preprocessing
service_configs: !$map
  items: !$ services
  template:
    name: "{{environment}}-{{item}}"
    config: *service_template  # Use alias in transformation
    port: !$if
      condition: !$eq ["{{item}}", "api"] 
      then: 8080
      else: 3000
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    let service_configs = result.get("service_configs").unwrap().as_sequence().unwrap();
    assert_eq!(service_configs.len(), 2);
    
    // Check first service (api)
    let api_service = service_configs[0].as_mapping().unwrap();
    assert_eq!(api_service.get(&Value::String("name".to_string())), Some(&Value::String("test-api".to_string())));
    assert_eq!(api_service.get(&Value::String("port".to_string())), Some(&Value::Number(serde_yaml::Number::from(8080))));
    
    let api_config = api_service.get(&Value::String("config".to_string())).unwrap().as_mapping().unwrap();
    assert_eq!(api_config.get(&Value::String("replicas".to_string())), Some(&Value::Number(serde_yaml::Number::from(2))));
    assert_eq!(api_config.get(&Value::String("timeout".to_string())), Some(&Value::Number(serde_yaml::Number::from(30))));
    assert_eq!(api_config.get(&Value::String("health_check".to_string())), Some(&Value::String("/health".to_string())));
    
    // Check second service (web)
    let web_service = service_configs[1].as_mapping().unwrap();
    assert_eq!(web_service.get(&Value::String("name".to_string())), Some(&Value::String("test-web".to_string())));
    assert_eq!(web_service.get(&Value::String("port".to_string())), Some(&Value::Number(serde_yaml::Number::from(3000))));
    
    Ok(())
}

#[tokio::test]
async fn test_nested_anchors_and_iidy_merge_alternative() -> Result<()> {
    let yaml_input = r#"
$defs:
  app_prefix: "myapp"

# Nested anchor definitions
database_defaults: &db_defaults
  engine: "postgresql"
  version: "13"
  settings: &db_settings
    max_connections: 100
    timeout: 30

cache_defaults: &cache_defaults
  engine: "redis"
  version: "6"
  settings: &cache_settings
    max_memory: "256mb"
    eviction_policy: "allkeys-lru"

# Alternative to nested merge keys: use !$merge with explicit structure
production_db: !$merge
  - *db_defaults
  - name: "{{app_prefix}}-prod-db"
    settings: !$merge
      - *db_settings
      - max_connections: 200  # Override nested setting
        backup_enabled: true

staging_cache: !$merge
  - *cache_defaults  
  - name: "{{app_prefix}}-staging-cache"
    settings: !$merge
      - *cache_settings
      - max_memory: "128mb"  # Override nested setting
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    // Verify production_db configuration
    let prod_db = result.get("production_db").unwrap().as_mapping().unwrap();
    assert_eq!(prod_db.get(&Value::String("engine".to_string())), Some(&Value::String("postgresql".to_string())));
    assert_eq!(prod_db.get(&Value::String("version".to_string())), Some(&Value::String("13".to_string())));
    assert_eq!(prod_db.get(&Value::String("name".to_string())), Some(&Value::String("myapp-prod-db".to_string())));
    
    let prod_db_settings = prod_db.get(&Value::String("settings".to_string())).unwrap().as_mapping().unwrap();
    assert_eq!(prod_db_settings.get(&Value::String("max_connections".to_string())), Some(&Value::Number(serde_yaml::Number::from(200))));
    assert_eq!(prod_db_settings.get(&Value::String("timeout".to_string())), Some(&Value::Number(serde_yaml::Number::from(30))));
    assert_eq!(prod_db_settings.get(&Value::String("backup_enabled".to_string())), Some(&Value::Bool(true)));
    
    // Verify staging_cache configuration
    let staging_cache = result.get("staging_cache").unwrap().as_mapping().unwrap();
    assert_eq!(staging_cache.get(&Value::String("engine".to_string())), Some(&Value::String("redis".to_string())));
    assert_eq!(staging_cache.get(&Value::String("name".to_string())), Some(&Value::String("myapp-staging-cache".to_string())));
    
    let cache_settings = staging_cache.get(&Value::String("settings".to_string())).unwrap().as_mapping().unwrap();
    assert_eq!(cache_settings.get(&Value::String("max_memory".to_string())), Some(&Value::String("128mb".to_string())));
    assert_eq!(cache_settings.get(&Value::String("eviction_policy".to_string())), Some(&Value::String("allkeys-lru".to_string())));
    
    Ok(())
}

#[tokio::test]
async fn test_anchors_in_sequences_and_arrays() -> Result<()> {
    let yaml_input = r#"
$defs:
  region: "us-west-2"

# Anchor in sequence
common_env_vars: &common_env
  - name: "ENVIRONMENT" 
    value: "production"
  - name: "REGION"
    value: "{{region}}"

# Use alias in different contexts
service1:
  name: "api-service"
  env_vars: *common_env

service2:
  name: "web-service"
  env_vars:
    - *common_env  # Nested alias
    - - name: "SERVICE_TYPE"
        value: "web"

# Use with iidy preprocessing  
all_services: !$map
  items: ["api", "web", "worker"]
  template:
    name: "{{item}}-service"
    env_vars: *common_env
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    // Verify service1 env vars
    let service1 = result.get("service1").unwrap().as_mapping().unwrap();
    let service1_env = service1.get(&Value::String("env_vars".to_string())).unwrap().as_sequence().unwrap();
    assert_eq!(service1_env.len(), 2);
    
    let env_var1 = service1_env[0].as_mapping().unwrap();
    assert_eq!(env_var1.get(&Value::String("name".to_string())), Some(&Value::String("ENVIRONMENT".to_string())));
    assert_eq!(env_var1.get(&Value::String("value".to_string())), Some(&Value::String("production".to_string())));
    
    let env_var2 = service1_env[1].as_mapping().unwrap();
    assert_eq!(env_var2.get(&Value::String("name".to_string())), Some(&Value::String("REGION".to_string())));
    assert_eq!(env_var2.get(&Value::String("value".to_string())), Some(&Value::String("us-west-2".to_string()))); // handlebars processed
    
    // Verify all_services from iidy preprocessing
    let all_services = result.get("all_services").unwrap().as_sequence().unwrap();
    assert_eq!(all_services.len(), 3);
    
    let api_service = all_services[0].as_mapping().unwrap();
    assert_eq!(api_service.get(&Value::String("name".to_string())), Some(&Value::String("api-service".to_string())));
    
    let api_env = api_service.get(&Value::String("env_vars".to_string())).unwrap().as_sequence().unwrap();
    assert_eq!(api_env.len(), 2);
    let api_region_var = api_env[1].as_mapping().unwrap();
    assert_eq!(api_region_var.get(&Value::String("value".to_string())), Some(&Value::String("us-west-2".to_string())));
    
    Ok(())
}

#[tokio::test]
async fn test_anchor_scope_and_ordering() -> Result<()> {
    let yaml_input = r#"
$defs:
  app_name: "scoped-app"

# Test that anchors work in the expected scope
section1:
  config: &section1_config
    timeout: 30
    retries: 3
    
  service:
    name: "{{app_name}}-service1"
    config: *section1_config

section2:
  # Should be able to reference anchor from section1
  service:
    name: "{{app_name}}-service2"  
    config: *section1_config
    
  # Define new anchor that could potentially conflict
  config: &section2_config
    timeout: 60
    retries: 5
    debug: true
    
  debug_service:
    name: "{{app_name}}-debug"
    config: *section2_config
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    // Verify section1 service uses section1_config
    let section1 = result.get("section1").unwrap().as_mapping().unwrap();
    let section1_service = section1.get(&Value::String("service".to_string())).unwrap().as_mapping().unwrap();
    let section1_config = section1_service.get(&Value::String("config".to_string())).unwrap().as_mapping().unwrap();
    assert_eq!(section1_config.get(&Value::String("timeout".to_string())), Some(&Value::Number(serde_yaml::Number::from(30))));
    
    // Verify section2 service uses the same section1_config  
    let section2 = result.get("section2").unwrap().as_mapping().unwrap();
    let section2_service = section2.get(&Value::String("service".to_string())).unwrap().as_mapping().unwrap();
    let section2_service_config = section2_service.get(&Value::String("config".to_string())).unwrap().as_mapping().unwrap();
    assert_eq!(section2_service_config.get(&Value::String("timeout".to_string())), Some(&Value::Number(serde_yaml::Number::from(30))));
    
    // Verify debug_service uses section2_config
    let debug_service = section2.get(&Value::String("debug_service".to_string())).unwrap().as_mapping().unwrap();
    let debug_config = debug_service.get(&Value::String("config".to_string())).unwrap().as_mapping().unwrap();
    assert_eq!(debug_config.get(&Value::String("timeout".to_string())), Some(&Value::Number(serde_yaml::Number::from(60))));
    assert_eq!(debug_config.get(&Value::String("debug".to_string())), Some(&Value::Bool(true)));
    
    // Verify handlebars worked in all services
    assert_eq!(section1_service.get(&Value::String("name".to_string())), Some(&Value::String("scoped-app-service1".to_string())));
    assert_eq!(section2_service.get(&Value::String("name".to_string())), Some(&Value::String("scoped-app-service2".to_string())));
    assert_eq!(debug_service.get(&Value::String("name".to_string())), Some(&Value::String("scoped-app-debug".to_string())));
    
    Ok(())
}