# StatusIndicator contrast support

StatusIndicator now includes explicit high-contrast overrides for users who prefer more contrast. When the operating system or browser exposes `prefers-contrast: more`, the indicator uses a stronger border and text color combination to maintain visibility and WCAG-friendly clarity.

## Notes
- The override is implemented in `src/styles/contrast.css`.
- The component remains compatible with existing state-based styling tokens.
- The change is covered by a focused regression test in `tests/status-indicator.test.ts`.
