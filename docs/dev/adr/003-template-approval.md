# ADR-003: Template Approval Workflow

Status: Accepted
Date: 2025-07-01

## Context

Organizations deploying CloudFormation templates through iidy need a mechanism to gate
deployments on prior review. A reviewer must be able to inspect the exact template that
will be deployed -- after all YAML preprocessing, import resolution, and Handlebars
expansion -- confirm it is acceptable, and record that approval in a durable, auditable
location. The deployment tool must then verify the template hash against the approval record
before proceeding.

Approval must be asynchronous: the person requesting approval (e.g., a CI job or developer)
and the person granting it (e.g., a senior engineer or automated policy check) may act at
different times. The approval state must be durable across machines and processes.

## Decision

S3 is used as the approval store. The workflow is split into two commands:

**`template-approval request <argsfile>`**: Loads the stack-args file, runs the template
through the full preprocessing pipeline via `template_loader::load_cfn_template()`, computes
a SHA256 hash of the processed template body, and derives a versioned S3 key:
`{ApprovedTemplateLocation prefix}/{hash}{extension}`. If an object at that key already
exists (without the `.pending` suffix), the template is already approved and the command
reports this and exits cleanly. Otherwise, the processed template is uploaded to
`{key}.pending` with the `bucket-owner-full-control` ACL, and the command prints the review
URL. Optionally, the template is linted via CloudFormation validation before upload.

**`template-approval review <url>`**: Takes the `.pending` S3 URL, fetches both the pending
template and the current approved version (or treats the approved version as empty if none
exists), generates a unified diff using the `similar` crate, and presents it to the
reviewer. The reviewer confirms or declines via the standard `ConfirmationRequest`
infrastructure. On approval, the pending object is copied to the versioned key (removing
`.pending`), a `latest` copy is updated, and the `.pending` object is deleted.

Hashing is performed on the fully-processed template, not the raw source. This is the
critical invariant: the approved hash must match the hash of what will actually be deployed,
so the deployment tool can verify integrity by re-processing and re-hashing at deploy time.

The `similar` crate is used for diff generation in preference to shelling out to `git diff`,
which avoids a runtime dependency on git and works correctly in CI environments without a
git installation.

## Consequences

S3 as the approval store means: approvals are durable, accessible across machines, and can
be audited via S3 object versioning and access logs. The bucket must exist and the requesting
and reviewing IAM principals must have appropriate read/write permissions. Cross-account
scenarios require the `bucket-owner-full-control` ACL to be set on upload.

The hash-based versioning scheme means that two identical processed templates (regardless of
source formatting differences) share the same approval record. Conversely, any change to the
processed template -- including changes to imported files or resolved variables -- produces a
different hash and requires a new approval.

The `.pending` suffix convention is a naming contract. Any external tool interacting with the
approval bucket must respect it. There is no locking mechanism: concurrent requests for the
same template hash are idempotent (both write the same content to the same `.pending` key),
and concurrent approvals of the same key are safe because the copy-and-delete is not
transactional but the final state is convergent.

Output follows the standard data-driven architecture: `ApprovalRequestResult`,
`ApprovalStatus`, `TemplateDiff`, and `ApprovalResult` are `OutputData` variants, so
approval commands support interactive, plain, and JSON output modes without additional work.
