# turbo-picard-picard-shim

Opt-in Bioconda shim package that installs the `picard` command name for
`turbo-picard`. The main `turbo-picard` package does not install this binary so
it can coexist with upstream Picard.

Keep this recipe separate from the main package. It intentionally declares a
constraint against upstream `picard` because both packages expose the same
command name, and workflow owners should opt into that shadowing behavior.
