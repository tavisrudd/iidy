# TODO

## CLI parity with iidy-js
- Implement remaining stack operation subcommands
  - create-stack
  - update-stack with changesets, diffing and stack-policy-during-update
  - create-or-update
  - estimate-cost
  - create-changeset and exec-changeset
  - describe/watch/delete stacks
  - stack drift detection
  - template retrieval and listing
  - parameter store subcommands
  - template approval workflow
  - rendering, get-import and demo helpers
  - lint-template, convert-stack-to-iidy and init-stack-args
  - shell completion generation

## YAML pre-processor
- Support `$imports` and `$defs`
- Implement `!$include` and `{{handlebars}}` templating
- Add custom tags such as `!$let`, `!$concat`, `!$map`, `!$merge`, etc.
- Enable custom resource templates with parameter validation
- Provide `filter` functionality for selective template extraction

## AWS integration
- Configure credentials via region/profile/assume-role
- Handle `client-request-token` and retry logic
- Sign S3 templates when needed
- Implement stack event watching and colored status output

## Parameter Store utilities
- `param set` with overwrite and approval options
- `param get` and `get-by-path` with decryption and format flags
- `param review` and `get-history` commands
- Support global configuration via SSM parameters (e.g. disable template approval)

## Template approval
- Request and review approval URLs
- Respect `/iidy/disable-template-approval` configuration

## Testing and examples
- Port test suite from `iidy-js` to Rust using `cargo test`
- Provide example stacks and demo scripts

## Documentation
- Expand README with usage instructions
- Document all CLI commands in `docs/cli-reference.md`
- Document YAML preprocessing features
