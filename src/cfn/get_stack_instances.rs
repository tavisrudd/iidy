use crate::cli::GetStackInstancesArgs;

pub fn get_stack_instances(args: &GetStackInstancesArgs) -> ! {
    eprintln!(
        "The get-stack-instances command has been removed. Use the AWS CLI instead:\n  \
         aws ec2 describe-instances \\\n    \
         --filters \"Name=tag:aws:cloudformation:stack-name,Values={}\"",
        args.stackname
    );
    std::process::exit(1)
}
