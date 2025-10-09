# iidy

`iidy-rs` is a Rust command-line tool for working with
Cloudformation. It is a port of the typescript version at
https://github.com/unbounce/iidy.


Claude and Codex wrote *all* the code, but with strict guidance and
review by @tavisrudd. This was an experiment to see how far I can take
`Vibe Engineering`.

The port is a WIP but it's almost complete and is well tested and
usable with the exception of iidy's custom AWS resources in the YAML
template system. That's an advanced feature not commonly used.  The
upstream documentation is still accurate for this version. A local
copy of the documentation and details of the improved functionality /
error reporting is coming in the near future.
