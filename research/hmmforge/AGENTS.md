# HMMForge engineering contract

This is an independent research package, not a Turbo Picard command. While it
lives in dnncha/turbo-picard on research/hmmforge-prototype, do not merge that
branch into main. The standalone repository target is dnncha/hmmforge (private
until release gates pass); never overwrite an existing repository.

Read README.md, docs/STUDY.md and the latest evidence report before optimising.
Run `python -m pytest -q` and `hmmforge-study` with the original thresholds before
making a performance claim. Normal annotation does not download data or phone
home. Preserve explicit errors, atomic no-clobber output and per-protein domZ.

Do not equate wall-time speedups with CPU savings or cloud bills. Do not describe
the direct baseline authored here as externally reviewed. Do not call the
512-protein full-model-library smoke test the 100,000-novel-protein adoption gate.
Retain failed or slower runs. Native sampling must have real nonempty samples;
phase timers and Python call profiles do not establish C-kernel bottlenecks.

Kernel development follows native profiles and the strongest practical baseline.
No GPU stubs, invented benchmark numbers or relaxed filters to manufacture a win.
