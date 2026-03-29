# RustGuard - Possible Future Improvements

This file captures potential product and engineering improvements for later consideration.   
It is intentionally not a task list and does not imply immediate implementation.

1) Privilege architecture hardening
   Consider moving privileged tunnel operations to a minimal dedicated helper/broker instead of running the full app with elevated permissions on Linux packages. This can reduce risk and improve security boundaries.

2) Linux tray behavior consistency
   Tray behavior varies by desktop environment and panel backend. A compatibility matrix (Mint/Cinnamon, GNOME variants, KDE, XFCE) plus backend-specific handling would improve predictability.

3) Notification UX consistency
   Standardize user feedback patterns (success/info/error) with a unified toast system, including non-blocking error toasts and actionable messages when operations fail.

4) Update-check configurability
   Allow the update source URL and policy to be managed by app configuration defaults per release while preserving user settings across upgrades.

5) Structured settings schema versioning
   Add explicit schema version metadata and migration logging so settings evolution is easier to audit and troubleshoot over time.

6) Observability and diagnostics
   Improve diagnostics around elevation path selection, tunnel command output, and environment detection to speed up support/debugging when behavior differs by distro/session type.

7) End-to-end desktop test coverage
   Add targeted integration/smoke tests for critical flows (settings save, connect/disconnect, tray open, startup behavior) to reduce regression risk.

8) Packaging and release ergonomics
   Continue refining Linux/Windows packaging parity, including clear post-install checks and optional verification scripts users can run after updates.
