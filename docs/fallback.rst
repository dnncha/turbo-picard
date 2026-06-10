Fallback behavior
=================

``turbo-picard`` targets full Picard 3.4.0 command compatibility. Commands
without a native fast path, plus unsupported options on accelerated commands,
are delegated to upstream Picard.

Auto-discovery
--------------

When ``TURBO_PICARD_FALLBACK_COMMAND`` is unset, ``turbo-picard`` tries to find
upstream Picard automatically:

* ``PICARD_JAR`` if set;
* ``$CONDA_PREFIX/share/picard-*/picard.jar``;
* an executable ``picard`` on ``PATH`` that is not the turbo-picard shim.

Disable auto-discovery with:

.. code-block:: bash

   export TURBO_PICARD_DISABLE_AUTO_FALLBACK=1

Explicit override
-----------------

For reproducible environments, set an absolute upstream command prefix:

.. code-block:: bash

   export TURBO_PICARD_FALLBACK_COMMAND='java -jar /opt/picard/picard.jar'

or:

.. code-block:: bash

   export TURBO_PICARD_FALLBACK_COMMAND='mamba run -p /opt/conda/envs/picard picard'

What delegates
--------------

``turbo-picard`` delegates:

* every Picard 3.4.0 command without a native fast path;
* explicitly unsupported native options or formats that the native
  implementation recognizes as outside its current scope;
* JVM-style leading options when upstream Picard is available.

Delegation keeps command compatibility. It does not replace the output parity
evidence in :doc:`parity` for accelerated commands you plan to switch.
Treat fallback as a compatibility bridge, not proof that every accelerated
surface is interchangeable with Picard. Unsupported surfaces remain visible so
operators can tell when a workflow is using upstream Picard instead of a native
turbo-picard fast path.

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
