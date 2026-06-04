Fallback behavior
=================

By default, unsupported Picard commands fail clearly. This is intentional: a
pipeline should not silently change behavior.

For workflows that already call a ``picard`` command, configure an upstream
Picard command prefix before testing the shim:

.. code-block:: bash

   export TURBO_PICARD_FALLBACK_COMMAND='java -jar /opt/picard/picard.jar'

or:

.. code-block:: bash

   export TURBO_PICARD_FALLBACK_COMMAND='mamba run -p /opt/conda/envs/picard picard'

What delegates
--------------

``turbo-picard`` delegates:

* unsupported Picard commands;
* explicitly unsupported native options or formats that the
  native implementation recognizes as outside its current scope;
* JVM-style leading options, but only when fallback is configured.

Fallback is a compatibility bridge, not proof that a workflow is ready to
switch. Use it with the command coverage table and the parity guidance in
:doc:`parity` so unsupported surfaces remain visible while you test.

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
