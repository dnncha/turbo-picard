Fallback behavior
=================

By default, unsupported Picard commands fail clearly. This is intentional: a
pipeline should not silently change behavior.

For drop-in environments, configure an upstream Picard command prefix:

.. code-block:: bash

   export TURBO_PICARD_FALLBACK_COMMAND='java -jar /opt/picard/picard.jar'

or:

.. code-block:: bash

   export TURBO_PICARD_FALLBACK_COMMAND='mamba run -p /opt/conda/envs/picard picard'

What delegates
--------------

``turbo-picard`` delegates:

* unsupported Picard commands;
* explicitly unsupported native surfaces, such as options or formats that the
  native implementation recognizes as outside its current scope;
* JVM-style leading options, but only when fallback is configured.

What does not delegate
----------------------

Native I/O failures and malformed inputs are reported by the native command.
They are not sent to fallback, because doing so could hide real data or
environment problems.

Avoid fallback loops
--------------------

Prefer an absolute upstream Picard path or JAR path. A fallback value such as
``picard`` can resolve back to the ``turbo-picard`` shim if the shim shadows
upstream Picard on ``PATH``.
