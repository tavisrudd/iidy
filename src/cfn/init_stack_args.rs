use std::path::Path;

use anyhow::Result;

use crate::cli::InitStackArgs;

const STACK_ARGS_TEMPLATE: &str = "# INITIALIZED STACK ARGS
# $imports:
#   environment: env:ENVIRONMENT

# REQUIRED SETTINGS:
StackName: <string>
Template: ./cfn-template.yaml
# optionally you can use the yaml pre-processor by prepending 'render:' to the filename
# Template: render:<local file path or s3 path>
# ApprovedTemplateLocation: s3://your-bucket/

# OPTIONAL SETTINGS:
# Region: <aws region name>
# Profile: <aws profile name>

# aws tags to apply to the stack
Tags:
#   owner: <your name>
#   environment: development
#   project: <your project>
#   lifetime: short

# stack parameters
Parameters:
#   key1: value
#   key2: value

# optional list. *Preferably empty*
Capabilities:
#   - CAPABILITY_IAM
#   - CAPABILITY_NAMED_IAM

NotificationARNs:
#   - <sns arn>

# CloudFormation ServiceRole
# RoleARN: arn:aws:iam::<acount>:role/<rolename>

# TimeoutInMinutes: <number>

# OnFailure defaults to ROLLBACK
# OnFailure: 'ROLLBACK' | 'DELETE' | 'DO_NOTHING'

# StackPolicy: <local file path or s3 path>

# see http://docs.aws.amazon.com/cli/latest/reference/cloudformation/create-stack.html#options
# ResourceTypes: <list of aws resource types allowed in the template>

# shell commands to run prior the cfn stack operation
# CommandsBefore:
#   - make build # for example";

const CFN_TEMPLATE: &str = "Dummy:
    Type: \"AWS::CloudFormation::WaitConditionHandle\"
    Properties: {}";

pub fn init_stack_args(args: &InitStackArgs) -> Result<i32> {
    init_stack_args_in(args, Path::new("."))
}

fn init_stack_args_in(args: &InitStackArgs, dir: &Path) -> Result<i32> {
    let force_stack_args = args.force || args.force_stack_args;
    let force_cfn_template = args.force || args.force_cfn_template;

    write_if_absent(
        dir,
        "stack-args.yaml",
        STACK_ARGS_TEMPLATE,
        force_stack_args,
    );
    write_if_absent(dir, "cfn-template.yaml", CFN_TEMPLATE, force_cfn_template);
    Ok(0)
}

fn write_if_absent(dir: &Path, filename: &str, content: &str, force: bool) {
    let path = dir.join(filename);
    if path.exists() && !force {
        println!("{filename} already exists! See help [-h] for overwrite options");
    } else {
        match std::fs::write(&path, content) {
            Ok(()) => println!("{filename} has been created!"),
            Err(e) => eprintln!("Failed to write {filename}: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_both_files() {
        let dir = tempfile::tempdir().unwrap();

        let args = InitStackArgs {
            force: false,
            force_stack_args: false,
            force_cfn_template: false,
        };
        let exit = init_stack_args_in(&args, dir.path()).unwrap();
        assert_eq!(exit, 0);

        let sa = std::fs::read_to_string(dir.path().join("stack-args.yaml")).unwrap();
        assert!(sa.contains("StackName: <string>"));
        assert!(sa.contains("Template: ./cfn-template.yaml"));

        let cfn = std::fs::read_to_string(dir.path().join("cfn-template.yaml")).unwrap();
        assert!(cfn.contains("AWS::CloudFormation::WaitConditionHandle"));
    }

    #[test]
    fn skips_existing_without_force() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("stack-args.yaml"), "existing").unwrap();
        std::fs::write(dir.path().join("cfn-template.yaml"), "existing").unwrap();

        let args = InitStackArgs {
            force: false,
            force_stack_args: false,
            force_cfn_template: false,
        };
        init_stack_args_in(&args, dir.path()).unwrap();

        let sa = std::fs::read_to_string(dir.path().join("stack-args.yaml")).unwrap();
        assert_eq!(sa, "existing");

        let cfn = std::fs::read_to_string(dir.path().join("cfn-template.yaml")).unwrap();
        assert_eq!(cfn, "existing");
    }

    #[test]
    fn force_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("stack-args.yaml"), "old").unwrap();
        std::fs::write(dir.path().join("cfn-template.yaml"), "old").unwrap();

        let args = InitStackArgs {
            force: true,
            force_stack_args: false,
            force_cfn_template: false,
        };
        init_stack_args_in(&args, dir.path()).unwrap();

        let sa = std::fs::read_to_string(dir.path().join("stack-args.yaml")).unwrap();
        assert!(sa.contains("StackName: <string>"));

        let cfn = std::fs::read_to_string(dir.path().join("cfn-template.yaml")).unwrap();
        assert!(cfn.contains("AWS::CloudFormation::WaitConditionHandle"));
    }

    #[test]
    fn force_individual_flags() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("stack-args.yaml"), "old-sa").unwrap();
        std::fs::write(dir.path().join("cfn-template.yaml"), "old-cfn").unwrap();

        // Only force stack-args
        let args = InitStackArgs {
            force: false,
            force_stack_args: true,
            force_cfn_template: false,
        };
        init_stack_args_in(&args, dir.path()).unwrap();

        let sa = std::fs::read_to_string(dir.path().join("stack-args.yaml")).unwrap();
        assert!(sa.contains("StackName: <string>"));
        let cfn = std::fs::read_to_string(dir.path().join("cfn-template.yaml")).unwrap();
        assert_eq!(cfn, "old-cfn");

        // Reset and only force cfn-template
        std::fs::write(dir.path().join("stack-args.yaml"), "old-sa").unwrap();
        let args = InitStackArgs {
            force: false,
            force_stack_args: false,
            force_cfn_template: true,
        };
        init_stack_args_in(&args, dir.path()).unwrap();

        let sa = std::fs::read_to_string(dir.path().join("stack-args.yaml")).unwrap();
        assert_eq!(sa, "old-sa");
        let cfn = std::fs::read_to_string(dir.path().join("cfn-template.yaml")).unwrap();
        assert!(cfn.contains("AWS::CloudFormation::WaitConditionHandle"));
    }
}
