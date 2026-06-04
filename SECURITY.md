# Security

`turbo-picard` processes local sequencing files and writes local outputs. It is
not a network service, but malformed input files can still expose bugs such as
crashes, excessive memory use, or unsafe handling of temporary paths.

Please report security concerns privately first through GitHub private
vulnerability reporting, if it is available for the repository:

<https://github.com/dnncha/turbo-picard/security/advisories/new>

Include the affected version or commit, a short description of the issue, and a
minimal reproducer if you can share one safely. Do not send private clinical,
human-subject, or controlled-access data.

For ordinary crashes, incorrect outputs, documentation problems, or unsupported
Picard behavior, please use GitHub issues instead:

<https://github.com/dnncha/turbo-picard/issues>
