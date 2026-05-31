Troubleshooting
===============

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
:doc:`commands`. Then run the closest parity script under ``tools/``. If your
workflow depends on a surface outside the native scope, route that command to
upstream Picard with fallback.

Index or md5 files are missing
------------------------------

Picard sidecars are controlled by command options such as ``CREATE_INDEX`` and
``CREATE_MD5_FILE``. Confirm those options are present and supported for the
command surface you are using.
