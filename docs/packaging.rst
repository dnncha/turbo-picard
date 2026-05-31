Packaging
=========

``turbo-picard`` is packaged with a conservative default: the main command does
not shadow upstream Picard.

Main package
------------

The main package installs:

``turbo-picard``
   Use this for evaluation, explicit workflow calls, and environments where
   upstream Picard must remain the default ``picard`` command.

Compatibility shim package
--------------------------

The optional shim package installs:

``picard``
   A compatibility entrypoint for workflow managers and scripts that already
   invoke Picard by command name.

Use the shim deliberately. It shadows upstream Picard wherever it appears first
on ``PATH``.

Conda-style deployment
----------------------

The repository includes Bioconda-oriented packaging files under
``packaging/bioconda``:

* ``turbo-picard`` for the explicit command;
* ``turbo-picard-picard-shim`` for the compatibility shim.

In shared environments, prefer installing the main package first, proving the
commands you need, and adding the shim only to pipeline-specific environments.
