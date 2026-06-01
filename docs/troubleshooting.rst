Troubleshooting
===============

Start by separating the two entrypoints. If you are debugging a difference,
run the explicit ``turbo-picard`` binary first and keep the compatibility
``picard`` shim out of ``PATH`` until you know the command surface behaves the
way your workflow needs.

Unsupported command
-------------------

If a command is unsupported, either use upstream Picard directly or configure
fallback:

.. code-block:: bash

   export TURBO_PICARD_FALLBACK_COMMAND='java -jar /opt/picard/picard.jar'

Then rerun the original command through the shim.

Fallback appears to recurse
---------------------------

Use an absolute upstream Picard command or JAR path. Avoid setting fallback to a
bare ``picard`` command when the ``turbo-picard`` shim appears first on
``PATH``.

Output differs from Picard
--------------------------

Check whether the command surface is documented as native or partially native in
:doc:`commands`. Then run the closest parity script under ``tools/`` and compare
the exact files your workflow consumes. For metrics-producing commands, compare
the metrics text first; chart PDFs are lightweight compatibility sidecars unless
the command documentation says otherwise.

If your workflow depends on behavior outside the native scope, route that
command to upstream Picard with fallback. Keep the failing input, command line,
Picard version, ``turbo-picard`` version, and output diff together so the
mismatch can become a regression test or a pinned real-data comparison.

Index or md5 files are missing
------------------------------

Picard sidecars are controlled by command options such as ``CREATE_INDEX`` and
``CREATE_MD5_FILE``. Confirm those options are present and supported for the
command surface you are using.

Bioconda recipe still uses source.path
--------------------------------------

That is expected before release. The local ``source.path`` block is for smoke
testing this repository's recipes. Do not copy those recipes into
``bioconda-recipes`` until ``tools/prepare_bioconda_release.py`` has replaced
the local source with the tagged GitHub archive URL and SHA-256, and
``python3 tools/verify_bioconda_recipes.py --release-ready`` passes.
