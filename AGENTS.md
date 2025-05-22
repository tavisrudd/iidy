# Workflow Requirements
- For each task, create a folder codex/{your-branch-name}/ containing plan.org file containing entries `prompt & task`, `requirements` and `implementation-plan`. 
- Begin your work by reviewing your prompt and the existing related code, both in iidy-js/ and iidy/, then fill out a detailed set of `requirements` in plan.org. Use org-bable src blocks and sub-entries if appropriate.
- Then plan your work in `implementation-plan`. It should be a hierarchical outline and you will tick off each item as you complete it. Again use org-babel src blocks and other org syntax to help structure it.
- Commit plan.org once you have filled it out. Your code changes will come in subsequent commits.
- After you have completed a coding task, mark off your org todo list items in plan.org and review what is left to do and if all requirements have been met.
- Always create a branch/PR even if you get stuck.
- If you are asked to refine an existing branch or fix an error in your work, make sure to include the original plan.org, task description and series of commits in your PR. Do not squash.
  
# Development Notes
- This environment loses network access after setup, so don't rely on fetching
  crates or git repositories while running commands in subsequent tasks.
- The rust project we're working on is in iidy/
- The gitignored folder iidy-js is the js/typescript version we're porting from, provided for reference. When adding functionality to our rust version always study related parts of the old version in iidy-js first.
- If cargo build or cargo test takes more than 1 minute to run assume it's stuck, kill it, and move on without it.

