# YAML Preprocessing Reference

iidy's YAML preprocessing engine transforms YAML documents through a set
of custom tags and an import system. It is used for both stack-args files
and CloudFormation templates.

For the engine architecture, see [dev/architecture.md](dev/architecture.md).
For import security restrictions, see [SECURITY.md](SECURITY.md).

## Document structure

A preprocessed YAML document has an optional header and a body:

```yaml
$imports:
  vpc: ./vpc-outputs.yaml
  config: env:APP_CONFIG

$defs:
  region: us-east-1
  prefix: !$join ["-", [myapp, !$ region]]

# Everything below $imports/$defs is the body
StackName: !$ prefix
Template: template.yaml
Region: !$ region
Parameters:
  VpcId: !$ vpc.VpcId
```

### `$imports`

Key-value pairs where the key becomes a variable name and the value is an
import location string. Import paths support Handlebars interpolation:
`config-{{ environment }}.yaml`. Imports are loaded and recursively
preprocessed before the body is resolved.

### `$defs`

Local variable definitions with let* semantics -- each definition can
reference prior definitions. Values are resolved in order.

### `$envValues`

Runtime values injected by `load_stack_args()` (not user-defined). Provides
legacy bare values (`region`, `environment`) and namespaced values
(`iidy.command`, `iidy.environment`, `iidy.region`, `iidy.profile`)
accessible in Handlebars expressions.

## Variable lookup

### `!$` / `!$include`

Look up a value from the environment (imports + defs).

```yaml
name: !$ config.appName
nested: !$ vpc.Outputs.SubnetId
full_object: !$include config
```

Dot notation traverses nested objects. `!$` and `!$include` are
interchangeable.

## Control flow

### `!$if`

Conditional expression. Evaluates `test`, returns `then` if truthy,
`else` otherwise.

```yaml
mode: !$if
  test: !$eq [!$ environment, production]
  then: multi-az
  else: single-az
```

### `!$let`

Introduces local variable bindings for an expression. Uses a flat format
where all keys except `in` are treated as bindings, and `in` holds the
expression to evaluate.

```yaml
result: !$let
  x: 10
  y: 20
  in: !$join ["-", [!$ x, !$ y]]
```

## Collection transforms

### `!$map`

Transforms each element in a list using a template expression.

```yaml
prefixed: !$map
  items: [a, b, c]
  var: item
  template: !$join ["-", [prefix, !$ item]]
```

Optional `filter` key to include only items where the filter is truthy.

### `!$concat`

Concatenates multiple sequences into one.

```yaml
all: !$concat
  - [a, b]
  - [c, d]
  - !$ moreItems
```

### `!$merge`

Deep-merges multiple mappings. Later values override earlier ones.

```yaml
config: !$merge
  - {port: 80, host: localhost}
  - {port: 443}
  # Result: {port: 443, host: localhost}
```

### `!$concatMap`

Maps each item to a sequence, then concatenates all results. Uses the
same `{items, template, var, filter}` format as `!$map`.

```yaml
flattened: !$concatMap
  items: [[1, 2], [3, 4]]
  var: group
  template: !$ group
```

### `!$mergeMap`

Maps each item to a mapping, then merges all results. Same format as
`!$map`.

```yaml
combined: !$mergeMap
  items: [a, b, c]
  var: name
  template:
    !$ name:
      enabled: true
```

### `!$mapListToHash`

Maps each item to a key-value pair mapping, then merges. Same format as
`!$map`. The template must produce a single-key mapping.

```yaml
lookup: !$mapListToHash
  items: [web, api, worker]
  var: svc
  template:
    !$ svc: !$join ["-", [!$ svc, service]]
```

### `!$mapValues`

Transforms each value in a mapping, preserving keys.

```yaml
uppercased: !$mapValues
  source:
    a: hello
    b: world
  var: val
  template: !$join ["", [!$ val, "!"]]
```

### `!$groupBy`

Groups items by a key expression.

```yaml
grouped: !$groupBy
  items:
    - {name: a, type: web}
    - {name: b, type: api}
    - {name: c, type: web}
  var: item
  key: !$ item.type
```

Note: current implementation uses HashMap, so group ordering is
non-deterministic.

### `!$fromPairs`

Converts a list of `[key, value]` pairs into a mapping.

```yaml
obj: !$fromPairs
  - [name, myapp]
  - [version, "1.0"]
# Result: {name: myapp, version: "1.0"}
```

## String operations

### `!$join`

Joins a list of strings with a separator.

```yaml
name: !$join ["-", [myapp, !$ environment, !$ region]]
```

