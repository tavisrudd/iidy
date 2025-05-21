# iidy Command Reference

This document summarizes the CLI commands and options implemented in the original
`iidy-js` project. The Rust port uses `clap` to parse the same structure.

## Global Options

* `--environment, -e` – select environment settings (default `development`)
* `--color` – color output (`auto`, `always`, `never`)
* `--debug` – enable debug logging
* `--log-full-error` – print full stack traces on errors

## AWS Options

* `--client-request-token` – idempotency token passed to AWS APIs
* `--region` – AWS region
* `--profile` – AWS credentials profile
* `--assume-role-arn` – role ARN to assume

These options can appear before or after commands and apply globally.

## Commands

The table below lists each command with its positional arguments and supported
options. Commands such as `param` and `template-approval` have their own
subcommands.

### Stack Operations

- **create-stack `<argsfile>`**
  - `--stack-name` – override StackName from args file
  - `--lint-template` – lint template before executing

- **update-stack `<argsfile>`**
  - `--stack-name`
  - `--lint-template`
  - `--changeset` – create changeset for review
  - `--yes` – auto confirm when using `--changeset`
  - `--diff` – diff and review template changes
  - `--stack-policy-during-update <POLICY>` – temporary stack policy

- **create-or-update `<argsfile>`** – same options as `update-stack`.
- **estimate-cost `<argsfile>`** – estimate AWS costs. Option: `--stack-name`.
- **create-changeset `<argsfile>` `[changesetName]`**
  - `--watch`
  - `--watch-inactivity-timeout <SECS>`
  - `--description <TEXT>`
  - `--stack-name`
- **exec-changeset `<argsfile>` `<changesetName>`**
  - `--stack-name`
- **describe-stack `<stackname>`**
  - `--events <N>` – number of events to display
  - `--query <JMES>` – filter output
- **watch-stack `<stackname>`**
  - `--inactivity-timeout <SECS>`
- **describe-stack-drift `<stackname>`**
  - `--drift-cache <SECS>` – cache previous drift results
- **delete-stack `<stackname>`**
  - `--role-arn <ARN>` – role for the delete operation
  - `--retain-resources <ID>` (multiple)
  - `--yes` – confirm deletion
  - `--fail-if-absent` – exit with error if stack does not exist
- **get-stack-template `<stackname>`**
  - `--format <original|yaml|json>`
  - `--stage <Original|Processed>`
- **get-stack-instances `<stackname>`**
  - `--short` – only display DNS names
- **list-stacks**
  - `--tag-filter <key=value>` (multiple)
  - `--jmespath-filter <JMES>`
  - `--query <JMES>`
  - `--tags` – include tags in output

### Parameter Store Commands (`param`)

- **set `<path>` `<value>`**
  - `--message <TEXT>`
  - `--overwrite`
  - `--with-approval`
  - `--type <String|StringList|SecureString>`
- **review `<path>`** – review a pending change.
- **get `<path>`**
  - `--decrypt`
  - `--format <simple|yaml|json>`
- **get-by-path `<path>`**
  - `--decrypt`
  - `--format <simple|yaml|json>`
  - `--recursive`
- **get-history `<path>`**
  - `--format <simple|yaml|json>`
  - `--decrypt`

### Template Approval Commands (`template-approval`)

- **request `<argsfile>`**
  - `--lint-template` (default true)
  - `--lint-using-parameters`
- **review `<url>`**
  - `--context <LINES>` – diff context lines

### Miscellaneous Commands

- **render `<template>`**
  - `--outfile <FILE>`
  - `--format <yaml|json>`
  - `--query <JMES>`
  - `--overwrite`
- **get-import `<import>`**
  - `--format <yaml|json>`
  - `--query <JMES>`
- **demo `<script>`**
  - `--timescaling <FACTOR>`
- **lint-template `<argsfile>`**
  - `--use-parameters`
- **convert-stack-to-iidy `<stackname>` `<outputDir>`**
  - `--move-params-to-ssm`
  - `--sortkeys`
  - `--project <NAME>`
- **init-stack-args**
  - `--force`
  - `--force-stack-args`
  - `--force-cfn-template`
- **completion `[bash|elvish|fish|powershell|zsh]`** – generate a shell completion script

This reference is derived from the TypeScript version's `main.ts` and related
files to guide the Rust `clap` implementation.