### `!$split`

Splits a string by a delimiter. Takes a two-element array: `[delimiter, string]`.

```yaml
parts: !$split ["-", "a-b-c"]
```

### `!$toYamlString`

Serializes a value to a YAML string.

```yaml
yamlText: !$toYamlString
  key: value
  list: [1, 2, 3]
```

### `!$parseYaml`

Parses a YAML string into a structured value.

```yaml
parsed: !$parseYaml "key: value\nlist:\n  - 1\n  - 2"
```

### `!$toJsonString`

Serializes a value to a JSON string.

```yaml
jsonText: !$toJsonString
  key: value
```

### `!$parseJson`

Parses a JSON string into a structured value.

```yaml
parsed: !$parseJson '{"key": "value"}'
```

## Comparison

### `!$eq`

Tests equality of two values. Returns a boolean.

```yaml
isProd: !$eq [!$ environment, production]
```

### `!$not`

Logical negation.

```yaml
isNotProd: !$not
  - !$eq [!$ environment, production]
```

## Escaping

### `!$escape`

Prevents preprocessing of its contents. The inner value passes through
without tag resolution.

```yaml
literal: !$escape
  !$map:
    items: [1, 2, 3]
```

## Import types

### `file`

Load from the local filesystem. Paths are resolved relative to the
importing document.

```yaml
$imports:
  config: ./config.yaml
  absolute: file:/etc/iidy/defaults.yaml
```

### `env`

Read an environment variable. Supports a default value after a second colon.

```yaml
$imports:
  home: env:HOME
  optional: env:MISSING_VAR:default-value
```

### `git`

Read git repository information.

```yaml
$imports:
  branch: git:branch
  sha: git:sha
  short: git:short
```

### `random`

Generate random values.

```yaml
$imports:
  uuid: random:dashed-name
  hex: random:hex
```

### `filehash` / `filehash-base64`

Compute SHA256 hash of a file's contents. Prefix with `?` to allow missing
files (returns empty string).

```yaml
$imports:
  hash: filehash:./template.yaml
  b64hash: filehash-base64:./template.yaml
  optional: filehash:?./maybe-missing.yaml
```

### `s3`

Load from an S3 bucket. Requires AWS credentials.

```yaml
$imports:
  remote: s3://my-bucket/configs/shared.yaml
```

### `http` / `https`

Fetch from an HTTP endpoint.

```yaml
$imports:
  api: https://config-api.example.com/v1/config.json
```

### `cfn`

Read CloudFormation stack outputs or exports.

```yaml
$imports:
  vpcStack: cfn:us-east-1:vpc-stack
```

### `ssm`

Read an SSM Parameter Store parameter. Supports `:json` or `:yaml` suffix
to parse the value.

```yaml
$imports:
  dbHost: ssm:/app/config/database-host
  dbConfig: ssm:/app/config/database:json
```

### `ssm-path`

Read all SSM parameters under a path prefix.

```yaml
$imports:
  allConfig: ssm-path:/app/config
```

## Handlebars interpolation

Scalar string values containing `{{ }}` expressions are processed through
Handlebars. The environment (imports + defs + envValues) is available as
context.

```yaml
$defs:
  app: myapp
name: "{{ app }}-{{ environment }}"
```

Available helpers include string manipulation (`toUpperCase`, `toLowerCase`,
`replace`, `trim`, `substring`, `length`, `pad`, `concat`, `capitalize`,
`titleize`), case conversion (`camelCase`, `snakeCase`, `kebabCase`,
`pascalCase`), encoding (`base64`, `urlEncode`, `sha256`, `filehash`,
`filehashBase64`), serialization (`toJson`, `toJsonPretty`, `toYaml`),
and object access (`lookup`).

## CloudFormation tag pass-through

CloudFormation intrinsic function tags (`!Ref`, `!Sub`, `!GetAtt`, `!Join`,
`!Select`, `!Split`, `!If`, `!Equals`, `!And`, `!Or`, `!Not`,
`!FindInMap`, `!ImportValue`, `!Base64`, `!Cidr`, `!GetAZs`, `!Length`,
`!ToJsonString`, `!Transform`, `!ForEach`) are
recognized by the parser. Their inner content is preprocessed (so you can
use `!$` lookups inside `!Sub`), then they pass through to output as
tagged YAML values:

```yaml
Resources:
  MyBucket:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: !Sub "${AWS::StackName}-!$ bucketSuffix"
      Tags:
        - Key: Environment
          Value: !Ref EnvironmentParam
```
